//! IO-aware tiled scan for hardware-efficient SSM inference.
//!
//! Inspired by Flash Attention's tiling approach, this module splits the
//! sequence into tiles that fit in fast memory (L1 cache / registers) and
//! processes them with minimal main-memory traffic.
//!
//! # Algorithm
//!
//! 1. Split sequence L into tiles of size `TILE_SIZE`
//! 2. For each tile (sequential inter-tile):
//!    a. Load tile into "SRAM" (contiguous buffer for cache locality)
//!    b. Run parallel scan within the tile (intra-tile)
//!    c. Propagate the final state from the previous tile
//!    d. Write tile outputs to main memory
//!
//! This gives O(L × TILE_SIZE) computation with O(TILE_SIZE²) peak memory,
//! effectively trading off some parallelism for dramatically better cache
//! behavior on long sequences.

use ndarray::{Array1, Array2};

use crate::parallel::MultiDimScan;
use crate::ssm;

/// Default tile size (typical L1 cache can hold this many N-dim states).
pub const DEFAULT_TILE_SIZE: usize = 64;

/// Tiled scan for SSM state propagation.
pub struct TiledScan;

impl TiledScan {
    /// Run a tiled selective scan over a sequence.
    ///
    /// Each tile processes a chunk of the sequence using the parallel scan,
    /// then propagates the final hidden state forward.
    ///
    /// # Arguments
    ///
    /// * `a_seq` — Discretized transition matrices (L, N, N)
    /// * `b_seq` — Discretized input vectors (L, N)
    /// * `tile_size` — Number of timesteps per tile
    /// * `h0` — Initial state (N,)
    ///
    /// # Returns
    ///
    /// State sequence (L, N)
    pub fn scan(
        a_seq: &[Array2<f64>],
        b_seq: &[Array1<f64>],
        tile_size: usize,
        h0: &Array1<f64>,
    ) -> Vec<Array1<f64>> {
        let l = a_seq.len();
        assert_eq!(b_seq.len(), l);

        let tile = tile_size.max(1);
        let mut h = h0.clone();
        let mut states = Vec::with_capacity(l);

        let mut t = 0;
        while t < l {
            let end = (t + tile).min(l);
            let chunk_len = end - t;

            // Load tile into local buffers
            let mut a_tile: Vec<Array2<f64>> = Vec::with_capacity(chunk_len);
            let mut b_tile: Vec<Array1<f64>> = Vec::with_capacity(chunk_len);
            for i in t..end {
                a_tile.push(a_seq[i].clone());
                b_tile.push(b_seq[i].clone());
            }

            if chunk_len == 1 {
                // Single element: direct recurrence
                let a = &a_tile[0];
                let b = &b_tile[0];
                h = a.dot(&h) + b;
                states.push(h.clone());
                t = end;
                continue;
            }

            // Intra-tile parallel scan
            let prefixes = MultiDimScan::parallel_scan(&a_tile, &b_tile);

            // Apply with inter-tile state propagation
            // h_t = A_combined · h_prev_tile + B_combined
            for (_i, (prefix_a, prefix_b)) in prefixes.iter().enumerate() {
                let mut hi = prefix_a.dot(&h);
                hi += prefix_b;
                states.push(hi);
            }

            // Propagate final h to next tile
            if let Some(last) = states.last() {
                h = last.clone();
            }

            t = end;
        }

        states
    }

    /// Tiled scan with explicit SRAM budget (simulated).
    ///
    /// `sram_size` is the number of f64 values that fit in fast memory.
    /// The tile size is derived as `sram_size / (N² + N)` for (A, B) pairs.
    ///
    /// # Arguments
    ///
    /// * `a_seq` — Discretized transition matrices (L, N, N)
    /// * `b_seq` — Discretized input vectors (L, N)
    /// * `h0` — Initial state (N,)
    /// * `sram_size` — Simulated SRAM capacity in f64 elements
    ///
    /// # Returns
    ///
    /// State sequence (L, N)
    pub fn scan_with_sram_budget(
        a_seq: &[Array2<f64>],
        b_seq: &[Array1<f64>],
        h0: &Array1<f64>,
        sram_size: usize,
    ) -> Vec<Array1<f64>> {
        if a_seq.is_empty() {
            return vec![];
        }
        let n = a_seq[0].shape()[0];
        // Each (A, B) pair costs N² + N f64 values
        let pair_size = n * n + n;
        let tile_size = (sram_size / pair_size).max(1).min(128);
        Self::scan(a_seq, b_seq, tile_size, h0)
    }

