//! Token budget management with usage tracking and alerting.
//!
//! A `TokenBudget` tracks token consumption against a configured maximum,
//! emitting warnings at configurable thresholds to help avoid unexpected
//! LLM API costs or resource exhaustion.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{info, warn};

/// Errors that can occur during budget operations.
#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    /// Attempted to consume more tokens than remain in the budget.
    #[error("token budget exceeded: attempted {attempted}, remaining {remaining}, max {max}")]
    Exceeded {
        /// Number of tokens that were attempted to consume.
        attempted: u64,
        /// Tokens still available before the attempt.
        remaining: u64,
        /// Total budget capacity.
        max: u64,
    },
}

/// Result of a token consumption operation.
#[derive(Debug, Clone, Copy)]
pub enum ConsumeResult {
    /// Tokens consumed successfully.
    Ok {
        /// Tokens consumed in this operation.
        consumed: u64,
        /// Tokens remaining after consumption.
        remaining: u64,
    },
    /// A threshold was crossed. The consumption still succeeded but
    /// the caller should consider taking action (e.g., switching to a
    /// cheaper model or summarising context).
    ThresholdCrossed {
        /// Tokens consumed in this operation.
        consumed: u64,
        /// Tokens remaining after consumption.
        remaining: u64,
        /// The threshold percentage that was crossed (50, 80, or 90).
        threshold_pct: u8,
    },
}

/// Alert thresholds as percentages of the budget.
///
/// Alerts fire when cumulative consumption **crosses** one of these
/// thresholds — they do not fire repeatedly for the same usage level.
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Fire an alert when 50% of the budget has been consumed.
    pub at_50: bool,
    /// Fire an alert when 80% of the budget has been consumed.
    pub at_80: bool,
    /// Fire an alert when 90% of the budget has been consumed.
    pub at_90: bool,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            at_50: true,
            at_80: true,
            at_90: true,
        }
    }
}

/// A thread-safe token budget.
///
/// Designed to be shared across async tasks. All mutation uses atomic
/// operations, so sharing via `Arc` (no `Mutex`) is perfectly fine.
///
/// # Examples
///
/// ```
/// use avid_tokenjuice::TokenBudget;
///
/// let budget = TokenBudget::new(10_000);
/// assert_eq!(budget.remaining(), 10_000);
///
/// let result = budget.consume(1_000).unwrap();
/// assert!(matches!(result, avid_tokenjuice::ConsumeResult::Ok { .. }));
/// assert_eq!(budget.remaining(), 9_000);
///
/// // Consuming beyond max returns an error
/// let err = budget.consume(20_000).unwrap_err();
/// ```
#[derive(Debug, Clone)]
pub struct TokenBudget {
    inner: Arc<BudgetInner>,
}

#[derive(Debug)]
struct BudgetInner {
    max: u64,
    consumed: AtomicU64,
    alert_50_fired: AtomicU64, // 0 = not fired, 1 = fired
    alert_80_fired: AtomicU64,
    alert_90_fired: AtomicU64,
    thresholds: AlertThresholds,
}

impl TokenBudget {
    /// Create a new budget with the given maximum token count.
    ///
    /// # Examples
    ///
    /// ```
    /// use avid_tokenjuice::TokenBudget;
    /// let budget = TokenBudget::new(1_000_000);
    /// ```
    #[must_use]
    pub fn new(max_tokens: u64) -> Self {
        Self {
            inner: Arc::new(BudgetInner {
                max: max_tokens,
                consumed: AtomicU64::new(0),
                alert_50_fired: AtomicU64::new(0),
                alert_80_fired: AtomicU64::new(0),
                alert_90_fired: AtomicU64::new(0),
                thresholds: AlertThresholds::default(),
            }),
        }
    }

    /// Create a budget with custom alert thresholds.
    #[must_use]
    pub fn with_thresholds(max_tokens: u64, thresholds: AlertThresholds) -> Self {
        Self {
            inner: Arc::new(BudgetInner {
                max: max_tokens,
                consumed: AtomicU64::new(0),
                alert_50_fired: AtomicU64::new(0),
                alert_80_fired: AtomicU64::new(0),
                alert_90_fired: AtomicU64::new(0),
                thresholds,
            }),
        }
    }

    /// Maximum number of tokens this budget allows.
    #[must_use]
    pub fn max(&self) -> u64 {
        self.inner.max
    }

    /// Number of tokens consumed so far.
    #[must_use]
    pub fn consumed(&self) -> u64 {
        self.inner.consumed.load(Ordering::Relaxed)
    }

