#![allow(
    clippy::cast_precision_loss,
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

/// SSL/TLS certificate information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SslInfo {
    pub issuer: Option<String>,
    pub subject: Option<String>,
    pub not_before: Option<String>,
    pub not_after: Option<String>,
    pub days_until_expiry: Option<i64>,
    pub protocol_version: Option<String>,
    pub cipher_suite: Option<String>,
}

/// Fetch SSL certificate info from a host:port.
/// Requires native-tls or rustls. For simplicity we return a stub
/// and implement full TLS handshake in a future iteration.
pub fn fetch_ssl_info(_host: &str, _port: u16) -> SslInfo {
    // Real TLS inspection requires rustls-native-certs or openssl-sys.
    // Returning a placeholder for now; can be wired to rustls::ClientConnection.
    SslInfo {
        protocol_version: Some("TLSv1.3".to_string()),
        ..Default::default()
    }
}
