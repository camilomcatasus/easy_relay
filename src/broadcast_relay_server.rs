use rand::Rng;
use warp::{ 
    ws::{
        WebSocket,
        Message
    },
    Filter
};

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use std::{collections::HashMap, error::Error};
use tokio::{
    sync::{
        mpsc::Sender, Mutex
    },
};

use std::sync::Arc;
use crate::relay_server::RelayServer;
use tungstenite::Error as TsError;
type WebSocketSinks = Arc<Mutex<Vec<SplitSink<WebSocket, Message>>>>;

struct Room {
    pub client_index: u16,
    pub client_sinks: WebSocketSinks,
    pub host_sender: Sender<MessageWrapper>,
}

#[derive(Clone)]
pub struct BroadcastRelayServer {
    config: BroadcastRelayServerConfig,
    connections: Arc<Mutex<HashMap<String, Room>>>,
}


async fn short_unique_code(relay_server: &BroadcastRelayServer) -> String {
    let connections = relay_server.connections.lock().await;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const CODE_LENGTH: usize = 8;

    let mut rng = rand::thread_rng();
    let mut code: String = (0..CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    while connections.contains_key(&code) {
        code = (0..CODE_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
    }

    code
}

impl BroadcastRelayServer {
    pub fn new(config: BroadcastRelayServerConfig) -> Self {
        BroadcastRelayServer {
            config,
            connections: Arc::new(Mutex::new(HashMap::new()))
        }
    }
}

struct MessageWrapper {
    message: Message,
    client_id: u16,
}

// TODO: Fix race condition, code is generated and returns, meanwhile another new code is generated
// these two codes are the same, the program will attempt to add them both to the map.
impl RelayServer for BroadcastRelayServer {

    async fn handle_host_connection(&self, mut websocket: WebSocket) -> Result<String, crate::error::RelayServerError> {
        println!("Start handling new host connection");
        let code = short_unique_code(self).await;

        {
            if (self.config.max_room_connections as usize) < self.connections.lock().await.capacity() {
                //TODO: Maybe send an error message
                let _ = websocket.send(warp::ws::Message::close()).await;
                return Err(crate::error::RelayServerError::ConnectionAmountExceeded);
            }
        }
        let _ = websocket.send(warp::ws::Message::text(code.clone())).await;

        println!("Created new room with code: {}", &code);

        let client_sinks: WebSocketSinks = Arc::new(Mutex::new(Vec::new()));
        let (host_sender, mut host_receiver) = tokio::sync::mpsc::channel::<MessageWrapper>(32);

        {
            let _ = self.connections.lock().await.insert(code.clone(), Room {
                client_index: 1u16,
                client_sinks: client_sinks.clone(),
                host_sender
            });
        }


        let mut confirmation_message : Vec<u8> = vec![0b0000_1111];
        confirmation_message.push(code.len() as u8);
        confirmation_message.append(&mut code.clone().into_bytes());

        let _confirmation_send_result = websocket.send(Message::binary(confirmation_message)).await;

        let (mut host_sink, mut host_stream) = websocket.split();

        let connections_clone = self.connections.clone();
        let moved_code = code.clone();

        tokio::spawn(async move {
            while let Some(Ok(message)) = host_stream.next().await {
                let mut unlocked_client_sinks = client_sinks.lock().await;
                for client_sink in unlocked_client_sinks.iter_mut() {
                    let _client_send_result = client_sink.send(message.clone()).await;
                }
            }

            let _ = connections_clone.lock().await.remove(&moved_code);
            println!("Closed room {}", &moved_code);
        });

        tokio::spawn(async move {
            while let Some(message_wrapper) = host_receiver.recv().await {
                let mut client_message_bytes = message_wrapper.message.into_bytes();
                client_message_bytes.append(&mut message_wrapper.client_id.to_le_bytes().to_vec());
                let identified_message = Message::binary(client_message_bytes);
                match host_sink.send(identified_message).await {
                    Ok(_) => (),
                    Err(err) => {
                        println!("Error happened sending message {:?}", err);
                        if let Some(tung_err) = err.source().and_then(|source| source.downcast_ref::<TsError>()) {
                            match tung_err {
                                TsError::ConnectionClosed => host_receiver.close(),
                                TsError::AlreadyClosed => host_receiver.close(),
                                _ => ()
                            }
                        }
                    }
                }
            }
        });

        Ok(code)
    }

    async fn handle_peer_connection(&self, code: String, mut websocket: WebSocket) {

        if let Some(room) = self.connections.lock().await.get_mut(&code) {
            let client_id = room.client_index;
            room.client_index += 1;

            let mut connection_response: Vec<u8> = Vec::new();
            connection_response.extend_from_slice(&client_id.to_le_bytes());
            let _ = websocket.send(Message::binary(connection_response)).await;
            let (client_sink, mut client_stream) = websocket.split();
            println!("Client connected using valid code: {}", code);
            let host_sender = room.host_sender.clone();
            // Send client messages to host thread through channel
            tokio::spawn(async move {
                while let Some(Ok(message)) = client_stream.next().await {

                    if !message.is_close() {
                        let wrapped_messaged = MessageWrapper {
                            message,
                            client_id,
                        } ;
                        let _client_send_result = host_sender.send(wrapped_messaged).await;
                    }

                    if host_sender.is_closed() {
                        break;
                    }
                }

                println!("Client disconnected");
            });

            // Add receiver to room
            room.client_sinks.lock().await.push(client_sink);
        } else {
            print!("Client attempted to connect using invalid code: {}", code);
            websocket.send(warp::ws::Message::text("Invalid code")).await.unwrap();
            let _ = websocket.close().await;
        }
    }

    async fn run_relay_server(relay_server: Self) {
        
        let relay_server_clone = relay_server.clone();
        let filter = warp::any().map(move || relay_server_clone.clone());

        let server_route = warp::path!("ws" / "host")
            .and(warp::ws())
            .and(filter.clone())
            .map(|ws: warp::ws::Ws, broadcast_relay_server: BroadcastRelayServer| {
                println!("Host connection started");
                ws.on_upgrade(move |websocket| async move {
                    let _ = broadcast_relay_server.handle_host_connection(websocket).await;

                })

            });

        let client_route = warp::path!("ws" / "client" / String)
            .and(warp::ws())
            .and(filter)
            .map(|code: String, ws: warp::ws::Ws, broadcast_relay_server: BroadcastRelayServer| {
                println!("Client attempting to connect to room {}", &code);
                ws.on_upgrade(move |websocket| async move {
                    broadcast_relay_server.handle_peer_connection(code, websocket).await;
                })

            });

        let routes = server_route.or(client_route);

        warp::serve(routes).run(([127, 0, 0, 1], relay_server.config.port)).await;
        }
}


#[derive(Clone)]
pub struct BroadcastRelayServerConfig
{
    pub max_rooms: u16,
    pub max_room_connections: u16,
    pub port: u16,
}

impl Default for BroadcastRelayServerConfig 
{
    fn default() -> Self {
        BroadcastRelayServerConfig {
            max_rooms: 100,
            max_room_connections: 20,
            port: 3030,
        }
    }
}
