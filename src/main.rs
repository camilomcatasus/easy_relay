use::easy_relay::broadcast_relay_server::{BroadcastRelayServer, BroadcastRelayServerConfig};
use::easy_relay::relay_server::RelayServer;

#[tokio::main]
async fn main() {
    let relay_server = BroadcastRelayServer::new(BroadcastRelayServerConfig::default());
    BroadcastRelayServer::run_relay_server(relay_server).await;
}
