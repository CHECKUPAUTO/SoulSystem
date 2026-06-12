//! Random orthogonal rotation matrix generation.
//!
//! Generates a deterministic orthogonal matrix via Householder QR decomposition
//! of a seeded Gaussian random matrix. Pure-Rust — no faer or external deps.

use crate::chacha8::ChaCha8Rng;
use crate::ROTATION_SEED;

/// Generate a dim × dim orthogonal matrix (deterministic, seeded).
/// Returns row-major flat Vec<f32> of length dim*dim.
pub fn make_rotation_matrix(dim: usize) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(ROTATION_SEED);

    // Build Gaussian random matrix in f64 for QR stability
    let mut a = vec![0.0_f64; dim * dim];
    for j in 0..dim {
        for i in 0..dim {
            a[j * dim + i] = rng.gen_normal();
        }
    }

    // Householder QR → Q (row-major)
    let q = householder_qr(&a, dim);

    // Convert to f32 row-major
    q.into_iter().map(|v| v as f32).collect()
}

/// Householder QR decomposition returning Q as a flat row-major f64 vec.
fn householder_qr(a: &[f64], dim: usize) -> Vec<f64> {
    let mut a = a.to_vec();

    // Build the product of all Householder reflectors applied to I → Q
    // Start with identity and accumulate H_0 * H_1 * ... * H_{n-1} * I
    let mut q: Vec<f64> = {
        let mut out = vec![0.0_f64; dim * dim];
        for i in 0..dim {
            out[i * dim + i] = 1.0;
        }
        out
    };

    for k in 0..dim.saturating_sub(1) {
        // Extract column below diagonal
        let mut x_norm2 = 0.0_f64;
        for i in k..dim {
            x_norm2 += a[i * dim + k] * a[i * dim + k];
        }
        let x_norm = x_norm2.sqrt();
        if x_norm < 1e-30 {
            continue;
        }

        let sign = if a[k * dim + k] >= 0.0 { -1.0 } else { 1.0 };
        let alpha = sign * x_norm;
        let v_len = dim - k;
        let mut v: Vec<f64> = vec![0.0_f64; v_len];
        // BUG corrige : le vecteur de Householder doit contenir TOUTE la colonne k
        // (sous-diagonale incluse), pas seulement v[0]. Sans la queue, le reflecteur
        // degenere (bascule de signe) et Q n'est pas orthonormale.
        for i in 0..v_len {
            v[i] = a[(k + i) * dim + k];
        }
        v[0] -= alpha;

        let v_norm2 = v.iter().map(|x| x * x).sum::<f64>();
        let v_norm = v_norm2.sqrt();
        if v_norm < 1e-30 {
            continue;
        }
        for vi in &mut v {
            *vi /= v_norm;
        }

        // Apply H_k to remaining A columns: A[k.., k..] -= 2*v*(v^T * A[k.., k..])
        for j in k..dim {
            let mut dot = 0.0_f64;
            for i in 0..v_len {
                dot += v[i] * a[(k + i) * dim + j];
            }
            for i in 0..v_len {
                a[(k + i) * dim + j] -= 2.0 * v[i] * dot;
            }
        }

        // Zero out below diagonal (numerical cleanup)
        for i in k + 1..dim {
            a[i * dim + k] = 0.0;
        }

        // Apply H_k to Q: Q -= 2*v*(v^T * Q[k.., :])
        let mut qt_row_sums: Vec<f64> = vec![0.0_f64; dim];
        for j in 0..dim {
            let mut dot = 0.0_f64;
            for i in 0..v_len {
                dot += v[i] * q[(k + i) * dim + j];
            }
            qt_row_sums[j] = dot;
        }
        for j in 0..dim {
            for i in 0..v_len {
                q[(k + i) * dim + j] -= 2.0 * v[i] * qt_row_sums[j];
            }
        }
    }

    // Sign correction: Q *= diag(sign(diag(R))) where R's diagonal is a[k*dim+k]
    for j in 0..dim {
        let sign = if a[j * dim + j] >= 0.0 { 1.0 } else { -1.0 };
        if sign < 0.0 {
            for i in 0..dim {
                q[i * dim + j] *= -1.0;
            }
        }
    }

    q
}
