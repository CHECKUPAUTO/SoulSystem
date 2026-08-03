use std::env;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub workers: usize,
    pub auth_token: Option<String>,
    pub log_level: String,
    pub enable_telegram: bool,
    pub enable_whatsapp: bool,
    pub telegram_token: Option<String>,
    pub whatsapp_session: Option<String>,
}

fn env_bool(key: &str) -> bool {
    matches!(env::var(key).ok().as_deref(), Some("1" | "true" | "yes" | "on"))
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let port = env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(18889);
        let workers = env::var("WORKERS").ok().and_then(|w| w.parse().ok()).unwrap_or(4).clamp(1, 64);

        let cfg = Self {
            port,
            workers,
            auth_token: env::var("GATEWAY_TOKEN").ok(),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            enable_telegram: env_bool("ENABLE_TELEGRAM"),
            enable_whatsapp: env_bool("ENABLE_WHATSAPP"),
            telegram_token: env::var("TELEGRAM_TOKEN").ok(),
            whatsapp_session: env::var("WHATSAPP_SESSION").ok(),
        };

        if cfg.auth_token.is_none() {
            tracing::warn!("⚠️ GATEWAY_TOKEN not set — gateway running without static operator token");
        }

        cfg
    }
}
