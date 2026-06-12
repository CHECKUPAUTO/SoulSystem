//! # SVD Tronquée — A ≈ U_r Σ_r V_r^T
//!
//! Compression LoRA du vecteur d'état q (4096 → r << 4096).
//! Essentiel pour la mémoire à long terme.
//!
//! Algorithme : itération de puissance randomisée (Halko, Martinsson, Tropp 2011)
//! Adapté pour f32 sur GPU local sans LAPACK.

use serde::{Deserialize, Serialize};
use rand::Rng;

/// Résultat d'une SVD tronquée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncatedSVD {
    /// Matrice U (m × r) — vecteurs singuliers gauches
    pub u: Vec<Vec<f32>>,
    /// Valeurs singulières Σ (r)
    pub sigma: Vec<f32>,
    /// Matrice V^T (r × n) — vecteurs singuliers droits transposés
    pub vt: Vec<Vec<f32>>,
    /// Rang de troncature
    pub rank: usize,
    /// Ratio d'énergie capturée (Σ_r² / Σ_total²)
    pub energy_ratio: f32,
}

impl TruncatedSVD {
    /// Reconstruire l'approximation A ≈ U_r Σ_r V_r^T
    pub fn reconstruct(&self) -> Vec<Vec<f32>> {
        let m = self.u.len();
        let n = if self.vt.is_empty() { 0 } else { self.vt[0].len() };
        let mut result = vec![vec![0.0; n]; m];

        for k in 0..self.rank {
            let s = self.sigma[k];
            for i in 0..m {
                for j in 0..n {
                    result[i][j] += self.u[i][k] * s * self.vt[k][j];
                }
            }
        }

        result
    }

    /// Compresser un vecteur q de dim n vers dim r : q_compressed = V_r^T q
    pub fn compress(&self, q: &[f32]) -> Vec<f32> {
        let mut compressed = vec![0.0; self.rank];
        for k in 0..self.rank {
            for j in 0..q.len().min(self.vt[k].len()) {
                compressed[k] += self.vt[k][j] * q[j];
            }
        }
        compressed
    }

    /// Décompresser : q_approx = V_r q_compressed
    pub fn decompress(&self, compressed: &[f32]) -> Vec<f32> {
        let n = if self.vt.is_empty() { 0 } else { self.vt[0].len() };
        let mut q = vec![0.0; n];
        for k in 0..self.rank.min(compressed.len()) {
            for j in 0..n {
                q[j] += self.vt[k][j] * compressed[k];
            }
        }
        q
    }

    /// Erreur de reconstruction relative
    pub fn reconstruction_error(&self, original: &[Vec<f32>]) -> f32 {
        let approx = self.reconstruct();
        let mut err_sq = 0.0f32;
        let mut orig_sq = 0.0f32;

        for i in 0..original.len() {
            for j in 0..original[i].len() {
                let diff = original[i][j] - approx[i][j];
                err_sq += diff * diff;
                orig_sq += original[i][j] * original[i][j];
            }
        }

        if orig_sq < 1e-12 { 0.0 } else { (err_sq / orig_sq).sqrt() }
    }
}

/// Calculer la SVD tronquée de rang r par itération de puissance randomisée
///
/// `matrix` : matrice m × n (row-major)
/// `rank` : rang de troncature r
/// `power_iter` : nombre d'itérations de puissance (2-5 suffisent)
pub fn truncated_svd(matrix: &[Vec<f32>], rank: usize, power_iter: usize) -> TruncatedSVD {
    let m = matrix.len();
    let n = if m == 0 { 0 } else { matrix[0].len() };
    let r = rank.min(m).min(n);

    if r == 0 || m == 0 || n == 0 {
        return TruncatedSVD {
            u: vec![], sigma: vec![], vt: vec![], rank: 0, energy_ratio: 0.0,
        };
    }

    let mut rng = rand::thread_rng();

    // Étape 1 : Matrice aléatoire Ω (n × r)
    let omega: Vec<Vec<f32>> = (0..n)
        .map(|_| (0..r).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();

    // Étape 2 : Y = A Ω (m × r)
    let mut y = mat_mul(matrix, &omega, m, n, r);

    // Étape 3 : Itérations de puissance pour améliorer le range
    for _ in 0..power_iter {
        // Y = A^T Y (n × r)
        let aty = mat_mul_transpose_left(matrix, &y, m, n, r);
        // Y = A (A^T Y) (m × r)
        y = mat_mul(matrix, &aty, m, n, r);
    }

    // Étape 4 : QR de Y → Q (m × r) orthonormale
    let q = gram_schmidt(&y, m, r);

    // Étape 5 : B = Q^T A (r × n)
    let b = mat_mul_transpose_left(&q, matrix, m, r, n);

    // Étape 6 : SVD de la petite matrice B (r × n)
    // Par itération de puissance sur B B^T
    let (u_b, sigma, vt) = small_svd(&b, r, n);

    // Étape 7 : U = Q U_B
    let u = mat_mul(&q, &u_b, m, r, r);

    // Calcul du ratio d'énergie
    let total_energy: f32 = matrix.iter()
        .flat_map(|row| row.iter())
        .map(|x| x * x)
        .sum();
    let captured_energy: f32 = sigma.iter().map(|s| s * s).sum();
    let energy_ratio = if total_energy > 0.0 { captured_energy / total_energy } else { 0.0 };

    TruncatedSVD { u, sigma, vt, rank: r, energy_ratio }
}

/// Adaptation LoRA : décompose un delta de poids en deux matrices low-rank
pub fn lora_decompose(delta_weights: &[Vec<f32>], rank: usize) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let svd = truncated_svd(delta_weights, rank, 3);

    // A = U √Σ, B = √Σ V^T
    let m = svd.u.len();
    let n = if svd.vt.is_empty() { 0 } else { svd.vt[0].len() };

    let mut a = vec![vec![0.0; svd.rank]; m];
    let mut b = vec![vec![0.0; n]; svd.rank];

    for k in 0..svd.rank {
        let sqrt_s = svd.sigma[k].sqrt();
        for i in 0..m {
            a[i][k] = svd.u[i][k] * sqrt_s;
        }
        for j in 0..n {
            b[k][j] = sqrt_s * svd.vt[k][j];
        }
    }

    (a, b)
}

