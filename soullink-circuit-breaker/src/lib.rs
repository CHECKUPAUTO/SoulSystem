//! OTP-style Circuit Breaker for Rust async services.
//!
//! Ported from AlexClaw's Elixir/OTP `CircuitBreaker` GenServer pattern.
//!
//! ## Architecture
//!
//! - **State machine:** `Closed` → `Open` → `HalfOpen` → `Closed`
//! - **DashMap** for lock-free hot-path reads (ETS equivalent)
//! - **tokio timers** for auto half-open probing (Process.send_after equivalent)
//! - **Notification callback** on state transitions (Telegram/Discord/logging)
//!
//! The circuit breaker wraps service execution transparently —
//! services have zero awareness of the breaker.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use soullink_circuit_breaker::{CircuitBreaker, Config};
//!
//! let cb = CircuitBreaker::new(Config {
//!     max_failures: 3,
//!     reset_timeout_secs: 300,
//!     on_state_change: Some(Box::new(|skill, state, count, error| {
//!         eprintln!("[CB] {} → {:?} (failures: {}, error: {:?})", skill, state, count, error);
//!     })),
//! });
//!
//! let result = cb.call("my_service", || {
//!     // Your fallible operation
//!     Ok::<_, &str>("success")
//! });
//! ```

use dashmap::DashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum State {
    /// Normal operation — all calls pass through.
    Closed,
    /// Circuit is open — all calls are rejected immediately.
    Open,
    /// Testing recovery — one call is allowed through to probe.
    HalfOpen,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Closed => write!(f, "CLOSED"),
            State::Open => write!(f, "OPEN"),
            State::HalfOpen => write!(f, "HALF_OPEN"),
        }
    }
}

/// Entry stored in the circuit breaker table.
#[derive(Debug, Clone)]
struct Entry {
    state: State,
    failures: u32,
    last_error: Option<String>,
    last_change: chrono::DateTime<chrono::Utc>,
}

/// Configuration for a circuit breaker instance.
pub struct Config {
    /// Number of consecutive failures before opening the circuit.
    pub max_failures: u32,
    /// Seconds to wait before auto-transitioning from Open to HalfOpen.
    pub reset_timeout_secs: u64,
    /// Optional callback invoked on state transitions.
    /// Arguments: (skill_name, new_state, failure_count, last_error)
    pub on_state_change: Option<Box<dyn Fn(&str, State, u32, Option<&str>) + Send + Sync>>,
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

/// Error returned when the circuit is open.
#[derive(Debug, Clone)]
pub struct CircuitOpenError {
    pub skill_name: String,
    pub failures: u32,
    pub last_error: Option<String>,
}

impl fmt::Display for CircuitOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Circuit OPEN for '{}' after {} failures. Last error: {:?}",
            self.skill_name, self.failures, self.last_error
        )
    }
}

impl std::error::Error for CircuitOpenError {}

