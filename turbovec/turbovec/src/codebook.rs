//! Lloyd-Max scalar quantizer for the Beta distribution.
//!
//! After orthogonal rotation, each coordinate of a unit vector on S^{d-1}
//! follows Beta((d-1)/2, (d-1)/2) on [-1, 1]. This module computes optimal
//! quantization boundaries and centroids via Lloyd-Max iteration using the
//! pre-computed Beta CDF LUT instead of statrs.

use crate::beta_lut::BetaCDF;

/// Returns (boundaries, centroids) for the given bit width and dimension.
pub fn codebook(bits: usize, dim: usize) -> (Vec<f32>, Vec<f32>) {
    lloyd_max(bits, dim)
}

fn lloyd_max(bits: usize, dim: usize) -> (Vec<f32>, Vec<f32>) {
    let n_levels = 1usize << bits;
    let beta_cdf = BetaCDF::new(dim);

    // Initialize centroids evenly spaced on [-1, 1]
    let mut centroids: Vec<f64> = (0..n_levels)
        .map(|i| -1.0 + 2.0 * i as f64 / (n_levels.max(2) as f64 - 1.0))
        .collect();

    for _ in 0..50 {
        // Boundaries = midpoints between consecutive centroids
        let mut boundaries: Vec<f64> = vec![-1.0_f64];
        for i in 0..n_levels - 1 {
            boundaries.push((centroids[i] + centroids[i + 1]) / 2.0);
        }
        boundaries.push(1.0_f64);

        let mut new_centroids = vec![0.0f64; n_levels];

        for i in 0..n_levels {
            let lo = boundaries[i];
            let hi = boundaries[i + 1];

            // CDF values via lookup table (CDF is already cumulative)
            let cdf_lo = beta_cdf.sample_cdf(lo as f32);
            let cdf_hi = beta_cdf.sample_cdf(hi as f32);
            let prob = cdf_hi - cdf_lo;

            if prob < 1e-15 {
                new_centroids[i] = (lo + hi) / 2.0;
                continue;
            }

            // Centroid = E[X | lo < X < hi] via numerical integration on CDF derivative
            let mut mass = 0.0_f64;
            let mut moment = 0.0_f64;
            let dx = 2.0 / BetaCDF::lut_size() as f64;
            for j in 1..BetaCDF::lut_size() {
                let x = -1.0 + dx * j as f64;
                if x >= lo && x <= hi {
                    let cdf_j = beta_cdf.sample_cdf(x as f32);
                    let pdf_f64 = (cdf_j as f64 - beta_cdf.sample_cdf((x - dx) as f32) as f64) / dx.max(1e-30);
                    mass += pdf_f64 * dx;
                    moment += x * pdf_f64 * dx;
                }
            }

            new_centroids[i] = if mass > 1e-15 { moment / mass } else { (lo + hi) / 2.0 };
        }

        // Force la symetrie : Beta((d-1)/2,(d-1)/2) est symetrique autour de 0, donc
        // le quantificateur MMSE l'est aussi. On projette chaque mise a jour sur
        // l'espace symetrique -> empeche la derive asymetrique (cellules de queue
        // vides -> repli sur le milieu) aux grandes dimensions.
        for i in 0..n_levels / 2 {
            let half = (new_centroids[i] - new_centroids[n_levels - 1 - i]) / 2.0;
            new_centroids[i] = half;
            new_centroids[n_levels - 1 - i] = -half;
        }

        // Check convergence
        let err: f64 = centroids
            .iter()
            .zip(new_centroids.iter())
            .map(|(c, nc)| (c - nc).abs())
            .sum();
        if err < 1e-8 {
            break;
        }
        centroids = new_centroids;
    }

    // Frontieres exportees = les n_levels-1 milieux internes uniquement
    // (boundary[i] entre centroids[i] et centroids[i+1]) ; pas les bords +/-1.
    let boundaries: Vec<f32> = (0..n_levels - 1)
        .map(|i| ((centroids[i] + centroids[i + 1]) / 2.0) as f32)
        .collect();

    (boundaries, centroids.iter().map(|&v| v as f32).collect())
}
