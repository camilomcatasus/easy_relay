
pub mod simple_relay_server;
pub mod broadcast_relay_server;
pub mod relay_server;
pub mod error;




    /*
    let relay_server = Arc::new(RelayServer {
        connections: Arc::new(Mutex::new(HashMap::new())),
    });

    let relay_server_filter = warp::any().map(move || relay_server.clone());

    // Route for the server (host) to connect
    let server_route = warp::path!("ws" / "server")
        .and(warp::ws())
        .and(relay_server_filter.clone())
        .map(|ws: warp::ws::Ws, relay_server: Arc<RelayServer>| {
            ws.on_upgrade(move |websocket| async move {
                // Handle host connection
                let code = relay_server.handle_host_connection(websocket).await;
                println!("Host connected with code: {}", code);
            })
        });

    // Route for the client to connect with a specific code
    let client_route = warp::path!("ws" / "client" / String)
        .and(warp::ws())
        .and(relay_server_filter)
        .map(|code: String, ws: warp::ws::Ws, relay_server: Arc<RelayServer>| {
            ws.on_upgrade(move |websocket| async move {
                // Handle client connection
                relay_server.handle_peer_connection(code, websocket).await;
            })
        });

    // Combine the routes
    let routes = server_route.or(client_route);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
    */