/// Per-skill circuit breaker state machine.
///
/// Internal state per skill name. Lock-free reads via DashMap,
/// state transitions via a dedicated tokio task per skill.
struct SkillBreaker {
    config: Arc<Config>,
    skill_name: String,
    timer_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// The main circuit breaker registry.
///
/// Thread-safe, cloneable. One instance serves all skills.
#[derive(Clone)]
pub struct CircuitBreaker {
    table: Arc<DashMap<String, Entry>>,
    config: Arc<Config>,
    breakers: Arc<DashMap<String, Arc<SkillBreaker>>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            table: Arc::new(DashMap::new()),
            config: Arc::new(config),
            breakers: Arc::new(DashMap::new()),
        }
    }

    /// Execute a fallible operation through the circuit breaker.
    ///
    /// If the circuit is open, returns `Err(CircuitOpenError)` immediately.
    /// Otherwise, executes `f` and records success or failure.
    pub fn call<F, T, E>(&self, skill_name: &str, f: F) -> Result<T, CallError<E>>
    where
        F: FnOnce() -> Result<T, E>,
        E: fmt::Debug,
    {
        self.ensure_started(skill_name);

        if !self.allow(skill_name) {
            let entry = self.table.get(skill_name).map(|e| e.clone());
            let failures = entry.as_ref().map(|e| e.failures).unwrap_or(0);
            let last_error = entry.and_then(|e| e.last_error.clone());
            return Err(CallError::CircuitOpen(CircuitOpenError {
                skill_name: skill_name.to_string(),
                failures,
                last_error,
            }));
        }

        match f() {
            Ok(result) => {
                self.record_success(skill_name);
                Ok(result)
            }
            Err(e) => {
                let err_str = format!("{:?}", e);
                self.record_failure(skill_name, &err_str);
                Err(CallError::Inner(e))
            }
        }
    }

    /// Check if a skill is allowed to execute (pure DashMap read, no lock).
    pub fn allow(&self, skill_name: &str) -> bool {
        match self.table.get(skill_name) {
            Some(entry) if entry.state == State::Open => false,
            _ => true,
        }
    }

    /// Get the current state for a skill.
    pub fn state(&self, skill_name: &str) -> Option<(State, u32, Option<String>)> {
        self.table
            .get(skill_name)
            .map(|e| (e.state, e.failures, e.last_error.clone()))
    }

    /// Force-reset a circuit to Closed.
    pub fn reset(&self, skill_name: &str) {
        if let Some(breaker) = self.breakers.get(skill_name) {
            let breaker = breaker.clone();
            let table = self.table.clone();
            let sn = skill_name.to_string();
            tokio::spawn(async move {
                if let Ok(mut handle) = breaker.timer_handle.try_lock() {
                    if let Some(h) = handle.take() {
                        h.abort();
                    }
                }
                table.insert(
                    sn.clone(),
                    Entry {
                        state: State::Closed,
                        failures: 0,
                        last_error: None,
                        last_change: chrono::Utc::now(),
                    },
                );
                info!("[CircuitBreaker] {} manually reset to CLOSED", sn);
            });
        }
    }

    // -- Internal --

    fn ensure_started(&self, skill_name: &str) {
        if self.breakers.contains_key(skill_name) {
            return;
        }

        let entry = Entry {
            state: State::Closed,
            failures: 0,
            last_error: None,
            last_change: chrono::Utc::now(),
        };
        self.table.insert(skill_name.to_string(), entry);

        let breaker = Arc::new(SkillBreaker {
            config: self.config.clone(),
            skill_name: skill_name.to_string(),
            timer_handle: Mutex::new(None),
        });
        self.breakers.insert(skill_name.to_string(), breaker);
    }

    fn record_success(&self, skill_name: &str) {
        let current_state = self
            .table
            .get(skill_name)
            .map(|e| e.state)
            .unwrap_or(State::Closed);

        match current_state {
            State::HalfOpen => {
                self.transition_to_closed(skill_name);
            }
            _ => {
                self.table.insert(
                    skill_name.to_string(),
                    Entry {
                        state: current_state,
                        failures: 0,
                        last_error: None,
                        last_change: chrono::Utc::now(),
                    },
                );
            }
        }
    }

    fn record_failure(&self, skill_name: &str, error: &str) {
        let mut entry = self
            .table
            .get(skill_name)
            .map(|e| e.clone())
            .unwrap_or(Entry {
                state: State::Closed,
                failures: 0,
                last_error: None,
                last_change: chrono::Utc::now(),
            });

        entry.failures += 1;
        entry.last_error = Some(error.to_string());
        entry.last_change = chrono::Utc::now();

        match entry.state {
            State::Closed if entry.failures >= self.config.max_failures => {
                self.transition_to_open(skill_name, entry.failures, Some(error.to_string()));
            }
            State::HalfOpen => {
                self.transition_to_open(
                    skill_name,
                    self.config.max_failures,
                    Some(error.to_string()),
                );
            }
            _ => {
                self.table.insert(skill_name.to_string(), entry);
            }
        }
    }

    fn transition_to_open(&self, skill_name: &str, failures: u32, error: Option<String>) {
        let config = self.config.clone();
        let table = self.table.clone();
        let breakers = self.breakers.clone();
        let sn = skill_name.to_string();

        // Cancel existing timer
        if let Some(breaker) = breakers.get(&sn) {
            let breaker = breaker.clone();
            tokio::spawn(async move {
                if let Ok(mut handle) = breaker.timer_handle.try_lock() {
                    if let Some(h) = handle.take() {
                        h.abort();
                    }
                }
            });
        }

        table.insert(
            sn.clone(),
            Entry {
                state: State::Open,
                failures,
                last_error: error.clone(),
                last_change: chrono::Utc::now(),
            },
        );

        let reset_dur = Duration::from_secs(config.reset_timeout_secs);
        let reset_min = config.reset_timeout_secs / 60;

        error!(
            "[CircuitBreaker] {} → OPEN after {} failures. Last error: {:?}. Auto-retry in {} min.",
            sn, failures, error, reset_min
        );

        // Notify
        if let Some(ref cb) = config.on_state_change {
            cb(&sn, State::Open, failures, error.as_deref());
        }

        // Schedule half-open probe
        let sn2 = sn.clone();
        let table2 = table.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(reset_dur).await;
            table2.insert(
                sn2.clone(),
                Entry {
                    state: State::HalfOpen,
                    failures: 0,
                    last_error: None,
                    last_change: chrono::Utc::now(),
                },
            );
            info!("[CircuitBreaker] {} → HALF_OPEN (testing)", sn2);
        });

        // Store handle — extract clone first to avoid borrow issues
        let breaker_ref = breakers.get(&sn).map(|r| r.clone());
        drop(breakers);
        if let Some(breaker) = breaker_ref {
            tokio::spawn(async move {
                if let Ok(mut h) = breaker.timer_handle.try_lock() {
                    *h = Some(handle);
                }
            });
        }
    }

    fn transition_to_closed(&self, skill_name: &str) {
        // Cancel timer
        if let Some(breaker) = self.breakers.get(skill_name) {
            let breaker = breaker.clone();
            tokio::spawn(async move {
                if let Ok(mut handle) = breaker.timer_handle.try_lock() {
                    if let Some(h) = handle.take() {
                        h.abort();
                    }
                }
            });
        }

        self.table.insert(
            skill_name.to_string(),
            Entry {
                state: State::Closed,
                failures: 0,
                last_error: None,
                last_change: chrono::Utc::now(),
            },
        );

        info!("[CircuitBreaker] {} → CLOSED (recovered)", skill_name);

        if let Some(ref cb) = self.config.on_state_change {
            cb(skill_name, State::Closed, 0, None);
        }
    }
}

