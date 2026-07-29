//! Bounded retry with exponential backoff and jitter (MED-001, MED-009).
//!
//! # Why this exists
//!
//! Before this module a single transient upstream failure aborted an
//! autonomous run: `LlmClient` made exactly one provider call and propagated
//! whatever came back. `LlmError::RateLimited` existed but was constructed
//! nowhere outside a unit test, because no provider inspected the HTTP status
//! at all — a 429 or a 503 was fed straight into `.json()` and surfaced as
//! `LlmError::Serialization`. Retrying without first fixing that would have
//! retried on the wrong signal, so the two changes land together.
//!
//! # The policy
//!
//! One [`RetryPolicy`] governs the provider layer. It is deliberately public
//! and its classification ([`LlmError::is_retryable`]) is deliberately a method
//! on the error rather than private logic here, so an outer layer — the agent
//! loop's replan/abort decision — can ask the same question of the same error
//! and reach the same answer instead of maintaining a second, divergent
//! opinion. That is the coherence half of MED-009.

use std::time::Duration;

use crate::error::LlmError;

/// How long to wait before an attempt, or why not to make one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Wait this long, then try again.
    After(Duration),
    /// Do not retry.
    Stop(StopReason),
}

/// Why a retry loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The error cannot be fixed by repeating the identical request.
    NotRetryable,
    /// [`RetryPolicy::max_attempts`] is spent.
    AttemptsExhausted,
    /// The server asked us to wait longer than [`RetryPolicy::max_backoff`].
    ///
    /// Waiting it out would park an autonomous run for the interval the server
    /// named; retrying sooner than instructed would just earn another 429 and
    /// burn an attempt for nothing. Neither is useful, so the error is returned
    /// to the caller with the server's own interval intact — a layer that can
    /// actually decide to wait that long (a scheduler, an operator) gets to
    /// make that call instead of this one.
    ServerBackoffTooLong,
}