// ── Helpers matriciels ──────────────────────

/// C = A * B (m×k @ k×n → m×n) — row-major
fn mat_mul(a: &[Vec<f32>], b: &[Vec<f32>], m: usize, k: usize, n: usize) -> Vec<Vec<f32>> {
    let mut c = vec![vec![0.0; n]; m];
    for i in 0..m {
        for j in 0..n {
            for l in 0..k {
                c[i][j] += a[i][l] * b[l][j];
            }
        }
    }
    c
}

/// C = A^T * B (k×m transposé @ m×n → k×n)
fn mat_mul_transpose_left(a: &[Vec<f32>], b: &[Vec<f32>], m: usize, k: usize, n: usize) -> Vec<Vec<f32>> {
    let mut c = vec![vec![0.0; n]; k];
    for i in 0..k {
        for j in 0..n {
            for l in 0..m {
                c[i][j] += a[l][i] * b[l][j];
            }
        }
    }
    c
}

/// Orthonormalisation de Gram-Schmidt modifiée
fn gram_schmidt(y: &[Vec<f32>], m: usize, r: usize) -> Vec<Vec<f32>> {
    let mut q = y.to_vec();

    for j in 0..r {
        // Soustraire les projections sur les colonnes précédentes
        for k in 0..j {
            let dot: f32 = (0..m).map(|i| q[i][j] * q[i][k]).sum();
            for i in 0..m {
                q[i][j] -= dot * q[i][k];
            }
        }
        // Normaliser
        let norm: f32 = (0..m).map(|i| q[i][j] * q[i][j]).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for i in 0..m {
                q[i][j] /= norm;
            }
        }
    }

    q
}

/// SVD d'une petite matrice (r × n) par itération de puissance
fn small_svd(b: &[Vec<f32>], r: usize, n: usize) -> (Vec<Vec<f32>>, Vec<f32>, Vec<Vec<f32>>) {
    let mut sigma = Vec::new();
    let mut u_vecs: Vec<Vec<f32>> = Vec::new();
    let mut v_vecs: Vec<Vec<f32>> = Vec::new();
    let mut residual = b.to_vec();

    for _ in 0..r {
        // Itération de puissance sur residual * residual^T
        let mut v: Vec<f32> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();

        for _ in 0..50 {
            // u = B v
            let mut u = vec![0.0; r];
            for i in 0..r {
                for j in 0..n {
                    u[i] += residual[i][j] * v[j];
                }
            }
            let u_norm: f32 = u.iter().map(|x| x * x).sum::<f32>().sqrt();
            if u_norm < 1e-12 { break; }
            u.iter_mut().for_each(|x| *x /= u_norm);

            // v = B^T u
            v = vec![0.0; n];
            for j in 0..n {
                for i in 0..r {
                    v[j] += residual[i][j] * u[i];
                }
            }
            let v_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if v_norm < 1e-12 { break; }
            v.iter_mut().for_each(|x| *x /= v_norm);
        }

        // σ = u^T B v
        let mut s = 0.0f32;
        let mut u_final = vec![0.0; r];
        for i in 0..r {
            for j in 0..n {
                u_final[i] += residual[i][j] * v[j];
            }
        }
        let u_norm: f32 = u_final.iter().map(|x| x * x).sum::<f32>().sqrt();
        s = u_norm;
        if s > 1e-10 {
            u_final.iter_mut().for_each(|x| *x /= s);
        }

        sigma.push(s);
        u_vecs.push(u_final.clone());
        v_vecs.push(v.clone());

        // Déflation
        for i in 0..r {
            for j in 0..n {
                residual[i][j] -= s * u_final[i] * v[j];
            }
        }
    }

    // Reformater en matrices
    let u_mat: Vec<Vec<f32>> = (0..r).map(|i| u_vecs.iter().map(|u| u[i]).collect()).collect();
    let vt_mat = v_vecs;

    (u_mat, sigma, vt_mat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svd_rank1() {
        // Matrice de rang 1 : [[1,2],[2,4]]
        let mat = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let svd = truncated_svd(&mat, 1, 3);
        assert_eq!(svd.rank, 1);
        assert!(svd.sigma[0] > 4.0); // √(1+4+4+16) ≈ 5
        assert!(svd.energy_ratio > 0.95);
    }

    #[test]
    fn test_compress_decompress() {
        let mat = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 2.0, 0.0, 0.0],
            vec![0.0, 0.0, 3.0, 0.0],
        ];
        let svd = truncated_svd(&mat, 2, 3);
        let q = vec![1.0, 1.0, 1.0, 0.0];
        let compressed = svd.compress(&q);
        assert_eq!(compressed.len(), 2);
        let decompressed = svd.decompress(&compressed);
        assert_eq!(decompressed.len(), 4);
    }
}
