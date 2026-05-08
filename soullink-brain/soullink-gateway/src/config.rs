//! Configuration for the SoulLink gateway.
//!
//! Reads from `/etc/soullink/gateway.toml` (override with
//! `SOULLINK_GATEWAY_CONFIG` env var). The bot token and any other
//! secrets are sourced separately from `/etc/soullink/secrets.env` —
//! never from this file — following the Phase 1 secrets discipline.

use std::{path::Path, time::Duration};

use serde::Deserialize;
use thiserror::Error;

/// Default base URL for Telegram Bot API. Phase 6b will switch this to
/// the self-hosted instance URL for >50 MB file support.
pub const TELEGRAM_DEFAULT_BASE_URL: &str = "https://api.telegram.org";

/// Default orchestrator URL — gateway posts `/api/mesh/think` here.
pub const DEFAULT_ORCHESTRATOR_URL: &str = "http://127.0.0.1:9020";

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    /// HTTP endpoint where the orchestrator accepts inbound messages.
    /// Points to the loopback orchestrator by default.
    #[serde(default = "default_orchestrator_url")]
    pub orchestrator_url: String,

    /// Telegram channel — presence of this section enables Telegram.
    pub telegram: Option<TelegramConfig>,

    /// Overall request timeout for calls to the orchestrator.
    #[serde(with = "humantime_ms", default = "default_orch_timeout")]
    pub orchestrator_timeout: Duration,

    /// Prometheus metrics port (gateway-internal, binds 127.0.0.1 only).
    /// Phase 6a hasn't implemented metrics yet; reserved for Phase 6b.
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    /// Base URL of the Bot API server. Default = public API. Change to
    /// self-hosted URL once Phase 6b is deployed for 2 GB file support.
    #[serde(default = "default_telegram_base_url")]
    pub api_base_url: String,

    /// Long-poll timeout in seconds — how long Telegram should hold the
    /// connection waiting for updates. Max 50 per Telegram docs.
    #[serde(default = "default_long_poll_timeout_s")]
    pub long_poll_timeout_s: u32,

    /// Allow-list of Telegram chat IDs. If `Some`, only messages from
    /// these chats are accepted. If `None`, **all** chats are allowed —
    /// use only for private bots or testing.
    pub allowed_chat_ids: Option<Vec<i64>>,

    /// Optional typing-indicator duration after receiving a message,
    /// before the actual reply comes back from the orchestrator. Keeps
    /// the UI alive during long brain thinking.
    #[serde(with = "humantime_ms_opt", default)]
    pub typing_pre_reply: Option<Duration>,

    /// Phase 6b: enable SSE streaming from orchestrator. When `true`,
    /// the gateway uses `POST /api/mesh/stream` and progressively edits
    /// the reply message as tokens arrive. When `false` (default), uses
    /// the blocking `/api/mesh/think` with a single sendMessage at end.
    #[serde(default)]
    pub streaming: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            orchestrator_url:     default_orchestrator_url(),
            telegram:             None,
            orchestrator_timeout: default_orch_timeout(),
            metrics_port:         default_metrics_port(),
        }
    }
}

fn default_orchestrator_url() -> String { DEFAULT_ORCHESTRATOR_URL.into() }
fn default_telegram_base_url() -> String { TELEGRAM_DEFAULT_BASE_URL.into() }
fn default_long_poll_timeout_s() -> u32 { 25 }
fn default_orch_timeout() -> Duration { Duration::from_secs(60) }
fn default_metrics_port() -> u16 { 9092 }

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found at {0}")]
    NotFound(String),
    #[error("read error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

/// Load the gateway config from disk.
pub fn load_from(path: &Path) -> Result<GatewayConfig, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.display().to_string()));
    }
    let raw = std::fs::read_to_string(path)?;
    let cfg: GatewayConfig = toml::from_str(&raw).map_err(|e| ConfigError::Parse(e.to_string()))?;
    Ok(cfg)
}

