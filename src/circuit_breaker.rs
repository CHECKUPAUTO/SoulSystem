//! Circuit Breaker — pattern de protection pour les bridges
//!
//! Trois états :
//!   - Closed    : appels normaux
//!   - Open      : appels rejetés immédiatement (service considéré down)
//!   - HalfOpen  : 1 essai autorisé, si OK → Closed, sinon → Open
//!
//! Transitions :
//!   Closed   --(failure_threshold atteint)-->  Open
//!   Open     --(reset_timeout expiré)------>   HalfOpen
//!   HalfOpen --(succès)-------------------->   Closed
//!   HalfOpen --(échec)-------------------->   Open

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub reset_timeout: Duration,
    pub name: String,
}

impl CircuitBreakerConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(60),
            name: name.into(),
        }
    }
    pub fn with_threshold(mut self, n: u32) -> Self {
        self.failure_threshold = n;
        self
    }
    pub fn with_reset_timeout(mut self, d: Duration) -> Self {
        self.reset_timeout = d;
        self
    }
}

#[derive(Debug)]
struct CircuitBreakerInner {
    state: CircuitState,
    consecutive_failures: u32,
    last_failure_at: Option<Instant>,
    last_state_change: Instant,
}

impl CircuitBreakerInner {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_failure_at: None,
            last_state_change: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    inner: Arc<Mutex<CircuitBreakerInner>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            inner: Arc::new(Mutex::new(CircuitBreakerInner::new())),
        }
    }

    /// Tente d'appeler. Renvoie Err si le circuit est ouvert.
    pub async fn try_call<F, Fut, T>(&self, f: F) -> Result<T, CircuitError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        // Check state first
        {
            let mut inner = self.inner.lock().await;
            match inner.state {
                CircuitState::Open => {
                    if let Some(last) = inner.last_failure_at {
                        if last.elapsed() >= self.config.reset_timeout {
                            // Transition to HalfOpen
                            inner.state = CircuitState::HalfOpen;
                            inner.last_state_change = Instant::now();
                            info!(
                                "🔄 circuit '{}' open → half_open after {}s",
                                self.config.name,
                                last.elapsed().as_secs()
                            );
                        } else {
                            return Err(CircuitError::Open {
                                name: self.config.name.clone(),
                                retry_after: self.config.reset_timeout - last.elapsed(),
                            });
                        }
                    }
                }
                CircuitState::HalfOpen | CircuitState::Closed => {}
            }
        }

        // Try the actual call
        match f().await {
            Ok(value) => {
                let mut inner = self.inner.lock().await;
                if inner.state == CircuitState::HalfOpen || inner.consecutive_failures > 0 {
                    inner.state = CircuitState::Closed;
                    inner.consecutive_failures = 0;
                    inner.last_state_change = Instant::now();
                    info!("✓ circuit '{}' closed (recovered)", self.config.name);
                }
                Ok(value)
            }
            Err(e) => {
                let mut inner = self.inner.lock().await;
                inner.consecutive_failures += 1;
                inner.last_failure_at = Some(Instant::now());
                let was_half_open = inner.state == CircuitState::HalfOpen;
                if was_half_open || inner.consecutive_failures >= self.config.failure_threshold {
                    inner.state = CircuitState::Open;
                    inner.last_state_change = Instant::now();
                    warn!(
                        "✗ circuit '{}' → OPEN ({} consecutive failures: {})",
                        self.config.name, inner.consecutive_failures, e
                    );
                }
                Err(CircuitError::CallFailed(e))
            }
        }
    }

    /// Renvoie l'état actuel.
    pub async fn state(&self) -> CircuitState {
        self.inner.lock().await.state
    }

    /// Renvoie les stats courantes.
    pub async fn stats(&self) -> CircuitStats {
        let inner = self.inner.lock().await;
        CircuitStats {
            name: self.config.name.clone(),
            state: inner.state,
            consecutive_failures: inner.consecutive_failures,
            last_failure_secs_ago: inner.last_failure_at.map(|t| t.elapsed().as_secs()),
            last_state_change_secs_ago: inner.last_state_change.elapsed().as_secs(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitStats {
    pub name: String,
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub last_failure_secs_ago: Option<u64>,
    pub last_state_change_secs_ago: u64,
}

#[derive(Debug)]
pub enum CircuitError {
    Open { name: String, retry_after: Duration },
    CallFailed(anyhow::Error),
}

impl std::fmt::Display for CircuitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitError::Open { name, retry_after } => {
                write!(
                    f,
                    "circuit '{}' is open, retry in {}s",
                    name,
                    retry_after.as_secs()
                )
            }
            CircuitError::CallFailed(e) => write!(f, "call failed: {}", e),
        }
    }
}

impl std::error::Error for CircuitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::new("test"));
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_threshold() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::new("test").with_threshold(3));
        for _ in 0..3 {
            let _ = cb
                .try_call(|| async { Err::<(), _>(anyhow::anyhow!("boom")) })
                .await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_recovers() {
        let cb = CircuitBreaker::new(
            CircuitBreakerConfig::new("test")
                .with_threshold(2)
                .with_reset_timeout(Duration::from_millis(10)),
        );
        for _ in 0..2 {
            let _ = cb
                .try_call(|| async { Err::<(), _>(anyhow::anyhow!("boom")) })
                .await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = cb
            .try_call(|| async { Ok::<_, anyhow::Error>(42) })
            .await
            .unwrap();
        assert_eq!(cb.state().await, CircuitState::Closed);
    }
}
