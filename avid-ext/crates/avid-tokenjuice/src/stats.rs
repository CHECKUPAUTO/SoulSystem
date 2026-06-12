//! Statistics and reporting for token usage.
//!
//! `TokenStats` provides a snapshot of token consumption across a session,
//! suitable for logging, metrics export, or display in a terminal UI.

use serde::Serialize;

/// A snapshot of token usage statistics.
///
/// Returned by [`TokenJuice::stats`](crate::TokenJuice::stats) and
/// [`TokenJuicedLlm::stats`](crate::TokenJuicedLlm::stats).
///
/// # Examples
///
/// ```
/// use avid_tokenjuice::{TokenJuice, TokenStats};
///
/// let juice = TokenJuice::new(100_000);
/// let stats = juice.stats();
/// assert_eq!(stats.total_input, 0);
/// assert_eq!(stats.total_output, 0);
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct TokenStats {
    /// Total input (prompt) tokens consumed this session.
    pub total_input: u64,
    /// Total output (response) tokens consumed this session.
    pub total_output: u64,
    /// Total tokens (input + output) consumed this session.
    pub total_tokens: u64,
    /// Budget maximum.
    pub budget_max: u64,
    /// Budget consumed so far.
    pub budget_consumed: u64,
    /// Budget remaining.
    pub budget_remaining: u64,
    /// Percentage of budget consumed (0–100).
    pub budget_pct: f64,
    /// Number of LLM calls made.
    pub call_count: u64,
    /// Estimated total cost in USD.
    pub estimated_cost_usd: f64,
}

impl TokenStats {
    /// Format statistics as a human-readable string.
    ///
    /// Suitable for logging or display.
    #[must_use]
    pub fn human_readable(&self) -> String {
        format!(
            "📊 Token Stats | {} calls | input: {} | output: {} | total: {} | budget: {}/{} ({:.1}%) | est. cost: ${:.4}",
            self.call_count,
            self.total_input,
            self.total_output,
            self.total_tokens,
            self.budget_consumed,
            self.budget_max,
            self.budget_pct,
            self.estimated_cost_usd,
        )
    }
}
