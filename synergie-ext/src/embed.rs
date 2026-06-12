//! Client d'embeddings.
//!
//! Deux modes :
//! - `local` : `scirust_core::embed::EmbeddingEngine` (par défaut, dim 128)
//! - `http`  : POST `{endpoint}` avec `{"text":"..."}` → `{"embedding":[...]}` (TRIBE 768d)
//!
//! Cache transparent sur le tree sled `embeddings_cache`.

use crate::config::EmbedCfg;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, warn};

pub struct EmbedClient {
    cfg: EmbedCfg,
    cache: Arc<sled::Tree>,
    http: reqwest::Client,
    local: Option<Mutex<scirust_core::embed::EmbeddingEngine>>,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    embedding: Vec<f32>,
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
}

impl EmbedClient {
    pub fn new(cfg: EmbedCfg, cache: Arc<sled::Tree>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_seconds))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let local = if cfg.mode == "local" {
            Some(Mutex::new(scirust_core::embed::EmbeddingEngine::new(&[])))
        } else {
            None
        };
        Ok(Self { cfg, cache, http, local })
    }

    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let key = cache_key(&self.cfg.endpoint, self.cfg.dim, text);
        if let Some(bytes) = self.cache.get(&key)? {
            if let Ok(v) = bincode::deserialize::<Vec<f32>>(&bytes) {
                return Ok(v);
            }
        }
        let v = match self.cfg.mode.as_str() {
            "local" => self.embed_local(text)?,
            _ => self.embed_http(text).await?,
        };
        if let Ok(b) = bincode::serialize(&v) {
            self.cache.insert(&key, b).ok();
        }
        Ok(v)
    }

    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        for t in texts {
            match self.embed_one(t).await {
                Ok(v) => out.push(v),
                Err(e) => {
                    warn!(target: "embed", error = %e, "skip text");
                    out.push(vec![0.0; self.cfg.dim]);
                }
            }
        }
        Ok(out)
    }

    fn embed_local(&self, text: &str) -> Result<Vec<f32>> {
        let engine = self
            .local
            .as_ref()
            .ok_or_else(|| anyhow!("local engine non initialisé"))?;
        let mut engine = engine.lock().unwrap();
        let v = engine.embed(text);
        Ok(v)
    }

    async fn embed_http(&self, text: &str) -> Result<Vec<f32>> {
        let resp = self
            .http
            .post(&self.cfg.endpoint)
            .json(&EmbedRequest { text })
            .send()
            .await
            .context("POST embed")?;
        if !resp.status().is_success() {
            return Err(anyhow!("embed http: {}", resp.status()));
        }
        let body: EmbedResponse = resp.json().await.context("decode embed")?;
        if !body.embedding.is_empty() {
            Ok(body.embedding)
        } else if !body.embeddings.is_empty() {
            Ok(body.embeddings.into_iter().next().unwrap_or_default())
        } else {
            Err(anyhow!("réponse embed vide"))
        }
    }

    pub async fn healthcheck(&self) -> bool {
        if self.cfg.mode == "local" {
            return true;
        }
        self.http
            .get(self.cfg.endpoint.replace("/embed", "/health"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 { return 0.0; }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 0.0 || nb <= 0.0 { return 0.0; }
    dot / (na.sqrt() * nb.sqrt())
}

pub fn chunk_text(text: &str, chunk_lines: usize) -> Vec<String> {
    if chunk_lines == 0 { return vec![text.to_string()]; }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= chunk_lines {
        return vec![text.to_string()];
    }
    lines.chunks(chunk_lines).map(|c| c.join("\n")).collect()
}

fn cache_key(endpoint: &str, dim: usize, text: &str) -> Vec<u8> {
    let seed = format!("{}|{}|{}", endpoint, dim, text);
    let h = blake3::hash(seed.as_bytes());
    h.as_bytes().to_vec()
}

#[allow(dead_code)]
fn _ensure_debug(_e: &EmbedCfg) { debug!("embed cfg"); }
