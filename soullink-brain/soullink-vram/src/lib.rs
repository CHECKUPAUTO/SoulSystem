//! VramPool — Centralized VRAM budget tracking and pressure-based eviction.
//!
//! Tracks which models are loaded in VRAM, their sizes, and provides
//! eviction decisions based on memory pressure.

use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── Types ───────────────────────────────────────────────────────────────

/// Priority of a loaded model (higher = keep longer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModelPriority {
    /// Core inference model (e.g., Qwen3) — never evict unless critical.
    Critical = 100,
    /// Active inference model — evict only under pressure.
    High = 75,
    /// Embedding model — evict before inference models.
    Medium = 50,
    /// Background/preloaded model — evict first.
    Low = 25,
    /// Prefetch/predicted model — evict immediately on pressure.
    Ephemeral = 10,
}

/// State of a loaded model in VRAM.
#[derive(Debug, Clone)]
pub struct LoadedModel {
    pub name: String,
    pub size_mib: u64,
    pub priority: ModelPriority,
    pub loaded_at: Instant,
    pub last_used: Instant,
    pub use_count: u64,
    pub ref_count: u64,
}

/// VRAM pressure level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash, Serialize, Deserialize)]
pub enum PressureLevel {
    /// VRAM usage < 70% — no action needed.
    Normal,
    /// VRAM usage 70-85% — start evicting ephemeral models.
    Elevated,
    /// VRAM usage 85-95% — evict low-priority models.
    Critical,
    /// VRAM usage > 95% — evict everything non-essential.
    Emergency,
}

/// Action to take based on VRAM pressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionAction {
    /// No action needed.
    None,
    /// Evict specific models to free VRAM.
    Evict {
        models: Vec<String>,
        target_free_mib: u64,
    },
    /// Reject new model loads until VRAM frees up.
    RejectLoad,
}

/// Configuration for the VRAM pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramPoolConfig {
    /// Total VRAM in MiB.
    pub total_mib: u64,
    /// Headroom to reserve in MiB (not usable by models).
    pub headroom_mib: u64,
    /// Pressure thresholds (fraction of usable VRAM).
    pub elevated_threshold: f32,
    pub critical_threshold: f32,
    pub emergency_threshold: f32,
}

impl Default for VramPoolConfig {
    fn default() -> Self {
        Self {
            total_mib: 8192,
            headroom_mib: 1024,
            elevated_threshold: 0.70,
            critical_threshold: 0.85,
            emergency_threshold: 0.95,
        }
    }
}

// ── VramPool ────────────────────────────────────────────────────────────

/// Centralized VRAM budget tracker with pressure-based eviction.
pub struct VramPool {
    config: RwLock<VramPoolConfig>,
    loaded: DashMap<String, LoadedModel>,
    /// External VRAM used/miB reported by NVML (includes non-model usage).
    external_used_mib: AtomicU64,
    /// Callback invoked when models need to be evicted.
    eviction_handler: RwLock<Option<EvictionFn>>,
}

type EvictionFn = Arc<dyn Fn(&[String]) + Send + Sync>;

impl VramPool {
    /// Create a new VRAM pool.
    pub fn new(config: VramPoolConfig) -> Self {
        Self {
            config: RwLock::new(config),
            loaded: DashMap::new(),
            external_used_mib: AtomicU64::new(0),
            eviction_handler: RwLock::new(None),
        }
    }

    /// Set the callback invoked when models need to be evicted.
    pub fn set_eviction_handler(&self, handler: EvictionFn) {
        *self.eviction_handler.blocking_write() = Some(handler);
    }

    /// Update the VRAM used by external processes (e.g., Ollama, CUDA allocators).
    pub fn set_external_used(&self, mib: u64) {
        self.external_used_mib.store(mib, Ordering::Relaxed);
    }

    /// Get the total usable VRAM in MiB.
    pub async fn usable_mib(&self) -> u64 {
        let config = self.config.read().await;
        config.total_mib.saturating_sub(config.headroom_mib)
    }