    /// Run tiled scan on a pre-computed selective SSM setup.
    ///
    /// Convenience wrapper for use with `selective::MultiDimSelectiveSSM`.
    ///
    /// # Arguments
    ///
    /// * `a_bar_seq` — Discretized A per timestep per channel (d_inner, L, N, N)
    /// * `b_bar_seq` — Discretized B per timestep per channel (d_inner, L, N)
    /// * `x` — Original input (L, d_inner) — for output computation
    /// * `c_proj` — Per-channel C vectors (d_inner, N)
    /// * `d` — Feedthrough scalar
    /// * `tile_size` — Number of timesteps per tile
    ///
    /// # Returns
    ///
    /// Output sequence (L, d_inner)
    pub fn selective_tiled_scan(
        a_bar_seq: &[Vec<Array2<f64>>], // (d_inner,) each (L, N, N)
        b_bar_seq: &[Vec<Array1<f64>>], // (d_inner,) each (L, N)
        x: &Array2<f64>,
        c_proj: &Array2<f64>,
        d: f64,
        tile_size: usize,
    ) -> Array2<f64> {
        let length = x.shape()[0];
        let d_inner = x.shape()[1];
        let n = c_proj.shape()[1];

        let mut y = Array2::zeros((length, d_inner));

        for ch in 0..d_inner {
            let h0 = Array1::zeros(n);
            let states = Self::scan(&a_bar_seq[ch], &b_bar_seq[ch], tile_size, &h0);

            for t in 0..length {
                let ci = c_proj.row(ch);
                y[[t, ch]] = ci.dot(&states[t]) + d * x[[t, ch]];
            }
        }

        y
    }

    /// Fused tiled scan: compute discretization on-the-fly within each tile.
    ///
    /// This avoids materializing all (Ā_t, B̄_t) for the full sequence at once.
    /// Instead, each tile computes its own discretization and scan, then moves on.
    /// This is the key IO-aware optimization: discretization and scan share the
    /// same data in fast memory.
    ///
    /// # Arguments
    ///
    /// * `a` — State matrix (N, N)
    /// * `b_shared` — B-projected inputs (L, N)
    /// * `deltas` — Per-timestep Δ values (L,)
    /// * `h0` — Initial state (N,)
    /// * `tile_size` — Tile size
    ///
    /// # Returns
    ///
    /// States (L, N)
    pub fn fused_tiled_scan(
        a: &Array2<f64>,
        b_shared: &[Array1<f64>],
        deltas: &[f64],
        h0: &Array1<f64>,
        tile_size: usize,
    ) -> Vec<Array1<f64>> {
        let l = b_shared.len();
        assert_eq!(deltas.len(), l);

        let tile = tile_size.max(1);
        let mut h = h0.clone();
        let mut states = Vec::with_capacity(l);

        let mut t = 0;
        while t < l {
            let end = (t + tile).min(l);
            let chunk_len = end - t;

            // --- Tile loads: discretize A and B on the fly ---
            let mut a_tile: Vec<Array2<f64>> = Vec::with_capacity(chunk_len);
            let mut b_tile: Vec<Array1<f64>> = Vec::with_capacity(chunk_len);

            for i in t..end {
                let (a_bar, b_bar) = ssm::discretize_zoh(a, &b_shared[i], deltas[i]);
                a_tile.push(a_bar);
                b_tile.push(b_bar);
            }

            // --- Intra-tile parallel scan ---
            if chunk_len == 1 {
                h = a_tile[0].dot(&h) + &b_tile[0];
                states.push(h.clone());
            } else {
                let prefixes = MultiDimScan::parallel_scan(&a_tile, &b_tile);
                for (pa, pb) in prefixes.iter() {
                    let mut hi = pa.dot(&h);
                    hi += pb;
                    states.push(hi);
                }
                // Propagate final state
                if let Some(last) = states.last() {
                    h = last.clone();
                }
            }

            t = end;
        }

        states
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array2;

    #[test]
    fn test_tiled_scan_matches_sequential() {
        let n = 4;
        let l = 16;

        // Generate random A matrices (diagonal-dominant)
        let mut rng = rand::thread_rng();
        let a_seq: Vec<Array2<f64>> = (0..l)
            .map(|_| {
                let mut a = Array2::zeros((n, n));
                for i in 0..n {
                    for j in 0..n {
                        a[[i, j]] = rand::Rng::gen_range(&mut rng, -0.2..0.2);
                    }
                    a[[i, i]] += 0.5;
                }
                a
            })
            .collect();

        let b_seq: Vec<Array1<f64>> = (0..l)
            .map(|_| Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.3..0.3)))
            .collect();

