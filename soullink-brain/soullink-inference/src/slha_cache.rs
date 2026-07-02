//! SLHA-compressed KV cache backend.
//!
//! An alternative to the full-fidelity [`crate::mtp::page_cache`] KV cache that
//! stores each token's key as a **128-byte SLHA tile** (INT4 low-rank latent +
//! 1-bit sign-LSH residual) instead of raw `f32` vectors. For a `D_C`-wide key
//! that is a 4× reduction (128·4 = 512 bytes → 128 bytes), and the cache
//! enforces a byte budget via CCOS-style soft-paging (HOT → WARM → evict).
//!
//! This wires the `slha-kernel` crate into the inference engine: keys go in as
//! plain `f32`, are quantized to SLHA tiles, and are held under a bounded
//! memory budget.

use slha::attention::slha_v2::{quantize_latent_grouped, N_GROUPS};
use slha::ccos::{ElasticKvCache, PageOutPolicy};
use slha::{SciRustSlhaTile, D_C, RESIDUAL_WORDS};

/// Number of raw bytes an uncompressed `f32` key of width `D_C` would occupy.
pub const RAW_KEY_BYTES: usize = D_C * 4;
/// Size of one compressed SLHA tile, in bytes.
pub const TILE_BYTES: usize = 128;

/// Build a HOT SLHA tile from a `D_C`-wide key vector (INT4 latent base, empty
/// residual — the caller can refine the residual later if desired).
pub fn tile_from_key(key: &[f32; D_C], token_id: u32, position: u32) -> SciRustSlhaTile {
    let (latent_kv, scale, group_scales) = quantize_latent_grouped(key);
    debug_assert_eq!(group_scales.len(), N_GROUPS);
    SciRustSlhaTile {
        latent_kv,
        residual_bitmap: [0u64; RESIDUAL_WORDS],
        scale,
        dynamic_lambda: 0.0,
        residual_sigma: 0.0,
        token_id,
        position,
        head_id: 0,
        flags: 0, // HOT
        group_scales,
    }
}

/// A byte-budgeted, SLHA-compressed KV cache for one attention head/layer.
pub struct CompressedKvCache {
    inner: ElasticKvCache,
    tokens: usize,
}

impl CompressedKvCache {
    /// New cache holding at most `budget_bytes` of live tile data before
    /// soft-paging kicks in (default lowest-impact-first policy).
    pub fn with_budget_bytes(budget_bytes: usize) -> Self {
        Self {
            inner: ElasticKvCache::new(budget_bytes, PageOutPolicy::default()),
            tokens: 0,
        }
    }

    /// Compress `key` into a tile and insert it; returns the assigned slot.
    pub fn push_key(&mut self, key: &[f32; D_C]) -> usize {
        let id = self.tokens as u32;
        let tile = tile_from_key(key, id, id);
        self.tokens += 1;
        self.inner.insert(tile)
    }

    /// Enforce the byte budget (page out / evict until under budget).
    pub fn enforce_budget(&mut self) {
        self.inner.enforce_budget();
    }

    /// Live (non-evicted) bytes currently held.
    pub fn live_bytes(&self) -> usize {
        self.inner.live_bytes()
    }

    /// Number of tokens ever pushed.
    pub fn tokens(&self) -> usize {
        self.tokens
    }

    /// Compression ratio of one tile vs a raw `f32` key (`= 4.0` for `D_C=128`).
    pub fn compression_ratio(&self) -> f32 {
        RAW_KEY_BYTES as f32 / TILE_BYTES as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: f32) -> [f32; D_C] {
        let mut k = [0.0f32; D_C];
        for (i, v) in k.iter_mut().enumerate() {
            *v = ((i as f32) * 0.1 + seed).sin();
        }
        k
    }

    #[test]
    fn compresses_keys_four_x_vs_raw() {
        let c = CompressedKvCache::with_budget_bytes(1 << 20);
        assert_eq!(RAW_KEY_BYTES, 512);
        assert_eq!(TILE_BYTES, 128);
        assert!((c.compression_ratio() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn each_tile_costs_128_bytes() {
        let mut c = CompressedKvCache::with_budget_bytes(1 << 20);
        for i in 0..10 {
            c.push_key(&key(i as f32));
        }
        assert_eq!(c.tokens(), 10);
        // 10 HOT tiles × 128 B.
        assert_eq!(c.live_bytes(), 10 * TILE_BYTES);
    }

    #[test]
    fn enforces_byte_budget_via_soft_paging() {
        // Budget only fits ~4 HOT tiles; pushing 16 must page/evict down.
        let budget = 4 * TILE_BYTES;
        let mut c = CompressedKvCache::with_budget_bytes(budget);
        for i in 0..16 {
            c.push_key(&key(i as f32));
        }
        c.enforce_budget();
        assert!(
            c.live_bytes() <= budget,
            "budget not enforced: {} > {budget}",
            c.live_bytes()
        );
        assert_eq!(c.tokens(), 16); // all were accepted; some are now WARM/evicted
    }
}
