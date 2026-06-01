pub use scirust_core::embed::EmbeddingEngine;

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

/// Async-friendly embedder wrapping EmbeddingEngine.
///
/// Uses Arc<Mutex<>> internally for thread-safe async access.
#[derive(Clone)]
pub struct Embedder {
    engine: Arc<Mutex<EmbeddingEngine>>,
    dim: usize,
}

impl Embedder {
    /// Create a new Embedder with default vocab.
    pub fn new(vocab_texts: &[&str]) -> Self {
        let engine = EmbeddingEngine::new(vocab_texts);
        let dim = engine.dim();
        Embedder {
            engine: Arc::new(Mutex::new(engine)),
            dim,
        }
    }

    /// Embedding dimension.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Embed text asynchronously.
    ///
    /// Returns L2-normalized f64 vector for use with HNN/Cortex.
    pub async fn embed(&self, text: &str) -> Result<Vec<f64>> {
        let text = text.to_string();
        let engine = Arc::clone(&self.engine);
        let vec = spawn_blocking(move || {
            let mut guard = engine.lock().unwrap();
            let f32_vec = guard.embed(&text);
            f32_vec.into_iter().map(|x| x as f64).collect::<Vec<_>>()
        })
        .await?;
        Ok(vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedder_creation() {
        let e = Embedder::new(&["hello", "world"]);
        assert!(e.dim() > 0, "Embedder should have positive dimension");
    }
}
