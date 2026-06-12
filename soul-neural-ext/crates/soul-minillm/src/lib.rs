use scirust_core::nn::transformer::mini_llm::{MiniLLM as CoreMiniLLM, CharTokenizer};

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

/// MiniLLM wrapper with async-compatible methods for cortex integration.
/// Uses Arc<Mutex<>> internally, so it is Clone for use in spawn_blocking.
#[derive(Clone)]
pub struct MiniLLM {
    inner: Arc<Mutex<CoreMiniLLM>>,
    latent_dim: usize,
    vocab_size: usize,
}

impl MiniLLM {
    /// Wrap an existing CoreMiniLLM.
    pub fn from_core(llm: CoreMiniLLM) -> Self {
        let latent_dim = llm.config.d_model;
        let vocab_size = llm.config.vocab_size;
        MiniLLM {
            inner: Arc::new(Mutex::new(llm)),
            latent_dim,
            vocab_size,
        }
    }

    /// Create a new MiniLLM with default config.
    pub fn new(vocab_texts: &[&str]) -> Self {
        let tokenizer = CharTokenizer::new(vocab_texts);
        let config = scirust_core::nn::transformer::mini_llm::MiniLLMConfig {
            vocab_size: tokenizer.vocab_size,
            ..scirust_core::nn::transformer::mini_llm::MiniLLMConfig::default()
        };
        let llm = CoreMiniLLM::new(config, tokenizer);
        Self::from_core(llm)
    }

    /// The latent dimension (d_model).
    pub fn latent_dim(&self) -> Result<usize> {
        Ok(self.latent_dim)
    }

    /// Forward pass returning logits for the last token as f64.
    pub async fn logits(&self, input: &str) -> Result<Vec<f64>> {
        let input = input.to_string();
        let inner = Arc::clone(&self.inner);
        let vocab_size = self.vocab_size;
        spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            let ids = guard.tokenizer.encode(&input);
            let logits_tensor = guard.forward(&ids);
            let last_row = logits_tensor.nrows().saturating_sub(1);
            let mut result = vec![-1e9f64; vocab_size];
            for j in 0..vocab_size {
                let v = logits_tensor.get((last_row, j)).copied().unwrap_or(-1e9f32);
                result[j] = if v.is_finite() { v as f64 } else { -1e9 };
            }
            Ok(result)
        })
        .await?
    }

    /// Greedy sample a token index from logits.
    pub fn sample_token(&self, logits: &[f64]) -> Result<usize> {
        if logits.is_empty() {
            anyhow::bail!("Empty logits");
        }
        let best = logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        Ok(best)
    }

    /// Decode a single token index to a char string.
    pub fn decode_token(&self, token: usize) -> Result<String> {
        let guard = self.inner.lock().unwrap();
        let s = guard.tokenizer.decode(&[token]);
        if s.is_empty() || s == "\0" {
            Ok(String::new())
        } else {
            Ok(s)
        }
    }

    /// Generate text from a prompt (direct, non-async).
    pub fn generate(&mut self, prompt: &str, max_tokens: usize) -> String {
        self.inner.lock().unwrap().generate(prompt, max_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_llm() {
        let llm = MiniLLM::new(&["hello test"]);
        assert!(llm.latent_dim().unwrap() > 0);
    }
}

pub mod dpo;
