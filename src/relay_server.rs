use warp::ws::WebSocket;
use crate::error::RelayServerError;

pub trait RelayServer {
    async fn handle_host_connection(&self, ws: WebSocket) -> Result<String, RelayServerError>;
    async fn handle_peer_connection(&self, code: String, ws: WebSocket);
    async fn run_relay_server(relay_server: Self);
}
