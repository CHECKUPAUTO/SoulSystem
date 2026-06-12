//! LLM response cache.
//!
//! Cheap, high-leverage optimisation: many evolution rounds end up sending
//! near-identical prompts (same parent in stagnation, same feedback re-prompt,
//! etc). A SHA-256-keyed in-memory cache with TTL eliminates redundant
//! provider calls and the cost they incur.
//!
//! The cache is **content-addressable**: the key is a hash over
//! `(provider, model, temperature_bucket, prompt)` so different temperatures
//! don't share entries. Temperature is bucketed in 0.1 steps because two
//! requests differing only by 0.01 effectively want the same answer.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Master switch. When false, every `get` returns `None`.
    pub enabled: bool,
    /// Maximum entries before LRU-style eviction. 0 = unbounded.
    pub max_entries: usize,
    /// Time-to-live in seconds. 0 = never expire.
    pub ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 10_000,
            ttl_secs: 60 * 60 * 6, // 6 hours
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stores: u64,
    pub evicted: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

struct Entry {
    response: String,
    stored_at: Instant,
}

pub struct LlmCache {
    cfg: CacheConfig,
    inner: Mutex<HashMap<String, Entry>>,
    stats: Mutex<CacheStats>,
}

impl LlmCache {
    pub fn new(cfg: CacheConfig) -> Self {
        Self {
            cfg,
            inner: Mutex::new(HashMap::new()),
            stats: Mutex::new(CacheStats::default()),
        }
    }

    pub fn shared(cfg: CacheConfig) -> Arc<Self> {
        Arc::new(Self::new(cfg))
    }

    pub fn is_enabled(&self) -> bool {
        self.cfg.enabled
    }

    fn key(&self, provider: &str, model: &str, temperature: f32, prompt: &str) -> String {
        let bucket = (temperature * 10.0).round() as i32;
        let mut h = Sha256::new();
        h.update(provider.as_bytes());
        h.update(b"|");
        h.update(model.as_bytes());
        h.update(b"|");
        h.update(bucket.to_le_bytes());
        h.update(b"|");
        h.update(prompt.as_bytes());
        hex::encode(h.finalize())
    }

    /// Try to retrieve a cached response. Returns `None` on miss or expired entry.
    pub fn get(
        &self,
        provider: &str,
        model: &str,
        temperature: f32,
        prompt: &str,
    ) -> Option<String> {
        if !self.cfg.enabled {
            return None;
        }
        let k = self.key(provider, model, temperature, prompt);
        let mut g = self.inner.lock();
        if let Some(e) = g.get(&k) {
            if self.cfg.ttl_secs == 0
                || e.stored_at.elapsed() < Duration::from_secs(self.cfg.ttl_secs)
            {
                let response = e.response.clone();
                drop(g);
                self.stats.lock().hits += 1;
                return Some(response);
            }
            // expired
            g.remove(&k);
        }
        drop(g);
        self.stats.lock().misses += 1;
        None
    }

    pub fn put(
        &self,
        provider: &str,
        model: &str,
        temperature: f32,
        prompt: &str,
        response: String,
    ) {
        if !self.cfg.enabled || response.is_empty() {
            return;
        }
        let k = self.key(provider, model, temperature, prompt);
        let mut g = self.inner.lock();
        // LRU-ish eviction: when full, drop the oldest entry. We don't need a
        // perfect LRU here — approximate eviction is plenty for an LLM cache.
        if self.cfg.max_entries > 0 && g.len() >= self.cfg.max_entries {
            if let Some(oldest_key) = g
                .iter()
                .min_by_key(|(_, e)| e.stored_at)
                .map(|(k, _)| k.clone())
            {
                g.remove(&oldest_key);
                drop(g);
                self.stats.lock().evicted += 1;
                g = self.inner.lock();
            }
        }
        g.insert(
            k,
            Entry {
                response,
                stored_at: Instant::now(),
            },
        );
        drop(g);
        self.stats.lock().stores += 1;
    }

    pub fn stats(&self) -> CacheStats {
        self.stats.lock().clone()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
    pub fn clear(&self) {
        self.inner.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_then_hit() {
        let c = LlmCache::new(CacheConfig::default());
        assert!(c.get("ollama", "qwen", 0.7, "hello").is_none());
        c.put("ollama", "qwen", 0.7, "hello", "world".into());
        assert_eq!(
            c.get("ollama", "qwen", 0.7, "hello").as_deref(),
            Some("world")
        );
        let s = c.stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.stores, 1);
    }

    #[test]
    fn temperature_bucketing() {
        let c = LlmCache::new(CacheConfig::default());
        c.put("p", "m", 0.71, "x", "A".into());
        // 0.74 rounds to the same 0.1 bucket (=7 after *10/round)
        assert_eq!(c.get("p", "m", 0.74, "x").as_deref(), Some("A"));
        // 0.80 → different bucket
        assert!(c.get("p", "m", 0.80, "x").is_none());
    }

    #[test]
    fn disabled_cache_is_no_op() {
        let c = LlmCache::new(CacheConfig {
            enabled: false,
            ..Default::default()
        });
        c.put("p", "m", 0.5, "x", "y".into());
        assert!(c.get("p", "m", 0.5, "x").is_none());
    }

    #[test]
    fn eviction_keeps_size_bounded() {
        let c = LlmCache::new(CacheConfig {
            enabled: true,
            max_entries: 3,
            ttl_secs: 0,
        });
        for i in 0..10 {
            c.put("p", "m", 0.5, &format!("prompt{i}"), format!("resp{i}"));
        }
        assert!(c.len() <= 3);
        assert!(c.stats().evicted >= 7);
    }

    #[test]
    fn hit_rate_calculation() {
        let s = CacheStats {
            hits: 3,
            misses: 1,
            stores: 0,
            evicted: 0,
        };
        assert!((s.hit_rate() - 0.75).abs() < 1e-9);
    }
}
