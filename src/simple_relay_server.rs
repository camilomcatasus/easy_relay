
use std::collections::HashMap;
use log::info;
use warp::{
    Filter,
    ws::{
        WebSocket,
        Message,
    }
};
use tokio::sync::Mutex;
use std::sync::Arc;
use rand::Rng;
use futures_util::{SinkExt, StreamExt};

use crate::error::RelayServerError;

#[derive(Clone)]
struct SimpleRelayServer{
    config: SimpleRelayServerConfig,
    connections: Arc<Mutex<HashMap<String, WebSocket>>>,
    connection_count: Arc<Mutex<usize>>,
}

use crate::relay_server::RelayServer;

#[derive(Clone)]
pub struct SimpleRelayServerConfig {
    pub max_connections: Option<usize>,
    pub port: usize,
}

impl Default for SimpleRelayServerConfig {
    fn default() -> Self {
        SimpleRelayServerConfig {
            max_connections: Some(10),
            port: 3030
        }
    }
}


impl SimpleRelayServer {

    async fn short_unique_code(&self) -> String {
        let connections = self.connections.lock().await;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        const CODE_LENGTH: usize = 6;

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
}

impl RelayServer for SimpleRelayServer{
    async fn handle_host_connection(&self, mut ws: WebSocket) -> Result<String, RelayServerError> {
        // Logic for handling the host
        let code = self.short_unique_code().await;

        let _ = ws.send(warp::ws::Message::text(code.clone())).await;


        let connection_count = self.connection_count.lock().await;


        if self.config.max_connections.is_some() && connection_count.lt(&self.config.max_connections.unwrap()) {

            self.connections.lock().await.insert(code.clone(), ws);
            return Ok(code);
        }
        else {
            return Err(RelayServerError::ConnectionAmountExceeded);
        }
    }

    async fn handle_peer_connection(&self, code: String, mut ws: WebSocket) {
        // Logic for handling the peer connection
        let mut connections = self.connections.lock().await;
        if let Some(host_ws) = connections.remove(&code) {
            let (mut host_tx, mut host_rx) = host_ws.split();
            let (mut peer_tx, mut peer_rx) = ws.split();

            let client_code = code.clone();
            // Relay messages between host and peer
            tokio::spawn(async move {
                while let Some(msg) = peer_rx.next().await {
                    if let Ok(msg) = msg {
                        host_tx.send(msg).await.unwrap();
                    }
                }

                info!("Client disconnected from room {}", client_code);
                host_tx.send(Message::text("Client Disconnected")).await.unwrap();
            });

            tokio::spawn(async move {
                while let Some(msg) = host_rx.next().await {
                    if let Ok(msg) = msg {
                        peer_tx.send(msg).await.unwrap();
                    }
                }

                info!("Host disconnected from room {}", code);
                peer_tx.send(Message::text("Host Disconnected")).await.unwrap();
            });
        } else {
            log::error!("Peer attempted to connect to invalid room: {}", code);
            ws.send(warp::ws::Message::text("Invalid code")).await.unwrap();
        }
    }

    async fn run_relay_server(relay_server: Self) {
        

        let relay_server_filter = warp::any().map(move || relay_server.clone());

        // Route for the server (host) to connect
        let server_route = warp::path!("ws" / "server")
            .and(warp::ws())
            .and(relay_server_filter.clone())
            .map(|ws: warp::ws::Ws, relay_server: SimpleRelayServer| {
                ws.on_upgrade(move |websocket| async move {
                    // Handle host connection
                    match relay_server.handle_host_connection(websocket).await {
                        Ok(code) => info!("Host connected with code: {}", code),
                        Err(err) => log::error!("Error ocurred trying to process new host connection: {:?}", err)
                    }
                })
            });

        // Route for the client to connect with a specific code
        let client_route = warp::path!("ws" / "client" / String)
            .and(warp::ws())
            .and(relay_server_filter)
            .map(|code: String, ws: warp::ws::Ws, relay_server: SimpleRelayServer| {
                ws.on_upgrade(move |websocket| async move {
                    // Handle client connection
                    relay_server.handle_peer_connection(code, websocket).await;
                })
            });

        // Combine the routes
        let routes = server_route.or(client_route);

        warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
    }
}
