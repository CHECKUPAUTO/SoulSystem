//! Circuit Breaker for SoulLink organ services.
//!
//! Ported from IronClaw's `llm/circuit_breaker.rs` (786 LOC).
//! Prevents cascade failures when an organ service is down.
//!
//! # State Machine
//!
//! ```text
//!   Closed ──(failures >= threshold)──► Open
//!     ▲                                   │
//!     │                          (recovery timeout)
//!     │                                   ▼
//!     └──(probe succeeds)──── HalfOpen ──(probe fails)──► Open
//! ```
//!
//! # Usage
//!
//! ```rust
//! use soullink_circuit::{CircuitBreaker, CircuitBreakerConfig};
//!
//! let breaker = CircuitBreaker::new("organ-memory-9030", CircuitBreakerConfig::default());
//!
//! // In your organ client:
//! match breaker.call(|| async {
//!     reqwest::get("http://localhost:9030/api/stats").await
//! }).await {
//!     Ok(response) => { /* process */ },
//!     Err(e) => { /* circuit open or service failed */ },
//! }
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum CircuitState {
    /// Normal operation; tracking consecutive failures.
    #[default]
    Closed,
    /// Rejecting all calls; waiting for recovery timeout.
    Open,
    /// Allowing probe calls to test whether the service recovered.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Name of the service being protected.
    pub service_name: String,
    /// Consecutive transient failures before the circuit opens.
    pub failure_threshold: u32,
    /// How long the circuit stays open before allowing a probe.
    pub recovery_timeout: Duration,
    /// Successful probes needed in half-open to close the circuit.
    pub half_open_successes_needed: u32,
    /// Timeout for individual calls.
    pub call_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            service_name: "unknown".into(),
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: Duration::from_secs(10),
        }
    }
}

impl CircuitBreakerConfig {
    /// Preset config for SoulLink organ services.
    pub fn organ(name: impl Into<String>, port: u16) -> Self {
        Self {
            service_name: format!("{}-{}", name.into(), port),
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(15),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(5),
        }
    }

    /// Preset config for LLM providers.
    pub fn llm_provider(name: impl Into<String>) -> Self {
        Self {
            service_name: name.into(),
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: Duration::from_secs(30),
        }
    }
}

/// Internal mutable state.
struct BreakerState {
    state: CircuitState,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    half_open_successes: u32,
    total_calls: u64,
    total_failures: u64,
    total_rejections: u64,
}

impl BreakerState {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            opened_at: None,
            half_open_successes: 0,
            total_calls: 0,
            total_failures: 0,
            total_rejections: 0,
        }
    }
}

