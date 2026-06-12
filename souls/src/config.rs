//! # souls_config — Configuration TOML de l'entité
//!
//! Format attendu (tous les champs optionnels) :
//! ```toml
//! [entity]
//! name = "soul"
//! memory_path = "/var/lib/souls/memory.db"
//! tick_ms = 750
//! autonomous = true
//!
//! [gateway]
//! address = "127.0.0.1:7878"
//! auth_token = "secret-abc123"
//! rate_limit_per_minute = 60
//! allowed_origins = ["http://localhost:*"]
//!
//! [llm]
//! ollama_url = "http://127.0.0.1:11434"
//! model = "qwen3:8b"
//! temperature = 0.7
//! max_tokens = 2048
//!
//! [sandbox]
//! strict = false
//! timeout_secs = 30
//!
//! [openclaw]
//! remote_url = "http://127.0.0.1:8080"
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoulsConfig {
    #[serde(default)]
    pub entity: EntityCfg,
    #[serde(default)]
    pub gateway: GatewayCfg,
    #[serde(default)]
    pub llm: LlmCfg,
    #[serde(default)]
    pub sandbox: SandboxCfg,
    #[serde(default)]
    pub openclaw: OpenclawCfg,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityCfg {
    pub name: Option<String>,
    pub memory_path: Option<String>,
    pub tick_ms: Option<u64>,
    pub autonomous: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayCfg {
    pub address: Option<String>,
    pub auth_token: Option<String>,
    pub rate_limit_per_minute: Option<usize>,
    pub allowed_origins: Option<Vec<String>>,
    pub max_body_size: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmCfg {
    pub ollama_url: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub http_timeout_secs: Option<u64>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxCfg {
    pub strict: Option<bool>,
    pub timeout_secs: Option<u64>,
    pub whitelist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenclawCfg {
    pub remote_url: Option<String>,
}

impl SoulsConfig {
    /// Charge depuis un fichier TOML. Si le fichier n'existe pas, renvoie
    /// une config par défaut.
    pub fn from_file(path: &Path) -> std::result::Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("lecture {}: {e}", path.display()))?;
        toml::from_str(&s).map_err(|e| format!("parse TOML: {e}"))
    }

    /// Charge + valide au démarrage.
    pub fn load_and_validate(path: &Path) -> std::result::Result<Self, String> {
        let cfg = Self::from_file(path)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Validation : cohérence des champs.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(t) = self.llm.temperature {
            if !(0.0..=2.0).contains(&t) {
                return Err(format!("temperature hors bornes: {t} (attendu 0.0-2.0)"));
            }
        }
        if let Some(t) = self.llm.http_timeout_secs {
            if t == 0 {
                return Err("http_timeout_secs doit être > 0".into());
            }
        }
        if let Some(t) = self.sandbox.timeout_secs {
            if t == 0 {
                return Err("sandbox.timeout_secs doit être > 0".into());
            }
        }
        if let Some(r) = self.gateway.rate_limit_per_minute {
            if r == 0 {
                return Err("rate_limit_per_minute doit être > 0".into());
            }
        }
        Ok(())
    }
}

/// Profils prédéfinis (F.3).
///
/// Utilisé quand le chargement TOML est activé via `--config`.
/// Pour l'instant, les valeurs par défaut sont hardcodées dans main.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Prod,
    Test,
}

impl Profile {
    pub fn from_env() -> Self {
        match std::env::var("SOUL_PROFILE").as_deref() {
            Ok("prod") | Ok("production") => Self::Prod,
            Ok("test") => Self::Test,
            _ => Self::Dev,
        }
    }

    pub fn apply(&self, cfg: &mut SoulsConfig) {
        match self {
            Self::Dev => {
                cfg.gateway.rate_limit_per_minute.get_or_insert(120);
                cfg.gateway.allowed_origins.get_or_insert(vec!["*".into()]);
                cfg.entity.autonomous.get_or_insert(false);
            }
            Self::Prod => {
                cfg.gateway.rate_limit_per_minute.get_or_insert(60);
                if cfg.gateway.allowed_origins.is_none() {
                    cfg.gateway.allowed_origins = Some(vec!["https://console.soul.example".into()]);
                }
                if cfg.gateway.auth_token.is_none() {
                    tracing::warn!(
                        "[profile:prod] SOUL_GATEWAY_TOKEN non défini — gateway ouvert !"
                    );
                }
            }
            Self::Test => {
                cfg.gateway.rate_limit_per_minute.get_or_insert(10_000);
                cfg.gateway.allowed_origins.get_or_insert(vec!["*".into()]);
                cfg.entity.autonomous.get_or_insert(false);
                cfg.entity.tick_ms.get_or_insert(50);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
[entity]
name = "alice"

[gateway]
address = "0.0.0.0:9000"
"#;
        let cfg: SoulsConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.entity.name.as_deref(), Some("alice"));
        assert_eq!(cfg.gateway.address.as_deref(), Some("0.0.0.0:9000"));
    }

    #[test]
    fn empty_config_is_default() {
        let cfg = SoulsConfig::default();
        assert!(cfg.entity.name.is_none());
    }

    #[test]
    fn validation_rejects_bad_temperature() {
        let mut cfg = SoulsConfig::default();
        cfg.llm.temperature = Some(3.0);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validation_rejects_zero_timeout() {
        let mut cfg = SoulsConfig::default();
        cfg.sandbox.timeout_secs = Some(0);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validation_rejects_zero_rate_limit() {
        let mut cfg = SoulsConfig::default();
        cfg.gateway.rate_limit_per_minute = Some(0);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn dev_profile_applies_defaults() {
        let mut cfg = SoulsConfig::default();
        Profile::Dev.apply(&mut cfg);
        assert_eq!(cfg.gateway.rate_limit_per_minute, Some(120));
        assert!(!cfg.entity.autonomous.unwrap_or(true));
    }

    #[test]
    fn prod_profile_warns_no_token() {
        let mut cfg = SoulsConfig::default();
        Profile::Prod.apply(&mut cfg);
        assert_eq!(cfg.gateway.rate_limit_per_minute, Some(60));
    }

    #[test]
    fn test_profile_high_rate_limit() {
        let mut cfg = SoulsConfig::default();
        Profile::Test.apply(&mut cfg);
        assert_eq!(cfg.gateway.rate_limit_per_minute, Some(10_000));
    }

    #[test]
    fn from_nonexistent_file_returns_default() {
        let cfg = SoulsConfig::from_file(Path::new("/tmp/_nonexistent_souls.toml")).unwrap();
        assert!(cfg.entity.name.is_none());
    }
}
