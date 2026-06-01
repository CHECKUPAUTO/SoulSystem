//! Proxy configuration — routes, TLS settings, auth.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// A backend service to proxy to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    /// Route prefix (e.g., "/api/mesh")
    pub prefix: String,
    /// Backend URL (e.g., "http://127.0.0.1:9020")
    pub url: String,
    /// Health check path (e.g., "/api/mesh/status")
    pub health: String,
}

/// Full proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Address to bind (e.g., "0.0.0.0:8443")
    pub bind_addr: SocketAddr,
    /// Backend services
    pub backends: Vec<Backend>,
    /// Enable TLS (self-signed cert if no cert provided)
    pub tls_enabled: bool,
    /// Path to TLS certificate (PEM). If empty, auto-generate.
    pub cert_path: Option<String>,
    /// Path to TLS private key (PEM). If empty, auto-generate.
    pub key_path: Option<String>,
    /// API token for authentication (Bearer token)
    pub auth_token: Option<String>,
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:8443".parse().unwrap(),
            backends: vec![
                Backend {
                    prefix: "/api/mesh".to_string(),
                    url: "http://127.0.0.1:9020".to_string(),
                    health: "/api/mesh/status".to_string(),
                },
                Backend {
                    prefix: "/api/rag".to_string(),
                    url: "http://127.0.0.1:9020".to_string(),
                    health: "/api/rag/health".to_string(),
                },
                Backend {
                    prefix: "/api/autonomy".to_string(),
                    url: "http://127.0.0.1:9046".to_string(),
                    health: "/api/autonomy/status".to_string(),
                },
                Backend {
                    prefix: "/monitoring".to_string(),
                    url: "http://127.0.0.1:9091".to_string(),
                    health: "/".to_string(),
                },
            ],
            tls_enabled: true,
            cert_path: None,
            key_path: None,
            auth_token: None,
            cors_origins: vec!["*".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_four_backends() {
        let config = ProxyConfig::default();
        assert_eq!(config.backends.len(), 4);
        assert_eq!(config.backends[0].prefix, "/api/mesh");
        assert_eq!(config.backends[1].prefix, "/api/rag");
        assert_eq!(config.backends[2].prefix, "/api/autonomy");
        assert_eq!(config.backends[3].prefix, "/monitoring");
    }

    #[test]
    fn default_bind_addr() {
        let config = ProxyConfig::default();
        assert_eq!(config.bind_addr.port(), 8443);
    }

    #[test]
    fn backend_urls_point_to_localhost() {
        let config = ProxyConfig::default();
        for backend in &config.backends {
            assert!(backend.url.starts_with("http://127.0.0.1"));
        }
    }
}
