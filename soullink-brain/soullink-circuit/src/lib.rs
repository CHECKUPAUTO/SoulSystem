//! Unified Circuit Breaker for SoulSystem.
//!
//! Merge des 3 impls historiques : `soullink-circuit`, `soullink-circuit-breaker`,
//! et `src/circuit_breaker.rs`.
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

use dashmap::DashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

// ── State ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CircuitState {
    #[default]
    Closed,
    Open,
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half-open"),
        }
    }
}

// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_successes_needed: u32,
    pub call_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: Duration::from_secs(10),
        }
    }
}

impl CircuitBreakerConfig {
    pub fn organ(name: impl Into<String>, port: u16) -> Self {
        Self {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(15),
            half_open_successes_needed: 1,
            call_timeout: Duration::from_secs(5),
        }
        .with_service_name(format!("{}-{}", name.into(), port))
    }

    pub fn llm_provider(name: impl Into<String>) -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: Duration::from_secs(30),
        }
        .with_service_name(name)
    }

    pub fn with_service_name(self, name: impl Into<String>) -> Self {
        let _ = name;
        self
    }

    pub fn with_threshold(mut self, n: u32) -> Self {
        self.failure_threshold = n;
        self
    }

    pub fn with_recovery_timeout(mut self, d: Duration) -> Self {
        self.recovery_timeout = d;
        self
    }
}

// ── Error ─────────────────────────────────────────────────────────────────

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

impl fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

// ── Stats ─────────────────────────────────────────────────────────────────

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

// ── Per-service internal state ────────────────────────────────────────────

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

// ── Single circuit breaker ────────────────────────────────────────────────

#[derive(Clone)]
pub struct CircuitBreaker {
    service_name: String,
    config: CircuitBreakerConfig,
    state: Arc<Mutex<BreakerState>>,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            service_name: name.into(),
            config,
            state: Arc::new(Mutex::new(BreakerState::new())),
        }
    }

    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            service_name: "unknown".into(),
            config,
            state: Arc::new(Mutex::new(BreakerState::new())),
        }
    }

    pub async fn state(&self) -> CircuitState {
        let state = self.state.lock().await;
        self.effective_state(&state)
    }

    /// Call an async function through the circuit breaker.
    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let mut state = self.state.lock().await;

        let effective = self.effective_state(&state);
        if effective == CircuitState::Open {
            state.total_rejections += 1;
            let opened_at = state.opened_at.unwrap_or_else(Instant::now);
            drop(state);
            return Err(CircuitBreakerError::Open {
                service: self.service_name.clone(),
                opened_at,
            });
        }

        state.total_calls += 1;
        drop(state);

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
                    service: self.service_name.clone(),
                    source: e,
                })
            }
            Err(_) => {
                self.on_failure(&mut state);
                Err(CircuitBreakerError::Timeout {
                    service: self.service_name.clone(),
                    timeout: self.config.call_timeout,
                })
            }
        }
    }

    pub async fn record_success(&self) {
        let mut state = self.state.lock().await;
        self.on_success(&mut state);
    }

    pub async fn record_failure(&self) {
        let mut state = self.state.lock().await;
        self.on_failure(&mut state);
    }

    pub async fn stats(&self) -> CircuitStats {
        let state = self.state.lock().await;
        CircuitStats {
            service: self.service_name.clone(),
            state: self.effective_state(&state),
            consecutive_failures: state.consecutive_failures,
            total_calls: state.total_calls,
            total_failures: state.total_failures,
            total_rejections: state.total_rejections,
            failure_threshold: self.config.failure_threshold,
            recovery_timeout: self.config.recovery_timeout,
        }
    }

    fn on_success(&self, state: &mut BreakerState) {
        state.consecutive_failures = 0;
        if self.effective_state(state) == CircuitState::HalfOpen {
            state.half_open_successes += 1;
            if state.half_open_successes >= self.config.half_open_successes_needed {
                info!(
                    service = %self.service_name,
                    successes = state.half_open_successes,
                    "Circuit CLOSED — service recovered"
                );
                state.state = CircuitState::Closed;
            }
        }
    }

    fn on_failure(&self, state: &mut BreakerState) {
        state.consecutive_failures += 1;
        state.total_failures += 1;

        match state.state {
            CircuitState::Closed => {
                if state.consecutive_failures >= self.config.failure_threshold {
                    warn!(
                        service = %self.service_name,
                        failures = state.consecutive_failures,
                        threshold = self.config.failure_threshold,
                        "Circuit OPENED"
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    state.half_open_successes = 0;
                }
            }
            CircuitState::HalfOpen => {
                warn!(service = %self.service_name, "Circuit re-OPENED — probe failed");
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
}

// ── Multi-service registry ────────────────────────────────────────────────

pub type StateChangeCallback = Box<dyn Fn(&str, CircuitState, u32, Option<&str>) + Send + Sync>;

pub struct CircuitBreakerRegistry {
    table: Arc<DashMap<String, CircuitBreaker>>,
    default_config: CircuitBreakerConfig,
    on_state_change: Option<StateChangeCallback>,
}

impl Clone for CircuitBreakerRegistry {
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            default_config: self.default_config.clone(),
            on_state_change: None,
        }
    }
}

impl CircuitBreakerRegistry {
    pub fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            table: Arc::new(DashMap::new()),
            default_config,
            on_state_change: None,
        }
    }

    pub fn with_state_change_callback(mut self, cb: StateChangeCallback) -> Self {
        self.on_state_change = Some(cb);
        self
    }

    /// Get or create a breaker for a service.
    pub fn get(&self, service: &str) -> CircuitBreaker {
        self.table
            .entry(service.to_string())
            .or_insert_with(|| CircuitBreaker::new(service, self.default_config.clone()))
            .clone()
    }

    /// Check if a service is allowed (pure DashMap read).
    pub fn allow(&self, service: &str) -> bool {
        !self.table.contains_key(service)
    }

    pub fn services(&self) -> Vec<String> {
        self.table.iter().map(|r| r.key().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn circuit_starts_closed() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn circuit_opens_after_threshold() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default().with_threshold(3));
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn circuit_rejects_when_open() {
        let cb = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig::default()
                .with_threshold(1)
                .with_recovery_timeout(Duration::from_secs(60)),
        );
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn circuit_half_open_after_recovery() {
        let cb = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig::default()
                .with_threshold(1)
                .with_recovery_timeout(Duration::from_millis(50)),
        );
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn circuit_closes_after_successful_probe() {
        let cb = CircuitBreaker::new(
            "test",
            CircuitBreakerConfig {
                failure_threshold: 1,
                recovery_timeout: Duration::from_millis(50),
                half_open_successes_needed: 1,
                call_timeout: Duration::from_secs(1),
            },
        );
        cb.record_failure().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn organ_preset() {
        let config = CircuitBreakerConfig::organ("memory", 9030);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.recovery_timeout, Duration::from_secs(15));
    }

    #[tokio::test]
    async fn stats_report() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        let stats = cb.stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.total_calls, 0);
    }

    #[tokio::test]
    async fn registry_get_creates_breakers() {
        let reg = CircuitBreakerRegistry::new(CircuitBreakerConfig::default());
        let _b1 = reg.get("service-a");
        let _b2 = reg.get("service-b");
        assert_eq!(reg.services().len(), 2);
    }
}