/// Bounded exponential backoff with jitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    /// Total attempts including the first. `1` disables retrying.
    pub max_attempts: u32,
    /// Delay before the second attempt.
    pub initial_backoff: Duration,
    /// Ceiling for any single wait, and for a server-supplied `Retry-After`.
    pub max_backoff: Duration,
    /// Growth factor per attempt.
    pub multiplier: f64,
    /// Whether to decorrelate concurrent callers.
    ///
    /// Without it, N requests that fail together retry together, reproducing
    /// the burst that caused the failure. Off only for tests that assert exact
    /// intervals.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// One attempt, no retries.
    pub fn disabled() -> Self {
        Self {
            max_attempts: 1,
            ..Default::default()
        }
    }

    /// Whether this policy would ever retry.
    pub fn is_enabled(&self) -> bool {
        self.max_attempts > 1
    }

    /// Decide what to do after `error` on attempt `attempt` (1-based).
    pub fn decide(&self, attempt: u32, error: &LlmError) -> RetryDecision {
        if !error.is_retryable() {
            return RetryDecision::Stop(StopReason::NotRetryable);
        }
        if attempt >= self.max_attempts {
            return RetryDecision::Stop(StopReason::AttemptsExhausted);
        }
        if let Some(hint) = error.retry_after() {
            if hint > self.max_backoff {
                return RetryDecision::Stop(StopReason::ServerBackoffTooLong);
            }
            // The server's own interval wins over our curve: it knows when its
            // limit resets and we are guessing. Jitter still applies, so a
            // fleet told "retry in 2s" does not stampede at exactly 2s.
            return RetryDecision::After(self.apply_jitter(hint));
        }
        RetryDecision::After(self.apply_jitter(self.base_backoff(attempt)))
    }

    /// The un-jittered delay after `attempt` failures, clamped to
    /// [`Self::max_backoff`].
    pub fn base_backoff(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(32);
        let scaled = self.initial_backoff.as_secs_f64() * self.multiplier.powi(exponent as i32);
        // A non-finite or negative product (a nonsense multiplier) must not
        // become a nonsense Duration — `from_secs_f64` panics on those.
        if !scaled.is_finite() || scaled <= 0.0 {
            return self.initial_backoff.min(self.max_backoff);
        }
        Duration::from_secs_f64(scaled).min(self.max_backoff)
    }

    /// Equal jitter: half the delay is fixed, half is random.
    ///
    /// Chosen over full jitter (`random(0, base)`) because it keeps a
    /// guaranteed minimum spacing — full jitter can return a near-zero wait and
    /// hammer a provider that just asked for room — while still spreading
    /// concurrent callers across the window.
    fn apply_jitter(&self, base: Duration) -> Duration {
        if !self.jitter {
            return base;
        }
        let half = base / 2;
        let spread = base.saturating_sub(half);
        if spread.is_zero() {
            return base;
        }
        let factor: f64 = rand::random::<f64>();
        half + spread.mul_f64(factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact() -> RetryPolicy {
        RetryPolicy {
            jitter: false,
            ..Default::default()
        }
    }

    #[test]
    fn backoff_grows_exponentially_and_is_clamped() {
        let p = RetryPolicy {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(800),
            multiplier: 2.0,
            ..exact()
        };
        assert_eq!(p.base_backoff(1), Duration::from_millis(100));
        assert_eq!(p.base_backoff(2), Duration::from_millis(200));
        assert_eq!(p.base_backoff(3), Duration::from_millis(400));
        assert_eq!(p.base_backoff(4), Duration::from_millis(800));
        assert_eq!(
            p.base_backoff(5),
            Duration::from_millis(800),
            "the ceiling holds rather than growing without bound"
        );
        assert_eq!(
            p.base_backoff(4_000_000_000),
            Duration::from_millis(800),
            "a huge attempt number must clamp, not overflow"
        );
    }

    #[test]
    fn a_nonsense_multiplier_does_not_panic() {
        for multiplier in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let p = RetryPolicy {
                multiplier,
                ..exact()
            };
            // `Duration::from_secs_f64` panics on negative/non-finite input,
            // so this asserts the guard rather than the value.
            let _ = p.base_backoff(3);
        }
    }

    #[test]
    fn a_permanent_error_is_never_retried() {
        let p = exact();
        assert_eq!(
            p.decide(1, &LlmError::Auth("bad key".into())),
            RetryDecision::Stop(StopReason::NotRetryable)
        );
        assert_eq!(
            p.decide(1, &LlmError::Unsupported("no embeddings".into())),
            RetryDecision::Stop(StopReason::NotRetryable)
        );
        assert_eq!(
            p.decide(
                1,
                &LlmError::BudgetExceeded {
                    goal_id: "g".into(),
                    used: 2,
                    budget: 1
                }
            ),
            RetryDecision::Stop(StopReason::NotRetryable)
        );
    }

    #[test]
    fn a_transient_error_is_retried_until_attempts_run_out() {
        let p = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            ..exact()
        };
        let err = LlmError::Network("connection reset".into());
        assert_eq!(
            p.decide(1, &err),
            RetryDecision::After(Duration::from_millis(100))
        );
        assert_eq!(
            p.decide(2, &err),
            RetryDecision::After(Duration::from_millis(200))
        );
        assert_eq!(
            p.decide(3, &err),
            RetryDecision::Stop(StopReason::AttemptsExhausted),
            "the third attempt is the last, so there is no fourth"
        );
    }

    #[test]
    fn a_server_retry_after_overrides_the_curve() {
        let p = RetryPolicy {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            ..exact()
        };
        assert_eq!(
            p.decide(
                1,
                &LlmError::RateLimited {
                    retry_after: Some(7)
                }
            ),
            RetryDecision::After(Duration::from_secs(7)),
            "the server knows when its limit resets; we are guessing"
        );
    }

    #[test]
    fn a_server_backoff_beyond_the_ceiling_stops_rather_than_waiting_or_ignoring() {
        let p = RetryPolicy {
            max_backoff: Duration::from_secs(30),
            ..exact()
        };
        assert_eq!(
            p.decide(
                1,
                &LlmError::RateLimited {
                    retry_after: Some(3600)
                }
            ),
            RetryDecision::Stop(StopReason::ServerBackoffTooLong),
            "neither park the run for an hour nor retry early and earn another 429"
        );
    }

    #[test]
    fn a_rate_limit_without_a_hint_falls_back_to_the_curve() {
        let p = RetryPolicy {
            initial_backoff: Duration::from_millis(250),
            ..exact()
        };
        assert_eq!(
            p.decide(1, &LlmError::RateLimited { retry_after: None }),
            RetryDecision::After(Duration::from_millis(250))
        );
    }

    #[test]
    fn disabled_policy_never_retries_even_a_transient_error() {
        let p = RetryPolicy::disabled();
        assert!(!p.is_enabled());
        assert_eq!(
            p.decide(1, &LlmError::Timeout("slow".into())),
            RetryDecision::Stop(StopReason::AttemptsExhausted)
        );
    }

    #[test]
    fn jitter_stays_within_the_equal_jitter_window() {
        let p = RetryPolicy {
            initial_backoff: Duration::from_millis(1000),
            jitter: true,
            ..Default::default()
        };
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let RetryDecision::After(d) = p.decide(1, &LlmError::Network("x".into())) else {
                panic!("expected a retry");
            };
            assert!(
                d >= Duration::from_millis(500) && d <= Duration::from_millis(1000),
                "equal jitter must stay in [base/2, base]; got {d:?}"
            );
            seen.insert(d.as_micros());
        }
        assert!(
            seen.len() > 1,
            "jitter must actually vary, or concurrent callers retry in lockstep"
        );
    }

    #[test]
    fn jitter_disabled_is_exactly_reproducible() {
        let p = exact();
        let first = p.decide(1, &LlmError::Network("x".into()));
        for _ in 0..50 {
            assert_eq!(p.decide(1, &LlmError::Network("x".into())), first);
        }
    }
}
