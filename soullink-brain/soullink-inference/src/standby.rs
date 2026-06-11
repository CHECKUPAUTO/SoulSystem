//! Warm standby — preload, keep models hot, and swap quantization safely.
//!
//! Two swap strategies based on VRAM pressure:
//! - **Atomic Swap** (VRAM sufficient): Load new, mark old pending, evict old.
//!   Both coexist briefly. Zero interruption.
//! - **Drain & Swap** (VRAM critical): Mark old pending, wait for active
//!   inferences to finish, evict old, THEN load new. Prevents OOM on 8GB RTX 4060.

use crate::types::{ExecutionTarget, HardwareSnapshot, Priority, Quantization};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A loaded model's state.
#[derive(Debug, Clone)]
pub struct ModelSlot {
    pub model: String,
    pub quant: Quantization,
    pub target: ExecutionTarget,
    pub loaded_at: Instant,
    pub last_used: Instant,
    pub times_used: u64,
    pub ref_count: u64,
    pub pending_evict: bool,
}

impl ModelSlot {
    pub fn new(model: &str, quant: Quantization, target: ExecutionTarget) -> Self {
        Self {
            model: model.to_string(),
            quant,
            target,
            loaded_at: Instant::now(),
            last_used: Instant::now(),
            times_used: 0,
            ref_count: 0,
            pending_evict: false,
        }
    }
    pub fn can_evict(&self) -> bool {
        self.ref_count == 0
    }
}

/// Warm standby manager with dual-strategy swap support.
pub struct WarmStandby {
    slots: Arc<RwLock<HashMap<String, ModelSlot>>>,
    model_versions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    model_sizes: Arc<RwLock<HashMap<String, f64>>>,
    idle_timeout: Duration,
}

