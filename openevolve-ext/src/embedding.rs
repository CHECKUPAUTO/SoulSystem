//! Semantic embeddings for transfer learning, novelty search, and Map-Elites.
//!
//! Strategy:
//!   - If `endpoint` is set, call out via HTTP to a TRIBE-style embeddings server.
//!     The server is expected to accept `{"texts": [...]}` and return
//!     `{"embeddings": [[...], ...]}`. Compatible with Tarek's TRIBE on :7440.
//!   - Otherwise, fall back to a dependency-free feature-hashed bag-of-tokens
//!     embedding (256 dims, L2-normalised). Surprisingly effective at code
//!     similarity within a single language.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const LOCAL_DIM: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Empty string → local fallback. Otherwise full URL to the embeddings
    /// endpoint (e.g. `http://localhost:7440/embed`).
    pub endpoint: String,
    pub timeout_secs: u64,
    pub max_batch: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            timeout_secs: 30,
            max_batch: 32,
        }
    }
}

pub struct EmbeddingClient {
    cfg: EmbeddingConfig,
    http: Client,
}

impl EmbeddingClient {
    pub fn new(cfg: EmbeddingConfig) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .expect("build reqwest client");
        Self { cfg, http }
    }

    /// Returns true when configured for remote (TRIBE) embeddings.
    pub fn is_remote(&self) -> bool {
        !self.cfg.endpoint.is_empty()
    }

    /// Embed a single text. Falls back to local on remote error.
    pub async fn embed(&self, text: &str) -> Vec<f32> {
        if self.is_remote() {
            match self.embed_remote(&[text]).await {
                Ok(mut v) if !v.is_empty() => return v.remove(0),
                Ok(_) => tracing::warn!("remote embedder returned empty list, using local"),
                Err(e) => tracing::warn!(error=%e, "remote embedder failed, using local"),
            }
        }
        embed_local(text)
    }

    /// Batch embed; honours `max_batch`.
    pub async fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        if self.is_remote() {
            let mut out = Vec::with_capacity(texts.len());
            for chunk in texts.chunks(self.cfg.max_batch.max(1)) {
                match self.embed_remote(chunk).await {
                    Ok(v) if v.len() == chunk.len() => out.extend(v),
                    _ => {
                        out.extend(chunk.iter().map(|t| embed_local(t)));
                    }
                }
            }
            return out;
        }
        texts.iter().map(|t| embed_local(t)).collect()
    }

    async fn embed_remote(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        #[derive(Serialize)]
        struct Req<'a> {
            texts: &'a [&'a str],
        }
        #[derive(Deserialize)]
        struct Resp {
            embeddings: Vec<Vec<f32>>,
        }
        let req = Req { texts };
        let resp = self
            .http
            .post(&self.cfg.endpoint)
            .json(&req)
            .send()
            .await
            .context("embedder POST failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "embedder HTTP {status}: {}",
                body.chars().take(200).collect::<String>()
            );
        }
        let r: Resp = resp.json().await.context("parse embedder response")?;
        Ok(r.embeddings)
    }
}

/// Local fallback: feature-hashed bag-of-tokens, L2-normalised.
pub fn embed_local(code: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; LOCAL_DIM];
    for tok in tokenize(code) {
        let bucket = hash_to_bucket(&tok, LOCAL_DIM);
        v[bucket] += 1.0;
    }
    l2_normalise(&mut v);
    v
}

/// Cosine similarity. Returns 0.0 if dims mismatch or either norm is 0.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn tokenize(code: &str) -> impl Iterator<Item = String> + '_ {
    code.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .map(|s| s.to_lowercase())
}

fn hash_to_bucket(token: &str, dim: usize) -> usize {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    let d = h.finalize();
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
    (n as usize) % dim
}

fn l2_normalise(v: &mut [f32]) {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_has_correct_dim() {
        let v = embed_local("fn main() {}");
        assert_eq!(v.len(), LOCAL_DIM);
    }

    #[test]
    fn similar_code_high_cosine() {
        let a = embed_local("fn add(a: i32, b: i32) -> i32 { a + b }");
        let b = embed_local("fn sum(x: i32, y: i32) -> i32 { x + y }");
        let c = embed_local("import os\nfor i in range(10): print(i)");
        let ab = cosine(&a, &b);
        let ac = cosine(&a, &c);
        assert!(ab > ac, "ab={ab} ac={ac}");
        assert!(ab > 0.3);
    }

    #[test]
    fn empty_or_mismatched_returns_zero() {
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0);
    }

    #[test]
    fn is_remote_reports_correctly() {
        let local = EmbeddingClient::new(EmbeddingConfig::default());
        assert!(!local.is_remote());
        let remote = EmbeddingClient::new(EmbeddingConfig {
            endpoint: "http://localhost:7440/embed".into(),
            ..Default::default()
        });
        assert!(remote.is_remote());
    }
}