    /// Number of tokens remaining in the budget.
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.inner
            .max
            .saturating_sub(self.inner.consumed.load(Ordering::Relaxed))
    }

    /// Percentage of the budget that has been consumed (0–100).
    #[must_use]
    pub fn usage_pct(&self) -> f64 {
        let consumed = self.inner.consumed.load(Ordering::Relaxed) as f64;
        let max = self.inner.max as f64;
        if max == 0.0 {
            return 100.0;
        }
        ((consumed / max) * 100.0).min(100.0)
    }

    /// Consume `count` tokens from the budget.
    ///
    /// # Errors
    ///
    /// Returns `BudgetError::Exceeded` if the attempted consumption would
    /// push total usage beyond `max_tokens`.
    ///
    /// # Threshold Alerts
    ///
    /// Returns `ConsumeResult::ThresholdCrossed` when cumulative consumption
    /// crosses 50%, 80%, or 90% of the budget. Each threshold fires at most
    /// once. When a threshold is crossed, a `warn!` log is emitted.
    pub fn consume(&self, count: u64) -> Result<ConsumeResult, BudgetError> {
        let before = self.inner.consumed.load(Ordering::Relaxed);
        let after = before + count;

        if after > self.inner.max {
            return Err(BudgetError::Exceeded {
                attempted: count,
                remaining: self.inner.max.saturating_sub(before),
                max: self.inner.max,
            });
        }

        // Atomically increment
        let actual_before = self.inner.consumed.fetch_add(count, Ordering::Relaxed);
        let actual_after = actual_before + count;
        let remaining = self.inner.max.saturating_sub(actual_after);

        // Check thresholds based on actual consumption
        let max = self.inner.max as f64;
        let pct = (actual_after as f64 / max) * 100.0;

        let crossed = if self.inner.thresholds.at_50
            && pct >= 50.0
            && actual_before < (self.inner.max * 50 / 100)
            && self
                .inner
                .alert_50_fired
                .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            warn!(
                consumed = actual_after,
                remaining = remaining,
                max = self.inner.max,
                "token budget at 50%"
            );
            Some(50)
        } else if self.inner.thresholds.at_80
            && pct >= 80.0
            && actual_before < (self.inner.max * 80 / 100)
            && self
                .inner
                .alert_80_fired
                .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            warn!(
                consumed = actual_after,
                remaining = remaining,
                max = self.inner.max,
                "token budget at 80% — consider summarising context"
            );
            Some(80)
        } else if self.inner.thresholds.at_90
            && pct >= 90.0
            && actual_before < (self.inner.max * 90 / 100)
            && self
                .inner
                .alert_90_fired
                .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            warn!(
                consumed = actual_after,
                remaining = remaining,
                max = self.inner.max,
                "token budget at 90% — near exhaustion!"
            );
            Some(90)
        } else {
            None
        };

        info!(
            consumed = actual_after,
            remaining = remaining,
            max = self.inner.max,
            "budget consumed {count} tokens"
        );

        crossed.map_or(
            Ok(ConsumeResult::Ok {
                consumed: count,
                remaining,
            }),
            |threshold_pct| {
                Ok(ConsumeResult::ThresholdCrossed {
                    consumed: count,
                    remaining,
                    threshold_pct,
                })
            },
        )
    }

    /// Reset the budget to zero consumption, clearing threshold flags.
    pub fn reset(&self) {
        self.inner.consumed.store(0, Ordering::Relaxed);
        self.inner.alert_50_fired.store(0, Ordering::Relaxed);
        self.inner.alert_80_fired.store(0, Ordering::Relaxed);
        self.inner.alert_90_fired.store(0, Ordering::Relaxed);
        info!("token budget reset (max={})", self.inner.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_budget_has_full_capacity() {
        let b = TokenBudget::new(1_000);
        assert_eq!(b.max(), 1_000);
        assert_eq!(b.remaining(), 1_000);
        assert_eq!(b.consumed(), 0);
        assert!((b.usage_pct() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn consume_reduces_remaining() {
        let b = TokenBudget::new(1_000);
        let result = b.consume(300).unwrap();
        assert!(matches!(result, ConsumeResult::Ok { .. }));
        assert_eq!(b.remaining(), 700);
        assert_eq!(b.consumed(), 300);
    }

    #[test]
    fn consume_to_exact_max() {
        let b = TokenBudget::new(100);
        b.consume(100).unwrap();
        assert_eq!(b.remaining(), 0);
    }

    #[test]
    fn exceed_returns_error() {
        let b = TokenBudget::new(100);
        let err = b.consume(101).unwrap_err();
        assert!(matches!(err, BudgetError::Exceeded { .. }));
    }

    #[test]
    fn cumulative_exceed() {
        let b = TokenBudget::new(100);
        b.consume(60).unwrap();
        let err = b.consume(41).unwrap_err();
        assert!(matches!(err, BudgetError::Exceeded { .. }));
        assert_eq!(b.consumed(), 60); // second consume never applied
    }

    #[test]
    fn reset_restores_full_capacity() {
        let b = TokenBudget::new(1_000);
        b.consume(500).unwrap();
        assert_eq!(b.remaining(), 500);
        b.reset();
        assert_eq!(b.remaining(), 1_000);
        assert_eq!(b.consumed(), 0);
    }

    #[test]
    fn usage_pct_calculates_correctly() {
        let b = TokenBudget::new(1_000);
        b.consume(250).unwrap();
        assert!((b.usage_pct() - 25.0).abs() < 0.01);
    }

    #[test]
    fn zero_max_budget() {
        let b = TokenBudget::new(0);
        assert!((b.usage_pct() - 100.0).abs() < f64::EPSILON);
        let err = b.consume(1).unwrap_err();
        assert!(matches!(err, BudgetError::Exceeded { .. }));
    }

    #[test]
    fn threshold_crossing_detected() {
        let b = TokenBudget::with_thresholds(
            100,
            AlertThresholds {
                at_50: true,
                at_80: false,
                at_90: false,
            },
        );

        let r1 = b.consume(40).unwrap();
        assert!(matches!(r1, ConsumeResult::Ok { .. }));

        let r2 = b.consume(15).unwrap();
        assert!(matches!(r2, ConsumeResult::ThresholdCrossed { .. }));
    }

    #[test]
    fn threshold_fires_once() {
        let b = TokenBudget::with_thresholds(
            100,
            AlertThresholds {
                at_50: true,
                at_80: false,
                at_90: false,
            },
        );

        b.consume(30).unwrap(); // 30% → no crossing
        let r = b.consume(25).unwrap(); // 55% → crosses 50%
        assert!(matches!(r, ConsumeResult::ThresholdCrossed { .. }));

        // Next consumption should NOT re-fire the 50% alert
        let r2 = b.consume(10).unwrap(); // 65%
        assert!(matches!(r2, ConsumeResult::Ok { .. }));
    }
}