impl WarmStandby {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            slots: Arc::new(RwLock::new(HashMap::new())),
            model_versions: Arc::new(RwLock::new(HashMap::new())),
            model_sizes: Arc::new(RwLock::new(HashMap::new())),
            idle_timeout,
        }
    }

    fn slot_key(model: &str, quant: Quantization) -> String {
        format!("{}:{:?}", model, quant)
    }

    /// Register a model size for VRAM estimation.
    pub async fn register_model(&self, name: &str, params_b: f64) {
        self.model_sizes
            .write()
            .await
            .insert(name.to_string(), params_b);
    }

    /// Register a model as loaded.
    pub async fn mark_loaded(&self, model: &str, quant: Quantization, target: ExecutionTarget) {
        let key = Self::slot_key(model, quant);
        let slot = ModelSlot::new(model, quant, target);
        let mut slots = self.slots.write().await;
        slots.insert(key.clone(), slot);
        let mut versions = self.model_versions.write().await;
        let vlist = versions.entry(model.to_string()).or_default();
        if !vlist.contains(&key) {
            vlist.push(key);
        }
        info!(model, quant = ?quant, "Model marked as loaded");
    }

    /// Acquire a reference. Rejects if model is pending eviction.
    pub async fn acquire(&self, model: &str, quant: Quantization) -> Option<ModelSlot> {
        let key = Self::slot_key(model, quant);
        let mut slots = self.slots.write().await;
        if let Some(slot) = slots.get_mut(&key) {
            if slot.pending_evict {
                return None;
            }
            slot.ref_count += 1;
            slot.times_used += 1;
            slot.last_used = Instant::now();
            Some(slot.clone())
        } else {
            None
        }
    }

    /// Release a reference. Triggers eviction if pending and ref_count=0.
    pub async fn release(&self, model: &str, quant: Quantization) {
        let key = Self::slot_key(model, quant);
        let should_evict = {
            let mut slots = self.slots.write().await;
            if let Some(slot) = slots.get_mut(&key) {
                if slot.ref_count > 0 {
                    slot.ref_count -= 1;
                }
                slot.pending_evict && slot.ref_count == 0
            } else {
                false
            }
        };
        if should_evict {
            self.evict_slot(&key).await;
            info!(model, quant = ?quant, "Last ref released — evicted");
        }
    }

    /// Check if a model with specific quantization is loaded.
    pub async fn is_loaded(&self, model: &str, quant: Quantization) -> bool {
        self.slots
            .read()
            .await
            .contains_key(&Self::slot_key(model, quant))
    }

    /// Check if any version of a model is loaded.
    pub async fn is_any_loaded(&self, model: &str) -> bool {
        self.model_versions.read().await.contains_key(model)
    }

    /// Swap quantization with dual strategy.
    ///
    /// **Atomic Swap** (VRAM > new_size + 0.5GB): Load new first, evict old after.
    /// **Drain & Swap** (VRAM tight): Evict old first, then load new.
    /// Drain timeout: 30 seconds max wait for active inferences.
    pub async fn swap_quantization(
        &self,
        model: &str,
        old_quant: Quantization,
        new_quant: Quantization,
        new_target: ExecutionTarget,
        gpu_vram_free_gb: f64,
    ) -> Result<(), String> {
        if old_quant == new_quant {
            return Ok(());
        }

        let old_key = Self::slot_key(model, old_quant);
        let model_params = self
            .model_sizes
            .read()
            .await
            .get(model)
            .copied()
            .unwrap_or(7.0);
        let new_size_gb = new_quant.model_size_gb(model_params);
        let atomic_swap = gpu_vram_free_gb > new_size_gb + 0.5;

        // Step 1: Mark old as pending_evict
        let old_has_refs = {
            let mut slots = self.slots.write().await;
            if let Some(old_slot) = slots.get_mut(&old_key) {
                old_slot.pending_evict = true;
                info!(
                    model, old_quant = ?old_quant, new_quant = ?new_quant,
                    ref_count = old_slot.ref_count,
                    strategy = if atomic_swap { "atomic" } else { "drain" },
                    "Swap initiated"
                );
                old_slot.ref_count > 0
            } else {
                drop(slots);
                self.mark_loaded(model, new_quant, new_target).await;
                return Ok(());
            }
        };

        if atomic_swap {
            // STRATEGY A: Atomic Swap — both coexist briefly
            self.mark_loaded(model, new_quant, new_target).await;
            if !old_has_refs {
                self.evict_slot(&old_key).await;
                info!(model, "Atomic swap complete — evicted immediately");
            } else {
                info!(
                    model,
                    "Atomic swap — old has active refs, will evict on release"
                );
            }
        } else {
            // STRATEGY B: Drain & Swap — wait, evict, then load
            warn!(model, "VRAM critical — Drain & Swap mode");
            if old_has_refs {
                let drain_timeout = Duration::from_secs(30);
                let start = Instant::now();
                loop {
                    let refs = {
                        let slots = self.slots.read().await;
                        slots.get(&old_key).map(|s| s.ref_count).unwrap_or(0)
                    };
                    if refs == 0 {
                        break;
                    }
                    if start.elapsed() > drain_timeout {
                        warn!(
                            model,
                            refs, "Drain timeout — forcing eviction with {} active refs", refs
                        );
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
            self.evict_slot(&old_key).await;
            self.mark_loaded(model, new_quant, new_target).await;
            info!(model, "Drain & swap complete");
        }

        Ok(())
    }

    /// Evict a specific slot.
    async fn evict_slot(&self, key: &str) {
        let mut slots = self.slots.write().await;
        if let Some(slot) = slots.remove(key) {
            let mut versions = self.model_versions.write().await;
            if let Some(vkeys) = versions.get_mut(&slot.model) {
                vkeys.retain(|k| k != key);
                if vkeys.is_empty() {
                    versions.remove(&slot.model);
                }
            }
            warn!(key, "Model slot evicted");
        }
    }

    /// Evict idle models (zero refs, past timeout).
    pub async fn evict_idle(&self) -> Vec<String> {
        let now = Instant::now();
        let mut to_evict = Vec::new();
        {
            let slots = self.slots.read().await;
            for (key, slot) in slots.iter() {
                let idle = now.duration_since(slot.last_used);
                if idle > self.idle_timeout && slot.can_evict() && !slot.pending_evict {
                    to_evict.push(key.clone());
                }
            }
        }
        for key in &to_evict {
            self.evict_slot(key).await;
        }
        if !to_evict.is_empty() {
            info!(count = to_evict.len(), "Idle eviction complete");
        }
        to_evict
    }

    pub async fn count(&self) -> usize {
        self.slots.read().await.len()
    }

    /// Suggest preloading based on autonomy cycle.
    pub fn suggest_preload(
        &self,
        snap: &HardwareSnapshot,
        priority: Priority,
    ) -> Vec<(&str, ExecutionTarget)> {
        match priority {
            Priority::Think if snap.gpu.vram_free_gb > 4.0 => vec![(
                "qwen2.5-coder:3b",
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )],
            Priority::Dream => snap
                .numa_nodes
                .iter()
                .max_by(|a, b| {
                    a.memory_free_gb
                        .partial_cmp(&b.memory_free_gb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .filter(|n| n.memory_free_gb > 3.0)
                .map(|n| {
                    (
                        "qwen2.5-coder:3b",
                        ExecutionTarget::CpuNuma {
                            node: n.node_id,
                            quant: Quantization::Q2_K,
                        },
                    )
                })
                .into_iter()
                .collect(),
            Priority::Embed if snap.gpu.vram_free_gb > 0.5 => {
                vec![("nomic-embed-text", ExecutionTarget::Gpu(Quantization::Q8_0))]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mark_loaded_and_check() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        assert!(standby.is_loaded("test-model", Quantization::Q4_K_M).await);
        assert!(!standby.is_loaded("test-model", Quantization::Q2_K).await);
        assert!(standby.is_any_loaded("test-model").await);
    }

    #[tokio::test]
    async fn acquire_release_refcount() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        let slot = standby
            .acquire("test-model", Quantization::Q4_K_M)
            .await
            .unwrap();
        assert_eq!(slot.ref_count, 1);
        standby.release("test-model", Quantization::Q4_K_M).await;
    }

    #[tokio::test]
    async fn acquire_rejected_when_evicting() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        {
            let mut slots = standby.slots.write().await;
            slots.get_mut("test-model:Q4_K_M").unwrap().pending_evict = true;
        }
        assert!(standby
            .acquire("test-model", Quantization::Q4_K_M)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn atomic_swap_immediate_evict() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        // Plenty of VRAM → atomic swap
        standby
            .swap_quantization(
                "test-model",
                Quantization::Q4_K_M,
                Quantization::Q2_K,
                ExecutionTarget::CpuNuma {
                    node: 0,
                    quant: Quantization::Q2_K,
                },
                6.0,
            )
            .await
            .unwrap();
        assert!(standby.is_loaded("test-model", Quantization::Q2_K).await);
        assert!(!standby.is_loaded("test-model", Quantization::Q4_K_M).await);
    }

    #[tokio::test]
    async fn drain_swap_low_vram() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        // Very low VRAM → drain & swap (evict first, then load)
        standby
            .swap_quantization(
                "test-model",
                Quantization::Q4_K_M,
                Quantization::Q2_K,
                ExecutionTarget::CpuNuma {
                    node: 0,
                    quant: Quantization::Q2_K,
                },
                0.2,
            )
            .await
            .unwrap();
        assert!(standby.is_loaded("test-model", Quantization::Q2_K).await);
        assert!(!standby.is_loaded("test-model", Quantization::Q4_K_M).await);
    }

    #[tokio::test]
    async fn swap_with_active_inferences() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        let _slot = standby
            .acquire("test-model", Quantization::Q4_K_M)
            .await
            .unwrap();
        // Atomic swap with active refs — both coexist
        standby
            .swap_quantization(
                "test-model",
                Quantization::Q4_K_M,
                Quantization::Q2_K,
                ExecutionTarget::CpuNuma {
                    node: 0,
                    quant: Quantization::Q2_K,
                },
                6.0,
            )
            .await
            .unwrap();
        assert!(standby.is_loaded("test-model", Quantization::Q4_K_M).await); // Still has ref
        assert!(standby.is_loaded("test-model", Quantization::Q2_K).await);
        standby.release("test-model", Quantization::Q4_K_M).await;
        assert!(!standby.is_loaded("test-model", Quantization::Q4_K_M).await);
    }

    #[tokio::test]
    async fn swap_same_quant_noop() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "test-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        standby
            .swap_quantization(
                "test-model",
                Quantization::Q4_K_M,
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
                6.0,
            )
            .await
            .unwrap();
        assert!(standby.is_loaded("test-model", Quantization::Q4_K_M).await);
    }

    #[tokio::test]
    async fn evict_idle() {
        let standby = WarmStandby::new(Duration::from_millis(10));
        standby
            .mark_loaded(
                "old-model",
                Quantization::Q2_K,
                ExecutionTarget::CpuNuma {
                    node: 0,
                    quant: Quantization::Q2_K,
                },
            )
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        let evicted = standby.evict_idle().await;
        assert_eq!(evicted.len(), 1);
        assert!(!standby.is_any_loaded("old-model").await);
    }

    #[tokio::test]
    async fn model_versions_tracking() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby
            .mark_loaded(
                "m",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        standby
            .mark_loaded(
                "m",
                Quantization::Q2_K,
                ExecutionTarget::CpuNuma {
                    node: 0,
                    quant: Quantization::Q2_K,
                },
            )
            .await;
        assert_eq!(standby.count().await, 2);
    }

    #[tokio::test]
    async fn register_model_size() {
        let standby = WarmStandby::new(Duration::from_secs(300));
        standby.register_model("big-model", 31.0).await;
        standby
            .mark_loaded(
                "big-model",
                Quantization::Q4_K_M,
                ExecutionTarget::Gpu(Quantization::Q4_K_M),
            )
            .await;
        // 31B * Q4_K_M ≈ 17.4 GB — won't fit in 8GB VRAM
        let new_size = Quantization::Q2_K.model_size_gb(31.0);
        assert!(new_size < 10.0, "31B Q2_K should be <10GB: {}", new_size);
    }
}