/// Circuit breaker that wraps async calls with failure protection.
///
/// Thread-safe via internal `Mutex`. Cheap to clone (Arc-backed).
#[derive(Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<Mutex<BreakerState>>,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        let config = CircuitBreakerConfig {
            service_name: name.into(),
            ..config
        };
        Self {
            config,
            state: Arc::new(Mutex::new(BreakerState::new())),
        }
    }

    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            config: config.clone(),
            state: Arc::new(Mutex::new(BreakerState::new())),
        }
    }

    /// Current state of the circuit.
    pub async fn state(&self) -> CircuitState {
        let state = self.state.lock().await;
        self.effective_state(&state)
    }

    /// Call an async function through the circuit breaker.
    ///
    /// Returns `Err(CircuitBreakerError::Open)` if the circuit is open.
    /// Returns `Err(CircuitBreakerError::Call)` if the call itself failed.
    /// Returns `Ok(T)` if the call succeeded.
    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let mut state = self.state.lock().await;

        // Check current state
        let effective = self.effective_state(&state);
        match effective {
            CircuitState::Open => {
                state.total_rejections += 1;
                warn!(
                    service = %self.config.service_name,
                    "Circuit OPEN — rejecting call"
                );
                drop(state);
                return Err(CircuitBreakerError::Open {
                    service: self.config.service_name.clone(),
                    opened_at: Instant::now(),
                });
            }
            CircuitState::Closed | CircuitState::HalfOpen => {}
        }

        state.total_calls += 1;
        drop(state); // Release lock during call

        // Execute the call with timeout
        let result = tokio::time::timeout(self.config.call_timeout, f()).await;

        let mut state = self.state.lock().await;
        match result {
            Ok(Ok(value)) => {
                self.on_success(&mut state);
                Ok(value)
            }
            Ok(Err(e)) => {
                self.on_failure(&mut state);
                Err(CircuitBreakerError::Call {
                    service: self.config.service_name.clone(),
                    source: e,
                })
            }
            Err(_) => {
                // Timeout
                self.on_failure(&mut state);
                Err(CircuitBreakerError::Timeout {
                    service: self.config.service_name.clone(),
                    timeout: self.config.call_timeout,
                })
            }
        }
    }

    /// Record a successful call (for external callers who manage their own calls).
    pub async fn record_success(&self) {
        let mut state = self.state.lock().await;
        self.on_success(&mut state);
    }

    /// Record a failed call (for external callers who manage their own calls).
    pub async fn record_failure(&self) {
        let mut state = self.state.lock().await;
        self.on_failure(&mut state);
    }

    fn on_success(&self, state: &mut BreakerState) {
        state.consecutive_failures = 0;
        let effective = self.effective_state(state);
        match effective {
            CircuitState::HalfOpen => {
                state.half_open_successes += 1;
                if state.half_open_successes >= self.config.half_open_successes_needed {
                    info!(
                        service = %self.config.service_name,
                        successes = state.half_open_successes,
                        "Circuit CLOSED — service recovered"
                    );
                    state.state = CircuitState::Closed;
                }
            }
            _ => {}
        }
    }

    fn on_failure(&self, state: &mut BreakerState) {
        state.consecutive_failures += 1;
        state.total_failures += 1;

        match state.state {
            CircuitState::Closed => {
                if state.consecutive_failures >= self.config.failure_threshold {
                    warn!(
                        service = %self.config.service_name,
                        failures = state.consecutive_failures,
                        threshold = self.config.failure_threshold,
                        "Circuit OPENED — too many failures"
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    state.half_open_successes = 0;
                }
            }
            CircuitState::HalfOpen => {
                warn!(
                    service = %self.config.service_name,
                    "Circuit re-OPENED — probe failed"
                );
                state.state = CircuitState::Open;
                state.opened_at = Some(Instant::now());
                state.half_open_successes = 0;
            }
            CircuitState::Open => {}
        }
    }

    fn effective_state(&self, state: &BreakerState) -> CircuitState {
        match state.state {
            CircuitState::Open => {
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.config.recovery_timeout {
                        CircuitState::HalfOpen
                    } else {
                        CircuitState::Open
                    }
                } else {
                    CircuitState::Open
                }
            }
            other => other,
        }
    }

    /// Get circuit breaker statistics.
    pub async fn stats(&self) -> CircuitStats {
        let state = self.state.lock().await;
        CircuitStats {
            service: self.config.service_name.clone(),
            state: self.effective_state(&state),
            consecutive_failures: state.consecutive_failures,
            total_calls: state.total_calls,
            total_failures: state.total_failures,
            total_rejections: state.total_rejections,
            failure_threshold: self.config.failure_threshold,
            recovery_timeout: self.config.recovery_timeout,
        }
    }
}

/// Circuit breaker error types.
#[derive(Debug)]
pub enum CircuitBreakerError {
    Open {
        service: String,
        opened_at: Instant,
    },
    Timeout {
        service: String,
        timeout: Duration,
    },
    Call {
        service: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl std::fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { service, .. } => write!(f, "Circuit open for {}", service),
            Self::Timeout { service, timeout } => {
                write!(f, "Timeout ({:?}) calling {}", timeout, service)
            }
            Self::Call { service, source } => write!(f, "Call to {} failed: {}", service, source),
        }
    }
}

impl std::error::Error for CircuitBreakerError {}

/// Circuit breaker statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitStats {
    pub service: String,
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub total_calls: u64,
    pub total_failures: u64,
    pub total_rejections: u64,
    pub failure_threshold: u32,
    #[serde(skip)]
    pub recovery_timeout: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn circuit_starts_closed() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig::default());
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn circuit_opens_after_threshold() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(1),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(1),
            service_name: "test".into(),
        });

        // Record 3 failures
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn circuit_rejects_when_open() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_secs(60),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(1),
            service_name: "test".into(),
        });

        cb.record_failure().await;
        // After threshold failure, circuit is open
        assert_eq!(cb.state().await, CircuitState::Open);
        let stats = cb.stats().await;
        assert_eq!(stats.total_rejections, 0);
    }

    #[tokio::test]
    async fn circuit_half_open_after_recovery_timeout() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_millis(50),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(1),
            service_name: "test".into(),
        });

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn circuit_closes_after_successful_probes() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_millis(50),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(1),
            service_name: "test".into(),
        });

        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn stats_report() {
        let cb = CircuitBreaker::with_config(CircuitBreakerConfig::default());
        let stats = cb.stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.total_calls, 0);
    }

    #[tokio::test]
    async fn organ_preset() {
        let config = CircuitBreakerConfig::organ("memory", 9030);
        assert_eq!(config.service_name, "memory-9030");
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.recovery_timeout, Duration::from_secs(15));
    }
}
