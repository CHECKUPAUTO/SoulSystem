//! Pre-computed Beta((d-1)/2, (d-1)/2) CDF on [-1, 1] via numerical integration.
//!
//! Replaces `statrs::Beta` for Lloyd-Max codebook boundary computation.

use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;

const LUT_SIZE: usize = 8192;

/// Cached Beta CDF lookup table.
pub struct BetaCDF {
    lut_x: [f32; LUT_SIZE],   // domain values [-1, 1]
    lut_y: [f32; LUT_SIZE],   // CDF values
}

impl BetaCDF {
    /// Get or create the (cached) Beta CDF for the given dimension.
    pub fn new(dim: usize) -> &'static Self {
        // BUG corrige : cache PAR dimension. L'ancien OnceLock<BetaCDF> global ne
        // memorisait qu'UNE CDF -> le 1er dim appele etait reutilise pour tous les
        // autres -> codebook faux en dimension differente.
        static CACHE: OnceLock<Mutex<HashMap<usize, &'static BetaCDF>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut map = cache.lock().unwrap();
        if let Some(&cdf) = map.get(&dim) {
            return cdf;
        }
        let leaked: &'static BetaCDF = Box::leak(Box::new(Self::compute(dim)));
        map.insert(dim, leaked);
        leaked
    }

    fn compute(dim: usize) -> BetaCDF {
        let a = (dim as f64 - 1.0) / 2.0;
        let dx = 2.0 / LUT_SIZE as f64;
        let mut lut_x = [0.0f32; LUT_SIZE];
        let mut lut_y = [0.0f32; LUT_SIZE];

        // CDF via cumulative trapezoidal integration of the Beta PDF.
        let mut cdf = 0.0_f64;
        for i in 0..LUT_SIZE {
            let x = -1.0 + dx * i as f64;
            lut_x[i] = x as f32;

            // Beta PDF proportional to (1+x)^(a-1) * (1-x)^(a-1), symmetric around 0
            let pdf_val = if a >= 1.0 {
                ((1.0 + x).max(1e-30)).powf(a - 1.0)
                    * ((1.0 - x).max(1e-30)).powf(a - 1.0)
            } else {
                1.0 // degenerate case
            };
            cdf += pdf_val * dx;
            lut_y[i] = cdf as f32;
        }

        BetaCDF { lut_x, lut_y }
    }

    /// Sample CDF value at x in [-1, 1].
    pub fn sample_cdf(&self, x: f32) -> f32 {
        let idx = ((x + 1.0) * (LUT_SIZE as f32 - 1.0) / 2.0) as usize;
        self.lut_y[idx.min(LUT_SIZE - 1)]
    }

    /// Inverse CDF: given probability p in [0,1], return x.
    pub fn inverse_cdf(&self, p: f32) -> f32 {
        let total = self.lut_y[LUT_SIZE - 1];
        let target = p * total;
        for i in 1..LUT_SIZE {
            if self.lut_y[i] >= target {
                let frac = (target - self.lut_y[i - 1]) / (self.lut_y[i] - self.lut_y[i - 1]);
                return self.lut_x[i - 1] + frac * (self.lut_x[i] - self.lut_x[i - 1]);
            }
        }
        self.lut_x[LUT_SIZE - 1]
    }

    /// Get quantiles for range [lo, hi] in [-1, 1].
    pub fn quantile(&self, lo: f32, hi: f32) -> (f32, f32) {
        (self.inverse_cdf(lo), self.inverse_cdf(hi))
    }

    /// Get the LUT size constant for external access.
    pub const fn lut_size() -> usize {
        LUT_SIZE
    }
}
