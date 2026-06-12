//! Proxy server — reverse proxy for SoulLink APIs.
//!
//! Routes:
//!   /api/mesh/*     → localhost:9020 (Orchestrator)
//!   /api/rag/*      → localhost:9020 (RAG)
//!   /api/autonomy/* → localhost:9046 (Autonomy)
//!   /monitoring/*   → localhost:9091 (Guardian)
//!   /health         → proxy health check

use crate::config::ProxyConfig;
use crate::routes::{health_check, proxy_handler, AppState};
use crate::tls;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

pub struct ProxyServer {
    config: ProxyConfig,
}

impl ProxyServer {
    pub fn new(config: ProxyConfig) -> Self {
        Self { config }
    }

    fn build_router(&self) -> Router {
        let state = Arc::new(AppState::new(self.config.clone()));

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        Router::new()
            .route("/health", get(health_check))
            .fallback(proxy_handler)
            .with_state(state)
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http())
            .layer(cors)
    }

    /// Run the proxy server.
    pub async fn run(self) -> anyhow::Result<()> {
        let router = self.build_router();
        let addr = self.config.bind_addr;

        if self.config.tls_enabled {
            let cert_path = self
                .config
                .cert_path
                .as_deref()
                .unwrap_or("/var/lib/soullink/proxy-cert.pem");
            let key_path = self
                .config
                .key_path
                .as_deref()
                .unwrap_or("/var/lib/soullink/proxy-key.pem");

            let (cert_pem, key_pem) = tls::load_or_generate_tls(
                std::path::Path::new(cert_path),
                std::path::Path::new(key_path),
            )?;

            info!(addr = %addr, "Starting SoulLink proxy with TLS");
            let tls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem, key_pem).await?;
            axum_server::bind_rustls(addr, tls_config)
                .serve(router.into_make_service())
                .await?;
        } else {
            info!(addr = %addr, "Starting SoulLink proxy (HTTP)");
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, router).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_router_succeeds() {
        let config = ProxyConfig::default();
        let server = ProxyServer::new(config);
        let _router = server.build_router();
    }

    #[test]
    fn config_roundtrip() {
        let config = ProxyConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.backends.len(), 4);
        assert_eq!(parsed.bind_addr.port(), 8443);
    }
}
