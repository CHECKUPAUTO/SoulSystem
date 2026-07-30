//! Process-wide retry ceiling, shared per provider (MED-009-C).
//!
//! [`RetryPolicy`](crate::retry::RetryPolicy) bounds retries *within one
//! request*: three attempts, backed off and jittered. Every request is
//! individually well behaved. The gap this closes is what happens when many
//! of them fail at once — 100 concurrent runs against a provider that has
//! started returning 503 produce 300 attempts, and each run believes it is
//! being polite. Backoff spreads a burst over time; it does not bound the
//! aggregate.
//!
//! ## Why per provider rather than per process
//!
//! The resource under strain is the provider, and it is the provider that is
//! failing. A single process-wide counter would let an Ollama outage consume
//! the shared allowance and starve retries to Anthropic — coupling two
//! services that have nothing to do with each other, and turning one
//! provider's bad day into a general degradation. The bucket is therefore
//! keyed by provider identity.
//!
//! Identity is `name@base_url`, not name alone: two Ollama endpoints on
//! different hosts are different providers, and sharing a ceiling between
//! them would rate-limit a healthy one because its neighbour is down.
//!
//! ## Why a token bucket
//!
//! A fixed counter would have to be reset by someone, and "who resets it" has
//! no good answer in a long-running process. A bucket refills on its own at a
//! rate that is the thing actually being chosen: *how many retries per second
//! this process may aim at one provider, sustained*. Capacity is the burst it
//! tolerates before that rate binds.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Sustained retries per second, per provider, across the whole process.
const DEFAULT_REFILL_PER_SEC: f64 = 2.0;
/// Retries available in a burst before the sustained rate binds.
const DEFAULT_CAPACITY: f64 = 20.0;

/// A refilling allowance of retry attempts.
#[derive(Debug)]
pub struct RetryBudget {
    inner: Mutex<BucketState>,
    capacity: f64,
    refill_per_sec: f64,
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last_refill: Instant,
}

impl RetryBudget {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            // Starts full: a process that has just started has not spent
            // anything, and making the first burst wait would penalise a
            // healthy cold start for a fault that has not happened.
            inner: Mutex::new(BucketState {
                tokens: capacity,
                last_refill: Instant::now(),
            }),
            capacity,
            refill_per_sec,
        }
    }

    /// Take one retry token if the provider's allowance permits.
    pub fn try_acquire(&self) -> bool {
        self.try_acquire_at(Instant::now())
    }

    /// [`Self::try_acquire`] against an explicit clock reading.
    ///
    /// Exists so the tests can assert refill behaviour without sleeping. A
    /// test that sleeps to observe a rate is slow and flaky; one that asserts
    /// nothing about refill would leave the bucket's whole point unverified.
    pub fn try_acquire_at(&self, now: Instant) -> bool {
        let mut state = match self.inner.lock() {
            Ok(guard) => guard,
            // A poisoned lock means another thread panicked mid-update.
            // Refuse the retry rather than bypass the ceiling: the failure
            // mode this exists to prevent is *too many* retries.
            Err(_) => return false,
        };

        let elapsed = now.saturating_duration_since(state.last_refill);
        if elapsed > Duration::ZERO {
            state.tokens =
                (state.tokens + elapsed.as_secs_f64() * self.refill_per_sec).min(self.capacity);
            state.last_refill = now;
        }

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Tokens currently available, for diagnostics and tests.
    pub fn available(&self) -> f64 {
        self.inner.lock().map(|s| s.tokens).unwrap_or(0.0)
    }
}

/// Process-wide registry of per-provider buckets.
fn registry() -> &'static Mutex<HashMap<String, &'static RetryBudget>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, &'static RetryBudget>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The shared budget for one provider endpoint, creating it on first use.
///
/// Leaks each bucket deliberately: they live for the process, and the number
/// of distinct provider endpoints is small and fixed by configuration. The
/// alternative — handing out `Arc`s — would let the last holder drop the
/// bucket and silently reset the ceiling, which is exactly the accounting
/// this is supposed to keep.
pub fn shared_for(provider_name: &str, base_url: &str) -> &'static RetryBudget {
    let key = format!("{provider_name}@{base_url}");
    let mut map = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    map.entry(key).or_insert_with(|| {
        Box::leak(Box::new(RetryBudget::new(
            DEFAULT_CAPACITY,
            DEFAULT_REFILL_PER_SEC,
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_budget_starts_full_and_spends_down() {
        let budget = RetryBudget::new(3.0, 1.0);
        let now = Instant::now();
        assert!(budget.try_acquire_at(now));
        assert!(budget.try_acquire_at(now));
        assert!(budget.try_acquire_at(now));
        assert!(
            !budget.try_acquire_at(now),
            "a fourth retry must be refused: the point is an aggregate ceiling \
             that individually well-behaved requests cannot exceed"
        );
    }

    #[test]
    fn it_refills_at_the_configured_rate() {
        let budget = RetryBudget::new(3.0, 2.0);
        let start = Instant::now();
        for _ in 0..3 {
            assert!(budget.try_acquire_at(start));
        }
        assert!(!budget.try_acquire_at(start));

        // Half a second at 2/sec is exactly one token.
        let later = start + Duration::from_millis(500);
        assert!(budget.try_acquire_at(later));
        assert!(!budget.try_acquire_at(later));
    }

    #[test]
    fn refill_is_capped_at_capacity() {
        let budget = RetryBudget::new(2.0, 100.0);
        let start = Instant::now();
        assert!(budget.try_acquire_at(start));
        assert!(budget.try_acquire_at(start));

        // A long idle period must not bank unlimited retries, or the next
        // outage gets an unbounded burst — the failure this prevents.
        let much_later = start + Duration::from_secs(3600);
        assert!(budget.try_acquire_at(much_later));
        assert!(budget.try_acquire_at(much_later));
        assert!(!budget.try_acquire_at(much_later));
    }

    #[test]
    fn different_providers_do_not_share_an_allowance() {
        let a = shared_for("ollama", "http://127.0.0.1:11434");
        let b = shared_for("anthropic", "https://api.anthropic.com");

        let start = Instant::now();
        while a.try_acquire_at(start) {}
        assert!(!a.try_acquire_at(start));

        assert!(
            b.try_acquire_at(start),
            "one provider's outage must not starve retries to another"
        );
    }

    #[test]
    fn the_same_endpoint_resolves_to_one_shared_bucket() {
        let first = shared_for("ollama", "http://shared-test:11434");
        let second = shared_for("ollama", "http://shared-test:11434");
        assert!(
            std::ptr::eq(first, second),
            "two lookups for the same endpoint must return the same bucket, or \
             the ceiling is per-caller and bounds nothing"
        );
    }

    /// Same provider name, different endpoint: separate allowances.
    #[test]
    fn the_same_name_at_a_different_url_is_a_different_provider() {
        let a = shared_for("ollama", "http://host-a:11434");
        let b = shared_for("ollama", "http://host-b:11434");
        assert!(
            !std::ptr::eq(a, b),
            "two endpoints must not share a ceiling: a healthy one would be \
             rate-limited because its neighbour is down"
        );
    }
}