/// Load from `SOULLINK_GATEWAY_CONFIG` env var, falling back to
/// `/etc/soullink/gateway.toml`.
pub fn load_default() -> Result<GatewayConfig, ConfigError> {
    let path = std::env::var("SOULLINK_GATEWAY_CONFIG")
        .unwrap_or_else(|_| "/etc/soullink/gateway.toml".into());
    load_from(Path::new(&path))
}

// ─── serde helpers for humantime-ish Duration parsing ────────────────────────

mod humantime_ms {
    use super::Duration;
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        parse(&s).map_err(serde::de::Error::custom)
    }
    pub fn parse(s: &str) -> Result<Duration, String> {
        let s = s.trim();
        if let Some(rest) = s.strip_suffix("ms") {
            let n: u64 = rest.trim().parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            Ok(Duration::from_millis(n))
        } else if let Some(rest) = s.strip_suffix('s') {
            let n: u64 = rest.trim().parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
            Ok(Duration::from_secs(n))
        } else {
            Err(format!("expected duration like '500ms' or '30s', got {s:?}"))
        }
    }
}

mod humantime_ms_opt {
    use super::Duration;
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let s: Option<String> = Option::deserialize(d)?;
        match s {
            None => Ok(None),
            Some(s) => super::humantime_ms::parse(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_parses() {
        let raw = r#"
            orchestrator_url = "http://127.0.0.1:9020"
            orchestrator_timeout = "60s"

            [telegram]
            api_base_url = "https://api.telegram.org"
            long_poll_timeout_s = 25
            allowed_chat_ids = [123, 456]
        "#;
        let cfg: GatewayConfig = toml::from_str(raw).unwrap();
        assert_eq!(cfg.orchestrator_url, "http://127.0.0.1:9020");
        let tg = cfg.telegram.expect("telegram section");
        assert_eq!(tg.allowed_chat_ids, Some(vec![123, 456]));
    }

    #[test]
    fn no_telegram_section_is_valid() {
        let raw = r#"
            orchestrator_url = "http://127.0.0.1:9020"
            orchestrator_timeout = "30s"
        "#;
        let cfg: GatewayConfig = toml::from_str(raw).unwrap();
        assert!(cfg.telegram.is_none());
    }

    #[test]
    fn defaults_apply_for_missing_fields() {
        let raw = r#"
            [telegram]
            allowed_chat_ids = [999]
        "#;
        let cfg: GatewayConfig = toml::from_str(raw).unwrap();
        assert_eq!(cfg.orchestrator_url, DEFAULT_ORCHESTRATOR_URL);
        let tg = cfg.telegram.unwrap();
        assert_eq!(tg.api_base_url, TELEGRAM_DEFAULT_BASE_URL);
        assert_eq!(tg.long_poll_timeout_s, 25);
    }

    #[test]
    fn allowed_chat_ids_empty_vec_is_restrictive() {
        // [] means "allow no chats" — useful during maintenance
        let raw = r#"
            [telegram]
            allowed_chat_ids = []
        "#;
        let cfg: GatewayConfig = toml::from_str(raw).unwrap();
        let tg = cfg.telegram.unwrap();
        assert_eq!(tg.allowed_chat_ids, Some(vec![]));
    }

    #[test]
    fn duration_parses_ms_and_s() {
        let raw = r#"
            orchestrator_timeout = "500ms"
            [telegram]
            typing_pre_reply = "3s"
        "#;
        let cfg: GatewayConfig = toml::from_str(raw).unwrap();
        assert_eq!(cfg.orchestrator_timeout, Duration::from_millis(500));
        assert_eq!(cfg.telegram.unwrap().typing_pre_reply, Some(Duration::from_secs(3)));
    }

    #[test]
    fn load_from_missing_file() {
        let err = load_from(Path::new("/tmp/nonexistent-gateway-config-xyz-42.toml")).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound(_)));
    }
}
