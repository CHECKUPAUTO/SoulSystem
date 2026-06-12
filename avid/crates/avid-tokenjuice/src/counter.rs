//! Token estimation for Ollama models.
//!
//! Provides a `TokenCounter` that estimates token counts from text using
//! a character-based heuristic (≈4 chars per token), which is a reasonable
//! approximation for most LLM tokenizers when tiktoken is unavailable.

/// Estimates the number of tokens in a string.
///
/// Uses the common approximation of **1 token ≈ 4 characters** for English text.
/// This is an O(1) estimation — reasonably close to tiktoken counts for many models
/// (GPT-4, Claude, Llama) and serves as a practical upper-bound heuristic for
/// budget tracking.
///
/// For more precise counting, consider integrating `tiktoken-rs` with the
/// appropriate BPE encoding for the target model.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenCounter;

impl TokenCounter {
    /// Create a new `TokenCounter`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Estimate the number of tokens in the given text.
    ///
    /// Uses the chars/4 heuristic with a minimum of 1 token for non-empty input.
    ///
    /// # Examples
    ///
    /// ```
    /// use avid_tokenjuice::TokenCounter;
    ///
    /// let counter = TokenCounter::new();
    /// assert_eq!(counter.count("hello world"), 3); // 11 / 4 = 2 → ceil 3
    /// assert_eq!(counter.count(""), 0);
    /// ```
    #[must_use]
    pub fn count(&self, text: &str) -> u64 {
        let len = text.chars().count() as u64;
        if len == 0 {
            return 0;
        }
        // Ceiling division
        len.div_ceil(4)
    }

    /// Estimate tokens for a prompt (system + user messages) and an assistant response.
    ///
    /// Adds a small overhead for message formatting delimiters (≈4 tokens per message).
    ///
    /// # Examples
    ///
    /// ```
    /// use avid_tokenjuice::TokenCounter;
    ///
    /// let counter = TokenCounter::new();
    /// let (input, output) = counter.count_exchange("You are helpful.", "Hello!", "Hi there!");
    /// assert!(input > 0);
    /// assert!(output > 0);
    /// ```
    #[must_use]
    pub fn count_exchange(&self, system: &str, user: &str, response: &str) -> (u64, u64) {
        let input = self.count(system) + self.count(user) + 8; // ~4 tokens overhead per message
        let output = self.count(response);
        (input, output)
    }

    /// Estimate cost in USD based on a per-million-token price.
    ///
    /// # Examples
    ///
    /// ```
    /// use avid_tokenjuice::TokenCounter;
    ///
    /// let counter = TokenCounter::new();
    /// let cost = counter.estimate_cost(500_000, 500_000, 0.50, 1.50);
    /// assert!((cost - 1.00).abs() < 0.01);
    /// ```
    #[must_use]
    pub fn estimate_cost(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        input_price_per_m: f64,
        output_price_per_m: f64,
    ) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_price_per_m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_price_per_m;
        ((input_cost + output_cost) * 1000.0).round() / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_returns_zero() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn short_text_estimates_one_token() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count("hi"), 1);
    }

    #[test]
    fn four_chars_estimates_one_token() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count("help"), 1);
    }

    #[test]
    fn five_chars_estimates_two_tokens() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count("hello"), 2);
    }

    #[test]
    fn long_text_estimates_reasonably() {
        let counter = TokenCounter::new();
        let text = "a".repeat(400);
        assert_eq!(counter.count(&text), 100);
    }

    #[test]
    fn count_exchange_adds_overhead() {
        let counter = TokenCounter::new();
        let (input, output) = counter.count_exchange("sys", "usr", "resp");
        // sys: 3/4=1, usr: 3/4=1, overhead=8 → 10
        assert_eq!(input, 10);
        // resp: 4/4=1
        assert_eq!(output, 1);
    }

    #[test]
    fn estimate_cost_basic() {
        let counter = TokenCounter::new();
        let cost = counter.estimate_cost(500_000, 500_000, 1.0, 2.0);
        assert!((cost - 1.50).abs() < 0.01);
    }

    #[test]
    fn multi_byte_chars_count_correctly() {
        let counter = TokenCounter::new();
        // 4 emojis = 4 chars
        assert_eq!(counter.count("🚀🦞✨🔥"), 1);
    }
}
