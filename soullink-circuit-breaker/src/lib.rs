//! Wrapper backward-compatible pour le circuit breaker unifié.
//!
//! Délègue à `soullink-circuit` pour toute la logique.

pub use soullink_circuit::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerRegistry,
    CircuitStats, CircuitState, StateChangeCallback,
};

/// Re-export du type Config historique pour compatibilité.
pub struct Config {
    pub max_failures: u32,
    pub reset_timeout_secs: u64,
    pub on_state_change: Option<Box<dyn Fn(&str, CircuitState, u32, Option<&str>) + Send + Sync>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_failures: 3,
            reset_timeout_secs: 300,
            on_state_change: None,
        }
    }
}

impl From<Config> for CircuitBreakerConfig {
    fn from(c: Config) -> Self {
        CircuitBreakerConfig {
            failure_threshold: c.max_failures,
            recovery_timeout: std::time::Duration::from_secs(c.reset_timeout_secs),
            ..Default::default()
        }
    }
}

/// Re-export de l'erreur historique.
pub type CircuitOpenError = CircuitBreakerError;
