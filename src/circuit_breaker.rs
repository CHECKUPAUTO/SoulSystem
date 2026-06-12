//! Circuit Breaker — re-export du module unifié.
//!
//! Voir `soullink-circuit` pour l'implémentation complète.

pub use soullink_circuit::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitBreakerRegistry,
    CircuitState, CircuitStats,
};