        let h0 = Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.1..0.1));

        // Sequential reference
        let mut h = h0.clone();
        let mut ref_states = Vec::with_capacity(l);
        for t in 0..l {
            h = a_seq[t].dot(&h) + &b_seq[t];
            ref_states.push(h.clone());
        }

        // Tiled with various tile sizes
        for &tile_size in &[1, 2, 4, 8, 16, 32] {
            let tiled = TiledScan::scan(&a_seq, &b_seq, tile_size, &h0);
            assert_eq!(tiled.len(), l);
            for t in 0..l {
                for i in 0..n {
                    assert!(
                        (tiled[t][i] - ref_states[t][i]).abs() < 1e-10,
                        "Tile={}, t={}, i={}: tiled={}, ref={}",
                        tile_size,
                        t,
                        i,
                        tiled[t][i],
                        ref_states[t][i]
                    );
                }
            }
        }
    }

    #[test]
    fn test_tiled_scan_empty() {
        let empty_a: &[Array2<f64>] = &[];
        let empty_b: &[Array1<f64>] = &[];
        let h0 = Array1::zeros(3);
        let result = TiledScan::scan(empty_a, empty_b, 64, &h0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sram_budget_tile_size() {
        let n = 8;
        let l = 32;
        let mut rng = rand::thread_rng();

        let a_seq: Vec<Array2<f64>> = (0..l).map(|_| Array2::eye(n)).collect();
        let b_seq: Vec<Array1<f64>> = (0..l)
            .map(|_| Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.1..0.1)))
            .collect();
        let h0 = Array1::zeros(n);

        // With small SRAM, tile size should be at least 1
        let _result = TiledScan::scan_with_sram_budget(&a_seq, &b_seq, &h0, 16);
        // Just check it doesn't panic
    }

    #[test]
    fn test_fused_tiled_scan() {
        let n = 4;
        let l = 12;
        let a = Array2::eye(n) * -0.5;

        let mut rng = rand::thread_rng();
        let b_shared: Vec<Array1<f64>> = (0..l)
            .map(|_| Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.2..0.2)))
            .collect();
        let deltas: Vec<f64> = (0..l)
            .map(|_| rand::Rng::gen_range(&mut rng, 0.01..0.05))
            .collect();
        let h0 = Array1::zeros(n);

        // Sequential reference with explicit discretization
        let mut h_seq = h0.clone();
        let mut ref_states = Vec::with_capacity(l);
        for t in 0..l {
            let (a_bar, b_bar) = ssm::discretize_zoh(&a, &b_shared[t], deltas[t]);
            h_seq = a_bar.dot(&h_seq) + &b_bar;
            ref_states.push(h_seq.clone());
        }

        // Fused tiled
        let fused = TiledScan::fused_tiled_scan(&a, &b_shared, &deltas, &h0, 4);
        assert_eq!(fused.len(), l);

        for t in 0..l {
            for i in 0..n {
                assert!(
                    (fused[t][i] - ref_states[t][i]).abs() < 1e-10,
                    "t={}, i={}: fused={}, ref={}",
                    t,
                    i,
                    fused[t][i],
                    ref_states[t][i]
                );
            }
        }
    }

    #[test]
    fn test_tiled_scan_single_element_tile() {
        let n = 3;
        let l = 5;
        let a_seq: Vec<Array2<f64>> = (0..l).map(|_| Array2::eye(n)).collect();
        let b_seq: Vec<Array1<f64>> = (0..l)
            .map(|i| Array1::from_elem(n, (i + 1) as f64))
            .collect();
        let h0 = Array1::zeros(n);

        // Reference: h_t = h_{t-1} + B_t (since A = I)
        let mut h = h0.clone();
        let mut ref_states = Vec::with_capacity(l);
        for t in 0..l {
            h = h + &b_seq[t];
            ref_states.push(h.clone());
        }

        let tiled = TiledScan::scan(&a_seq, &b_seq, 1, &h0);
        for t in 0..l {
            for i in 0..n {
                assert!((tiled[t][i] - ref_states[t][i]).abs() < 1e-10);
            }
        }
    }
}
