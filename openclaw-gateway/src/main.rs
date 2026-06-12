use tracing::info;

mod config;
mod gateway;
mod protocol;
mod session;
mod auth;
mod providers;

use config::GatewayConfig;
use gateway::GatewayServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = GatewayConfig::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(config.log_level.clone())
        .init();

    std::panic::set_hook(Box::new(|panic| {
        tracing::error!("Panic occurred: {:?}", panic);
    }));

    info!("🦀 OpenClaw Gateway Rust v{} starting...", env!("CARGO_PKG_VERSION"));
    info!("Configuration: port={}, workers={}", config.port, config.workers);

    let server = GatewayServer::new(config);
    server.run().await?;

    Ok(())
}
