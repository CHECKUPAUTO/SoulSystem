#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Adaptive rate limiter that slows down on errors and speeds up on success.
#[derive(Debug)]
pub struct AdaptiveRateLimiter {
    /// Current delay in milliseconds.
    current_ms: AtomicU64,
    /// Minimum delay (floor).
    min_ms: u64,
    /// Maximum delay (ceiling).
    max_ms: u64,
    /// Backoff multiplier on 429/503 errors.
    backoff_factor: f64,
    /// Recovery divisor on successful requests.
    recovery_factor: f64,
}

impl AdaptiveRateLimiter {
    pub fn new(min_ms: u64, max_ms: u64) -> Self {
        Self {
            current_ms: AtomicU64::new(min_ms),
            min_ms,
            max_ms,
            backoff_factor: 2.0,
            recovery_factor: 1.1,
        }
    }

    /// Get the current delay duration.
    pub fn delay(&self) -> Duration {
        Duration::from_millis(self.current_ms.load(Ordering::Relaxed))
    }

    /// Call this after a successful request.
    pub fn on_success(&self) {
        let current = self.current_ms.load(Ordering::Relaxed);
        let reduced = (current as f64 / self.recovery_factor).ceil() as u64;
        let new = reduced.max(self.min_ms);
        self.current_ms.store(new, Ordering::Relaxed);
    }

    /// Call this after receiving a rate-limit or server-error response.
    pub fn on_rate_limited(&self) {
        let current = self.current_ms.load(Ordering::Relaxed);
        let increased = (current as f64 * self.backoff_factor).ceil() as u64;
        let new = increased.min(self.max_ms);
        self.current_ms.store(new, Ordering::Relaxed);
    }
}
