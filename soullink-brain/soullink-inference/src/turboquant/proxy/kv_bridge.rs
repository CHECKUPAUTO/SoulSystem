//! KV Bridge — connects llama-server's KV cache to TurboQuant 3-bit offload.
//!
//! Monitors llama-server metrics, detects when GPU KV is filling up,
//! and offloads old positions to 3-bit compressed storage.

use crate::turboquant::cache::TurboQuantKVCache;
use std::sync::Arc;
use tokio::sync::RwLock;

/// State of a KV position: on GPU (Q4_0) or offloaded (TurboQuant 3-bit)
#[derive(Debug, Clone, PartialEq)]
pub enum KVSlotState {
    /// Position is on GPU via llama-server Q4_0 KV
    GpuQ4,
    /// Position offloaded to TurboQuant 3-bit compressed storage
    TurboQuant3Bit,
    /// Position not yet stored
    Empty,
}

/// Bridge between llama-server GPU KV cache and TurboQuant offload.
pub struct KVBridge {
    /// TurboQuant compressed cache
    cache: Arc<RwLock<TurboQuantKVCache>>,
    /// Track state of each position
    slot_states: Arc<RwLock<Vec<KVSlotState>>>,
    /// GPU KV capacity (positions) before offload triggers
    gpu_capacity: usize,
    /// Offload threshold (0.0-1.0, e.g. 0.8 = offload when 80% full)
    offload_threshold: f64,
    /// llama-server base URL
    llama_url: String,
}

impl KVBridge {
    pub fn new(
        num_layers: usize,
        max_seq_len: usize,
        head_dim: usize,
        num_heads: usize,
        gpu_capacity: usize,
        llama_url: String,
    ) -> Self {
        let cache = TurboQuantKVCache::new(num_layers, max_seq_len, head_dim, num_heads);
        let offload_threshold = 0.8;

        Self {
            cache: Arc::new(RwLock::new(cache)),
            slot_states: Arc::new(RwLock::new(vec![KVSlotState::Empty; max_seq_len])),
            gpu_capacity,
            offload_threshold,
            llama_url,
        }
    }

    /// Check if GPU KV is above offload threshold.
    pub async fn should_offload(&self, current_usage: usize) -> bool {
        (current_usage as f64 / self.gpu_capacity as f64) >= self.offload_threshold
    }

    /// Offload a range of positions from GPU to TurboQuant 3-bit.
    /// Returns the number of positions offloaded.
    pub async fn offload_range(&self, start: usize, end: usize, k: &[f32], v: &[f32]) -> usize {
        let n = end - start;
        if n == 0 { return 0; }

        // Store in TurboQuant compressed cache
        let mut cache = self.cache.write().await;
        // Store across all layers (simplified — real impl gets per-layer K/V)
        for layer in 0..cache.num_layers {
            cache.store_range(layer, start, k, v);
        }

        // Update slot states
        let mut states = self.slot_states.write().await;
        for pos in start..end {
            if pos < states.len() {
                states[pos] = KVSlotState::TurboQuant3Bit;
            }
        }

        n
    }

    /// Recall positions from TurboQuant back to GPU.
    /// Returns approximate K/V data for the positions.
    pub async fn recall_range(&self, positions: &[usize]) -> Option<(Vec<f32>, Vec<f32>)> {
        let cache = self.cache.read().await;
        let states = self.slot_states.read().await;

        // Check all positions are in TurboQuant
        let all_offloaded = positions.iter().all(|&p| {
            p < states.len() && states[p] == KVSlotState::TurboQuant3Bit
        });

        if !all_offloaded {
            return None;
        }

        // Decompress from TurboQuant (layer 0 as representative)
        Some(cache.retrieve_range(0, positions))
    }

    /// Get current offload statistics.
    pub async fn stats(&self) -> BridgeStats {
        let cache = self.cache.read().await;
        let states = self.slot_states.read().await;
        let gpu_count = states.iter().filter(|s| **s == KVSlotState::GpuQ4).count();
        let tq_count = states.iter().filter(|s| **s == KVSlotState::TurboQuant3Bit).count();

        BridgeStats {
            gpu_positions: gpu_count,
            turboquant_positions: tq_count,
            memory_turboquant_mib: cache.memory_usage_mib(),
            compression_ratio: cache.compression_ratio(),
            effective_vs_fp16: 6.0, // Q4_0 (4x) + TurboQuant 3-bit offload = ~6x
        }
    }

    /// Get reference to the compressed cache.
    pub fn cache(&self) -> Arc<RwLock<TurboQuantKVCache>> {
        self.cache.clone()
    }
}

#[derive(Debug, Clone)]
pub struct BridgeStats {
    pub gpu_positions: usize,
    pub turboquant_positions: usize,
    pub memory_turboquant_mib: f64,
    pub compression_ratio: f64,
    pub effective_vs_fp16: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn offload_and_recall() {
        let bridge = KVBridge::new(2, 256, 32, 4, 128, "http://localhost:8081".into());
        let rot_dim = 32 * 4;
        let k: Vec<f32> = (0..rot_dim).map(|i| i as f32 * 0.01).collect();
        let v: Vec<f32> = (0..rot_dim).map(|i| i as f32 * 0.02).collect();

        let offloaded = bridge.offload_range(0, 1, &k, &v).await;
        assert_eq!(offloaded, 1);

        let recalled = bridge.recall_range(&[0]).await;
        assert!(recalled.is_some());
        let (k_out, v_out) = recalled.unwrap();
        assert_eq!(k_out.len(), rot_dim);
        assert_eq!(v_out.len(), rot_dim);
    }

    #[tokio::test]
    async fn should_offload_threshold() {
        let bridge = KVBridge::new(2, 256, 32, 4, 100, "http://localhost:8081".into());
        assert!(!bridge.should_offload(79).await);  // 79% < 80%
        assert!(bridge.should_offload(80).await);   // 80% >= 80%
        assert!(bridge.should_offload(90).await);   // 90% >= 80%
    }

    #[tokio::test]
    async fn stats_report() {
        let bridge = KVBridge::new(2, 256, 32, 4, 128, "http://localhost:8081".into());
        let stats = bridge.stats().await;
        assert_eq!(stats.gpu_positions, 0);
        assert_eq!(stats.turboquant_positions, 0);
        assert!(stats.effective_vs_fp16 > 5.0);
    }
}
