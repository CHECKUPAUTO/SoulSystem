//! Orchestrator configuration.
//!
//! Optional TOML file at `/etc/soullink/orchestrator.toml` (override via
//! `SOULLINK_ORCHESTRATOR_CONFIG` env var). Only one section for now:
//! `[ollama]`. Missing file → default config with no Ollama (falls back
//! to symbolic synthesis).

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::ollama_bridge::OllamaConfig;

// Consommé par le chemin mémoire pas encore câblé (voir ollama_bridge).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MemoryConfig {
    #[allow(dead_code)]
    pub db_path: String,
    #[serde(default = "default_sled_cache_mb")]
    #[allow(dead_code)]
    pub sled_cache_mb: usize,
    #[serde(default = "default_search_top_k")]
    #[allow(dead_code)]
    pub search_top_k: usize,
}

fn default_sled_cache_mb() -> usize {
    10240
}
fn default_search_top_k() -> usize {
    5
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OrchestratorConfig {
    pub ollama: Option<OllamaConfig>,
    #[allow(dead_code)]
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("read error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub fn load_from(path: &Path) -> Result<OrchestratorConfig, ConfigError> {
    let raw = std::fs::read_to_string(path)?;
    toml::from_str(&raw).map_err(|e| ConfigError::Parse(e.to_string()))
}

/// Load from env var or default path. If the file doesn't exist, return a
/// default (empty) config — orchestrator should run without a TOML file.
pub fn load_default() -> OrchestratorConfig {
    let path_str = std::env::var("SOULLINK_ORCHESTRATOR_CONFIG")
        .unwrap_or_else(|_| "/etc/soullink/orchestrator.toml".into());
    let path = Path::new(&path_str);

    if !path.exists() {
        return OrchestratorConfig::default();
    }
    match load_from(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(path = %path.display(), %e,
                "orchestrator config parse failed — using defaults");
            OrchestratorConfig::default()
        }
    }
}

/// Shared serde helper: "60s" / "500ms" → Duration.
pub mod duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        let s = s.trim();
        if let Some(rest) = s.strip_suffix("ms") {
            let n: u64 = rest.trim().parse().map_err(serde::de::Error::custom)?;
            Ok(Duration::from_millis(n))
        } else if let Some(rest) = s.strip_suffix('s') {
            let n: u64 = rest.trim().parse().map_err(serde::de::Error::custom)?;
            Ok(Duration::from_secs(n))
        } else {
            Err(serde::de::Error::custom(format!(
                "expected duration like '60s' or '500ms', got {s:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn missing_file_returns_default() {
        let cfg = load_default();
        // Le défaut ne doit pas embarquer de config Ollama implicite.
        assert!(cfg.ollama.is_none());
    }

    #[test]
    fn parse_with_ollama_section() {
        let raw = r#"
            [ollama]
            base_url = "http://127.0.0.1:11434"
            model = "qwen2.5-coder:3b"
            timeout = "45s"
        "#;
        let cfg: OrchestratorConfig = toml::from_str(raw).unwrap();
        let ollama = cfg.ollama.unwrap();
        assert_eq!(ollama.model, "qwen2.5-coder:3b");
        assert_eq!(ollama.timeout, Duration::from_secs(45));
    }

    #[test]
    fn parse_without_ollama_section_is_valid() {
        let cfg: OrchestratorConfig = toml::from_str("").unwrap();
        assert!(cfg.ollama.is_none());
    }

    #[test]
    fn bad_duration_format_rejected() {
        let raw = r#"
            [ollama]
            timeout = "forever"
        "#;
        let err = toml::from_str::<OrchestratorConfig>(raw).unwrap_err();
        assert!(err.to_string().contains("expected duration"));
    }
}