    /// Get the total VRAM used by tracked models in MiB.
    pub fn models_used_mib(&self) -> u64 {
        self.loaded.iter().map(|e| e.value().size_mib).sum()
    }

    /// Get total VRAM used (models + external).
    pub fn total_used_mib(&self) -> u64 {
        self.models_used_mib() + self.external_used_mib.load(Ordering::Relaxed)
    }

    /// Get current VRAM pressure level.
    pub async fn pressure_level(&self) -> PressureLevel {
        let config = self.config.read().await;
        let usable = config.total_mib.saturating_sub(config.headroom_mib);
        if usable == 0 {
            return PressureLevel::Emergency;
        }

        let used = self.total_used_mib();
        let usage = used as f32 / usable as f32;

        if usage >= config.emergency_threshold {
            PressureLevel::Emergency
        } else if usage >= config.critical_threshold {
            PressureLevel::Critical
        } else if usage >= config.elevated_threshold {
            PressureLevel::Elevated
        } else {
            PressureLevel::Normal
        }
    }

    /// Register a model as loaded in VRAM.
    pub fn register_loaded(&self, name: String, size_mib: u64, priority: ModelPriority) {
        let now = Instant::now();
        let model = LoadedModel {
            name: name.clone(),
            size_mib,
            priority,
            loaded_at: now,
            last_used: now,
            use_count: 0,
            ref_count: 0,
        };
        self.loaded.insert(name, model);
    }

    /// Mark a model as recently used.
    pub fn touch(&self, name: &str) {
        if let Some(mut model) = self.loaded.get_mut(name) {
            model.last_used = Instant::now();
            model.use_count += 1;
        }
    }

    /// Increment/decrement reference count for a model.
    pub fn add_ref(&self, name: &str) {
        if let Some(mut model) = self.loaded.get_mut(name) {
            model.ref_count += 1;
        }
    }

    pub fn release_ref(&self, name: &str) {
        if let Some(mut model) = self.loaded.get_mut(name) {
            model.ref_count = model.ref_count.saturating_sub(1);
        }
    }

    /// Remove a model from tracking (after eviction).
    pub fn unregister(&self, name: &str) -> Option<LoadedModel> {
        self.loaded.remove(name).map(|(_, v)| v)
    }

    /// Check if a model can be loaded given current VRAM state.
    pub async fn can_load(&self, size_mib: u64) -> bool {
        let config = self.config.read().await;
        let usable = config.total_mib.saturating_sub(config.headroom_mib);
        let used = self.total_used_mib();
        let available = usable.saturating_sub(used);
        available >= size_mib
    }

