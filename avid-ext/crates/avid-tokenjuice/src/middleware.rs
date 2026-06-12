//! LLM client middleware that wraps `avid_core::LlmClient` with token tracking.
//!
//! `TokenJuicedLlm` intercepts every chat call, counts input/output tokens,
//! updates the budget, logs statistics, and optionally blocks calls when
//! the budget would be exceeded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::budget::TokenBudget;
use crate::counter::TokenCounter;
use crate::stats::TokenStats;

/// Errors that can occur from the token-juiced LLM wrapper.
#[derive(Debug, thiserror::Error)]
pub enum JuicedLlmError {
    /// An error from the underlying LLM client.
    #[error("LLM client error: {0}")]
    Llm(#[from] avid_core::llm::LlmError),

    /// The token budget would be exceeded by this call.
    #[error("token budget blocked: {0}")]
    BudgetBlocked(#[from] crate::budget::BudgetError),
}

/// A token-tracked wrapper around [`avid_core::LlmClient`].
///
/// Every `chat()` call is intercepted: input tokens are counted before
/// dispatch, the budget is checked, the LLM is called, output tokens are
/// counted, and the budget is updated. Statistics are accumulated across
/// the lifetime of this wrapper.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use std::time::Duration;
/// use avid_core::llm::LlmClient;
/// use avid_tokenjuice::{TokenJuice, TokenJuicedLlm};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let client = LlmClient::new("http://localhost:11434".into(), "llama3".into()).await?;
/// let juice = TokenJuice::new(50_000);
/// let tracked = juice.wrap_llm(Arc::new(client));
///
/// let response = tracked.chat("Be helpful.", "Hello!", false, Duration::from_secs(30)).await?;
/// println!("{}", response);
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
pub struct TokenJuicedLlm {
    pub(crate) client: Arc<avid_core::llm::LlmClient>,
    pub(crate) budget: TokenBudget,
    pub(crate) counter: TokenCounter,
    pub(crate) total_input: Arc<AtomicU64>,
    pub(crate) total_output: Arc<AtomicU64>,
    pub(crate) call_count: Arc<AtomicU64>,
    /// Price per million input tokens in USD (default: 0.15 — typical Ollama local cost)
    pub(crate) input_price_per_m: f64,
    /// Price per million output tokens in USD (default: 0.15)
    pub(crate) output_price_per_m: f64,
}

impl TokenJuicedLlm {
    /// Set custom pricing for cost estimation.
    #[must_use]
    pub const fn with_pricing(mut self, input_price_per_m: f64, output_price_per_m: f64) -> Self {
        self.input_price_per_m = input_price_per_m;
        self.output_price_per_m = output_price_per_m;
        self
    }

    /// Send a chat completion request with token tracking.
    ///
    /// Before calling the underlying LLM, the input tokens are estimated
    /// and checked against the budget. After the response arrives, output
    /// tokens are counted and the budget is updated.
    ///
    /// # Errors
    ///
    /// Returns `JuicedLlmError::BudgetBlocked` if the estimated input tokens
    /// would exceed the remaining budget. Returns `JuicedLlmError::Llm` for
    /// underlying client errors.
    pub async fn chat(
        &self,
        system: &str,
        user: &str,
        json_mode: bool,
        timeout: Duration,
    ) -> Result<String, JuicedLlmError> {
        // Estimate input tokens
        let input_tokens = self.counter.count(system) + self.counter.count(user) + 8;

        debug!(
            input_tokens = input_tokens,
            remaining = self.budget.remaining(),
            "checking token budget before LLM call"
        );

        // Check budget — this is a dry-run check; actual consumption happens after the response
        if input_tokens > self.budget.remaining() {
            warn!(
                input_tokens = input_tokens,
                remaining = self.budget.remaining(),
                "blocking LLM call — estimated input exceeds remaining budget"
            );
            return Err(JuicedLlmError::BudgetBlocked(
                crate::budget::BudgetError::Exceeded {
                    attempted: input_tokens,
                    remaining: self.budget.remaining(),
                    max: self.budget.max(),
                },
            ));
        }

        // Call the underlying LLM
        self.call_count.fetch_add(1, Ordering::Relaxed);

        let response = self.client.chat(system, user, json_mode, timeout).await?;

        // Count output tokens
        let output_tokens = self.counter.count(&response);

        // Total to consume: input + output
        let total = input_tokens + output_tokens;

        // Attempt to consume from budget (may fail if output was unexpectedly large)
        self.budget
            .consume(total)
            .map_err(JuicedLlmError::BudgetBlocked)?;

        // Update cumulative stats
        self.total_input.fetch_add(input_tokens, Ordering::Relaxed);
        self.total_output
            .fetch_add(output_tokens, Ordering::Relaxed);

        let estimated_cost = self.counter.estimate_cost(
            input_tokens,
            output_tokens,
            self.input_price_per_m,
            self.output_price_per_m,
        );

        info!(
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            total = total,
            remaining = self.budget.remaining(),
            estimated_cost_usd = estimated_cost,
            "LLM call completed with token tracking"
        );

        Ok(response)
    }

    /// Get a snapshot of current token statistics.
    #[must_use]
    pub fn stats(&self) -> TokenStats {
        let total_input = self.total_input.load(Ordering::Relaxed);
        let total_output = self.total_output.load(Ordering::Relaxed);
        let total_tokens = total_input + total_output;
        let estimated_cost = self.counter.estimate_cost(
            total_input,
            total_output,
            self.input_price_per_m,
            self.output_price_per_m,
        );

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

    /// Access the underlying LLM client (for operations that don't need tracking).
    #[must_use]
    pub const fn inner(&self) -> &Arc<avid_core::llm::LlmClient> {
        &self.client
    }

    /// Tokens remaining in the budget.
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.budget.remaining()
    }

    /// Reset the budget to full capacity.
    pub fn reset_budget(&self) {
        self.budget.reset();
    }
}

#[async_trait::async_trait]
impl avid_core::llm::ChatBackend for TokenJuicedLlm {
    async fn chat(
        &self,
        system: &str,
        user: &str,
        json_mode: bool,
        timeout: Duration,
    ) -> Result<String, avid_core::llm::LlmError> {
        Self::chat(self, system, user, json_mode, timeout)
            .await
            .map_err(|e| match e {
                JuicedLlmError::Llm(inner) => inner,
                JuicedLlmError::BudgetBlocked(budget_err) => {
                    avid_core::llm::LlmError::BudgetExceeded(budget_err.to_string())
                }
            })
    }

    fn model(&self) -> &str {
        self.client.model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_zero_on_fresh_wrapper() {
        let juice = crate::TokenJuice::new(10_000);
        let stats = juice.stats();
        assert_eq!(stats.total_input, 0);
        assert_eq!(stats.call_count, 0);
    }

    #[test]
    fn input_estimation_blocks_over_budget() {
        let budget = TokenBudget::new(10);
        let counter = TokenCounter::new();

        // Input would be ~(3 + 3 + 8) = 14 tokens for "system" and "user"
        // Budget has only 10 → should be blocked
        let remaining = budget.remaining();
        let estimated = counter.count("system") + counter.count("user") + 8;
        assert!(estimated > remaining);
        assert!(budget.consume(estimated).is_err());
    }

    #[test]
    fn small_call_passes_budget() {
        let budget = TokenBudget::new(100);
        let counter = TokenCounter::new();
        let estimated = counter.count("hello") + counter.count("hi") + 8;
        // 2 + 1 + 8 = 11
        assert!(budget.consume(estimated).is_ok());
        assert_eq!(budget.remaining(), 89);
    }
}
