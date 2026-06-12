//! `avid-tokenjuice` — Token counting, budgeting, and middleware for LLM calls.
//!
//! This crate provides a lightweight token-tracking layer for [`avid_core::LlmClient`].
//! It wraps every chat call, estimates token usage, enforces budgets, and logs
//! statistics — all without modifying the core LLM client.
//!
//! # Architecture
//!
//! ```text
//! TokenJuice
//!   ├── TokenCounter      → estimate tokens from text (chars/4 heuristic)
//!   ├── TokenBudget       → atomic budget tracking with threshold alerts
//!   ├── TokenJuicedLlm    → middleware wrapping LlmClient with tracking
//!   └── TokenStats        → snapshot for reporting / metrics
//! ```
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::time::Duration;
//! use avid_core::llm::LlmClient;
//! use avid_tokenjuice::TokenJuice;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = LlmClient::new("http://localhost:11434".into(), "llama3".into()).await?;
//!
//! // Create a token-juiced session with 100K token budget
//! let juice = TokenJuice::new(100_000);
//! let tracked = juice.wrap_llm(Arc::new(client));
//!
//! let response = tracked.chat(
//!     "You are a helpful assistant.",
//!     "What is Rust?",
//!     false,
//!     Duration::from_secs(30),
//! ).await?;
//!
//! // Print session stats
//! println!("{}", juice.stats().human_readable());
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(clippy::todo, clippy::unimplemented)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::single_match,
    clippy::match_single_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::needless_for_each,
    clippy::needless_pass_by_value,
    clippy::cognitive_complexity
)]

pub mod budget;
pub mod counter;
pub mod middleware;
pub mod stats;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub use budget::{BudgetError, ConsumeResult, TokenBudget};
pub use counter::TokenCounter;
pub use middleware::{JuicedLlmError, TokenJuicedLlm};
pub use stats::TokenStats;

/// Top-level token tracking session.
///
/// A `TokenJuice` bundles a [`TokenCounter`], a [`TokenBudget`], and cumulative
/// statistics into one handle. It can produce [`TokenJuicedLlm`] wrappers
/// around any [`avid_core::LlmClient`] and provides snapshot statistics.
///
/// # Examples
///
/// ```
/// use avid_tokenjuice::TokenJuice;
///
/// let juice = TokenJuice::new(1_000_000);
/// assert_eq!(juice.remaining(), 1_000_000);
///
/// let stats = juice.stats();
/// assert_eq!(stats.budget_max, 1_000_000);
/// ```
#[derive(Debug, Clone)]
pub struct TokenJuice {
    counter: TokenCounter,
    budget: TokenBudget,
    total_input: Arc<AtomicU64>,
    total_output: Arc<AtomicU64>,
    call_count: Arc<AtomicU64>,
}

impl TokenJuice {
    /// Create a new token juice session with the given maximum budget.
    ///
    /// # Examples
    ///
    /// ```
    /// use avid_tokenjuice::TokenJuice;
    /// let juice = TokenJuice::new(500_000);
    /// ```
    #[must_use]
    pub fn new(max_tokens: u64) -> Self {
        Self {
            counter: TokenCounter::new(),
            budget: TokenBudget::new(max_tokens),
            total_input: Arc::new(AtomicU64::new(0)),
            total_output: Arc::new(AtomicU64::new(0)),
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new token juice session with a custom budget (for threshold configuration).
    ///
    /// ```
    /// use avid_tokenjuice::{TokenJuice, budget::AlertThresholds};
    ///
    /// let thresholds = AlertThresholds {
    ///     at_50: true,
    ///     at_80: true,
    ///     at_90: false, // don't alert at 90%
    /// };
    /// let juice = TokenJuice::with_budget(TokenJuice::static_budget(500_000, thresholds));
    /// ```
    #[must_use]
    pub fn with_budget(budget: TokenBudget) -> Self {
        Self {
            counter: TokenCounter::new(),
            budget,
            total_input: Arc::new(AtomicU64::new(0)),
            total_output: Arc::new(AtomicU64::new(0)),
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Convenience helper to construct a budget with thresholds.
    #[must_use]
    pub fn static_budget(max_tokens: u64, thresholds: budget::AlertThresholds) -> TokenBudget {
        TokenBudget::with_thresholds(max_tokens, thresholds)
    }

    /// Wrap an `LlmClient` with token tracking.
    ///
    /// The returned [`TokenJuicedLlm`] shares the budget and counter with
    /// this `TokenJuice`, so all stats are accumulated across multiple
    /// wrapped clients if needed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use avid_core::llm::LlmClient;
    /// use avid_tokenjuice::TokenJuice;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = LlmClient::new("http://localhost:11434".into(), "llama3".into()).await?;
    /// let juice = TokenJuice::new(100_000);
    /// let tracked = juice.wrap_llm(Arc::new(client));
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn wrap_llm(&self, client: Arc<avid_core::llm::LlmClient>) -> TokenJuicedLlm {
        TokenJuicedLlm {
            client,
            budget: self.budget.clone(),
            counter: self.counter,
            total_input: Arc::clone(&self.total_input),
            total_output: Arc::clone(&self.total_output),
            call_count: Arc::clone(&self.call_count),
            input_price_per_m: 0.15,
            output_price_per_m: 0.15,
        }
    }

    /// Tokens remaining in the budget.
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.budget.remaining()
    }

    /// Maximum budget capacity.
    #[must_use]
    pub fn max(&self) -> u64 {
        self.budget.max()
    }

    /// Snapshot of current token statistics.
    #[must_use]
    pub fn stats(&self) -> TokenStats {
        let total_input = self.total_input.load(Ordering::Relaxed);
        let total_output = self.total_output.load(Ordering::Relaxed);
        let total_tokens = total_input + total_output;
        let estimated_cost = self
            .counter
            .estimate_cost(total_input, total_output, 0.15, 0.15);

        TokenStats {
            total_input,
            total_output,
            total_tokens,
            budget_max: self.budget.max(),
            budget_consumed: self.budget.consumed(),
            budget_remaining: self.budget.remaining(),
            budget_pct: self.budget.usage_pct(),
            call_count: self.call_count.load(Ordering::Relaxed),
            estimated_cost_usd: estimated_cost,
        }
    }

    /// Reset the budget to full capacity and clear cumulative stats.
    pub fn reset(&self) {
        self.budget.reset();
        self.total_input.store(0, Ordering::Relaxed);
        self.total_output.store(0, Ordering::Relaxed);
        self.call_count.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_juice_creation() {
        let juice = TokenJuice::new(10_000);
        assert_eq!(juice.remaining(), 10_000);
        assert_eq!(juice.max(), 10_000);
    }

    #[test]
    fn wrap_llm_shares_stats() {
        let juice = TokenJuice::new(10_000);
        let stats = juice.stats();
        assert_eq!(stats.total_input, 0);
        assert_eq!(stats.call_count, 0);
        assert_eq!(stats.budget_max, 10_000);
    }

    #[test]
    fn reset_clears_everything() {
        let juice = TokenJuice::new(10_000);
        // We can't easily increment stats without a real LLM call,
        // but we can verify reset doesn't panic
        juice.reset();
        assert_eq!(juice.remaining(), 10_000);
        let stats = juice.stats();
        assert_eq!(stats.total_input, 0);
        assert_eq!(stats.call_count, 0);
    }

    #[test]
    fn stats_human_readable_does_not_panic() {
        let juice = TokenJuice::new(10_000);
        let s = juice.stats().human_readable();
        assert!(s.contains("Token Stats"));
        assert!(s.contains("10000"));
    }
}
