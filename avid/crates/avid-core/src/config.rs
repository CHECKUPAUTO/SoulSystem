use std::path::PathBuf;
use thiserror::Error;

/// Errors produced during configuration loading.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {var}: {reason}")]
    Invalid { var: &'static str, reason: String },
}

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_token: String,
    pub home: PathBuf,
    pub ollama_url: String,
    pub ollama_model: String,
    pub redis_url: Option<String>,
    pub workers: usize,
    pub split_threshold: usize,
    pub port: u16,
    pub log_level: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    /// Returns `ConfigError` if required variables are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let api_token =
            std::env::var("AVID_API_TOKEN").map_err(|_| ConfigError::Missing("AVID_API_TOKEN"))?;
        if api_token.len() < 32 {
            return Err(ConfigError::Invalid {
                var: "AVID_API_TOKEN",
                reason: "must be at least 32 characters".to_string(),
            });
        }

        let home = std::env::var("AVID_HOME").map_err(|_| ConfigError::Missing("AVID_HOME"))?;
        let home_path = PathBuf::from(&home);
        if !home_path.is_dir() {
            return Err(ConfigError::Invalid {
                var: "AVID_HOME",
                reason: format!("{home} is not an existing directory"),
            });
        }

        let ollama_url =
            std::env::var("OLLAMA_URL").map_err(|_| ConfigError::Missing("OLLAMA_URL"))?;

        let ollama_model =
            std::env::var("OLLAMA_MODEL").map_err(|_| ConfigError::Missing("OLLAMA_MODEL"))?;

        let redis_url = std::env::var("AVID_REDIS_URL").ok();

        let workers: usize = std::env::var("AVID_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);

        let split_threshold: usize = std::env::var("AVID_SPLIT_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let port: u16 = std::env::var("AVID_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8088);

        let log_level = std::env::var("AVID_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            api_token,
            home: home_path,
            ollama_url,
            ollama_model,
            redis_url,
            workers,
            split_threshold,
            port,
            log_level,
        })
    }

    #[must_use]
    pub fn soullink_db_path(&self) -> PathBuf {
        self.home.join("soullink.db")
    }
}
