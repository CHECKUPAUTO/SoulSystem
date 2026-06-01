//! SoulLink Proxy — TLS reverse proxy for SoulLink V13.5 APIs.
//!
//! Usage: soullink-proxy [config.toml]

use soullink_proxy::ProxyConfig;
use soullink_proxy::ProxyServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("soullink_proxy=info".parse()?),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/soullink/proxy.toml".to_string());

    let config = if std::path::Path::new(&config_path).exists() {
        let content = std::fs::read_to_string(&config_path)?;
        toml::from_str(&content)?
    } else {
        tracing::info!("No config file at {}, using defaults", config_path);
        ProxyConfig::default()
    };

    let server = ProxyServer::new(config);
    server.run().await
}