    /// Determine which models to evict to free `target_free_mib` bytes.
    ///
    /// Returns models in eviction order (lowest priority first, then LRU).
    pub fn eviction_candidates(&self, target_free_mib: u64) -> Vec<String> {
        let mut candidates: Vec<LoadedModel> =
            self.loaded.iter().map(|e| e.value().clone()).collect();

        // Sort by priority (ascending), then by last_used (ascending = oldest first)
        candidates.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.last_used.cmp(&b.last_used))
        });

        let mut to_evict = Vec::new();
        let mut freed = 0u64;

        for model in &candidates {
            // Never evict models with active references
            if model.ref_count > 0 {
                continue;
            }
            // Never evict Critical models unless Emergency
            if model.priority == ModelPriority::Critical {
                continue;
            }

            to_evict.push(model.name.clone());
            freed += model.size_mib;

            if freed >= target_free_mib {
                break;
            }
        }

        to_evict
    }

    /// Evaluate current pressure and return an eviction action.
    pub async fn evaluate_pressure(&self) -> EvictionAction {
        let level = self.pressure_level().await;
        let config = self.config.read().await;
        let usable = config.total_mib.saturating_sub(config.headroom_mib);
        let used = self.total_used_mib();

        match level {
            PressureLevel::Normal => EvictionAction::None,

            PressureLevel::Elevated => {
                // Only evict Ephemeral models
                let ephemeral: Vec<String> = self
                    .loaded
                    .iter()
                    .filter(|e| e.value().priority == ModelPriority::Ephemeral)
                    .filter(|e| e.value().ref_count == 0)
                    .map(|e| e.key().clone())
                    .collect();

                if ephemeral.is_empty() {
                    EvictionAction::None
                } else {
                    let target = (usable as f32 * 0.10) as u64;
                    EvictionAction::Evict {
                        models: ephemeral,
                        target_free_mib: target,
                    }
                }
            }

            PressureLevel::Critical => {
                let target = used.saturating_sub((usable as f32 * 0.80) as u64);
                let candidates = self.eviction_candidates(target);
                if candidates.is_empty() {
                    EvictionAction::RejectLoad
                } else {
                    EvictionAction::Evict {
                        models: candidates,
                        target_free_mib: target,
                    }
                }
            }

            PressureLevel::Emergency => {
                let target = used.saturating_sub((usable as f32 * 0.60) as u64);
                let candidates = self.eviction_candidates(target);
                if candidates.is_empty() {
                    EvictionAction::RejectLoad
                } else {
                    EvictionAction::Evict {
                        models: candidates,
                        target_free_mib: target,
                    }
                }
            }
        }
    }

    /// Get a snapshot of all loaded models.
    pub fn snapshot(&self) -> Vec<LoadedModel> {
        self.loaded.iter().map(|e| e.value().clone()).collect()
    }

    /// Get per-model VRAM usage.
    pub fn per_model_usage(&self) -> HashMap<String, u64> {
        self.loaded
            .iter()
            .map(|e| (e.key().clone(), e.value().size_mib))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_levels() {
        let pool = VramPool::new(VramPoolConfig {
            total_mib: 10000,
            headroom_mib: 1000,
            ..Default::default()
        });

        // Empty pool = Normal
        assert_eq!(block_on(pool.pressure_level()), PressureLevel::Normal);

        // Add 6000 MiB of models (6000/9000 = 66%)
        pool.register_loaded("model1".into(), 6000, ModelPriority::High);
        assert_eq!(block_on(pool.pressure_level()), PressureLevel::Normal);

        // Add 1000 more (7000/9000 = 77%)
        pool.register_loaded("model2".into(), 1000, ModelPriority::Medium);
        assert_eq!(block_on(pool.pressure_level()), PressureLevel::Elevated);

        // Add 1500 more (8500/9000 = 94%)
        pool.register_loaded("model3".into(), 1500, ModelPriority::Low);
        assert_eq!(block_on(pool.pressure_level()), PressureLevel::Critical);
    }

    #[test]
    fn test_eviction_candidates() {
        let pool = VramPool::new(VramPoolConfig {
            total_mib: 10000,
            headroom_mib: 1000,
            ..Default::default()
        });

        pool.register_loaded("critical_model".into(), 3000, ModelPriority::Critical);
        pool.register_loaded("ephemeral1".into(), 1000, ModelPriority::Ephemeral);
        pool.register_loaded("low_model".into(), 2000, ModelPriority::Low);
        pool.register_loaded("high_model".into(), 2000, ModelPriority::High);

        // Request 2500 MiB — should evict ephemeral + low (skipping critical)
        let candidates = pool.eviction_candidates(2500);
        assert!(candidates.contains(&"ephemeral1".to_string()));
        assert!(candidates.contains(&"low_model".to_string()));
        assert!(!candidates.contains(&"critical_model".to_string()));
    }

    #[test]
    fn test_ref_count_blocks_eviction() {
        let pool = VramPool::new(VramPoolConfig::default());

        pool.register_loaded("model1".into(), 1000, ModelPriority::Low);
        pool.add_ref("model1");

        let candidates = pool.eviction_candidates(2000);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_can_load() {
        let pool = VramPool::new(VramPoolConfig {
            total_mib: 10000,
            headroom_mib: 1000,
            ..Default::default()
        });

        assert!(block_on(pool.can_load(5000)));
        pool.register_loaded("big_model".into(), 5000, ModelPriority::High);
        assert!(block_on(pool.can_load(4000)));
        assert!(!block_on(pool.can_load(4500)));
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(f)
    }
}