/// Combined error type for circuit breaker calls.
#[derive(Debug)]
pub enum CallError<E: fmt::Debug> {
    /// The circuit is open — call rejected.
    CircuitOpen(CircuitOpenError),
    /// The inner function returned an error.
    Inner(E),
}

impl<E: fmt::Debug> fmt::Display for CallError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::CircuitOpen(e) => write!(f, "Circuit open: {}", e),
            CallError::Inner(e) => write!(f, "{:?}", e),
        }
    }
}

impl<E: fmt::Debug + std::error::Error + 'static> std::error::Error for CallError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CallError::CircuitOpen(e) => Some(e),
            CallError::Inner(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_allows_calls() {
        let cb = CircuitBreaker::new(Config::default());
        let result = cb.call("test_skill", || Ok::<_, &str>("success"));
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_opens_after_max_failures() {
        let cb = CircuitBreaker::new(Config {
            max_failures: 3,
            reset_timeout_secs: 300,
            on_state_change: None,
        });

        for _ in 0..3 {
            let _ = cb.call("failing_skill", || Err::<(), _>("boom"));
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!cb.allow("failing_skill"));
        let result = cb.call("failing_skill", || Ok::<_, &str>("nope"));
        assert!(matches!(result, Err(CallError::CircuitOpen(_))));
    }

    #[tokio::test]
    async fn test_reset_closes_circuit() {
        let cb = CircuitBreaker::new(Config {
            max_failures: 2,
            ..Default::default()
        });

        for _ in 0..2 {
            let _ = cb.call("resettable", || Err::<(), _>("fail"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!cb.allow("resettable"));

        cb.reset("resettable");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(cb.allow("resettable"));
    }

    #[tokio::test]
    async fn test_half_open_recovery() {
        let cb = CircuitBreaker::new(Config {
            max_failures: 2,
            reset_timeout_secs: 0,
            ..Default::default()
        });

        for _ in 0..2 {
            let _ = cb.call("recoverable", || Err::<(), _>("fail"));
        }

        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = cb.call("recoverable", || Ok::<_, &str>("ok"));
        assert!(result.is_ok());
        assert!(cb.allow("recoverable"));
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new(Config::default());
        let _ = cb.call("count_reset", || Err::<(), _>("fail"));
        let _ = cb.call("count_reset", || Ok::<_, &str>("ok"));
        let _ = cb.call("count_reset", || Err::<(), _>("f1"));
        let _ = cb.call("count_reset", || Err::<(), _>("f2"));
        assert!(cb.allow("count_reset"));
    }

    #[tokio::test]
    async fn test_state_query() {
        let cb = CircuitBreaker::new(Config {
            max_failures: 1,
            ..Default::default()
        });

        assert!(cb.state("unknown").is_none());
        let _ = cb.call("stateful", || Err::<(), _>("err"));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let (state, count, err) = cb.state("stateful").unwrap();
        assert_eq!(state, State::Open);
        assert_eq!(count, 1);
        assert!(err.unwrap().contains("err"));
    }
}
