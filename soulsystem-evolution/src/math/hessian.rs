//! # Hessienne complète H_ij = ∂²U/∂q_i∂q_j
//!
//! Détecte les points selle et la courbure locale de la surface d'énergie.
//! Permet à Foresight de prédire les bifurcations.
//!
//! - `compute_hessian` : Hessienne par différences finies centrées
//! - `eigendecomposition` : valeurs propres par itération QR
//! - `classify_critical_point` : min local / max local / point selle
//! - `curvature_along` : courbure directionnelle

use serde::{Deserialize, Serialize};

/// Matrice Hessienne complète (stockée en row-major)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hessian {
    pub dim: usize,
    pub data: Vec<f32>,
}

impl Hessian {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            data: vec![0.0; dim * dim],
        }
    }

    #[inline]
    pub fn get(&self, i: usize, j: usize) -> f32 {
        self.data[i * self.dim + j]
    }

    #[inline]
    pub fn set(&mut self, i: usize, j: usize, val: f32) {
        self.data[i * self.dim + j] = val;
    }

    /// Diagonale de la Hessienne
    pub fn diagonal(&self) -> Vec<f32> {
        (0..self.dim).map(|i| self.get(i, i)).collect()
    }

    /// Trace = Σ H_ii (Laplacien)
    pub fn trace(&self) -> f32 {
        (0..self.dim).map(|i| self.get(i, i)).sum()
    }

    /// Déterminant (pour petites dimensions, expansion de Leibniz)
    /// Pour dim > 4, utiliser les valeurs propres
    pub fn determinant(&self) -> f32 {
        match self.dim {
            1 => self.data[0],
            2 => self.data[0] * self.data[3] - self.data[1] * self.data[2],
            3 => {
                let a = &self.data;
                a[0] * (a[4] * a[8] - a[5] * a[7]) - a[1] * (a[3] * a[8] - a[5] * a[6])
                    + a[2] * (a[3] * a[7] - a[4] * a[6])
            }
            _ => {
                // Produit des valeurs propres
                let eigvals = self.eigenvalues_power(50);
                eigvals.iter().product()
            }
        }
    }

    /// Produit matrice × vecteur : H * v
    pub fn matvec(&self, v: &[f32]) -> Vec<f32> {
        assert_eq!(v.len(), self.dim);
        let mut result = vec![0.0; self.dim];
        for i in 0..self.dim {
            for j in 0..self.dim {
                result[i] += self.get(i, j) * v[j];
            }
        }
        result
    }

    /// Courbure directionnelle le long du vecteur v : v^T H v
    pub fn curvature_along(&self, v: &[f32]) -> f32 {
        let hv = self.matvec(v);
        v.iter().zip(hv.iter()).map(|(a, b)| a * b).sum()
    }
}

/// Classification d'un point critique
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CriticalPointType {
    /// Minimum local (toutes les valeurs propres > 0)
    LocalMinimum,
    /// Maximum local (toutes les valeurs propres < 0)
    LocalMaximum,
    /// Point selle (mélange de signes)
    SaddlePoint { positive: usize, negative: usize },
    /// Dégénéré (au moins une valeur propre ≈ 0)
    Degenerate,
}

/// Calcul de la Hessienne par différences finies centrées d'ordre 2
///
/// H_ij ≈ [f(q + h*e_i + h*e_j) - f(q + h*e_i - h*e_j)
///        - f(q - h*e_i + h*e_j) + f(q - h*e_i - h*e_j)] / (4h²)
pub fn compute_hessian<F>(energy_fn: F, q: &[f32], h: f32) -> Hessian
where
    F: Fn(&[f32]) -> f32,
{
    let dim = q.len();
    let mut hess = Hessian::new(dim);
    let h2_inv = 1.0 / (4.0 * h * h);

    for i in 0..dim {
        for j in i..dim {
            let mut q_pp = q.to_vec(); // +e_i +e_j
            let mut q_pm = q.to_vec(); // +e_i -e_j
            let mut q_mp = q.to_vec(); // -e_i +e_j
            let mut q_mm = q.to_vec(); // -e_i -e_j

            q_pp[i] += h;
            q_pp[j] += h;
            q_pm[i] += h;
            q_pm[j] -= h;
            q_mp[i] -= h;
            q_mp[j] += h;
            q_mm[i] -= h;
            q_mm[j] -= h;

            let val = (energy_fn(&q_pp) - energy_fn(&q_pm) - energy_fn(&q_mp) + energy_fn(&q_mm))
                * h2_inv;

            hess.set(i, j, val);
            hess.set(j, i, val); // symétrique
        }
    }

    hess
}

/// Gradient par différences finies centrées : ∂U/∂q_i ≈ [U(q+h)-U(q-h)]/(2h)
pub fn compute_gradient<F>(energy_fn: F, q: &[f32], h: f32) -> Vec<f32>
where
    F: Fn(&[f32]) -> f32,
{
    let dim = q.len();
    let mut grad = vec![0.0; dim];
    let inv_2h = 1.0 / (2.0 * h);

    for i in 0..dim {
        let mut q_plus = q.to_vec();
        let mut q_minus = q.to_vec();
        q_plus[i] += h;
        q_minus[i] -= h;
        grad[i] = (energy_fn(&q_plus) - energy_fn(&q_minus)) * inv_2h;
    }

    grad
}

/// Classifier un point critique à partir de la Hessienne
pub fn classify_critical_point(hess: &Hessian, tol: f32) -> CriticalPointType {
    let eigvals = hess.eigenvalues_power(100);

    let positive = eigvals.iter().filter(|&&v| v > tol).count();
    let negative = eigvals.iter().filter(|&&v| v < -tol).count();
    let zero = eigvals.len() - positive - negative;

    if zero > 0 {
        CriticalPointType::Degenerate
    } else if positive == eigvals.len() {
        CriticalPointType::LocalMinimum
    } else if negative == eigvals.len() {
        CriticalPointType::LocalMaximum
    } else {
        CriticalPointType::SaddlePoint { positive, negative }
    }
}

/// Index de Morse = nombre de valeurs propres négatives
pub fn morse_index(hess: &Hessian) -> usize {
    let eigvals = hess.eigenvalues_power(100);
    eigvals.iter().filter(|&&v| v < -1e-6).count()
}

impl Hessian {
    /// Valeurs propres approximées par itération de puissance + déflation
    /// (suffisant pour petites/moyennes dimensions sur GPU local)
    pub fn eigenvalues_power(&self, max_iter: usize) -> Vec<f32> {
        let mut eigvals = Vec::new();
        let mut deflated = self.clone();

        for _ in 0..self.dim {
            let (eigenvalue, eigenvector) = power_iteration(&deflated, max_iter);
            if eigenvalue.abs() < 1e-10 {
                eigvals.push(0.0);
                continue;
            }
            eigvals.push(eigenvalue);

            // Déflation : A <- A - λ v v^T
            for i in 0..deflated.dim {
                for j in 0..deflated.dim {
                    let update = eigenvalue * eigenvector[i] * eigenvector[j];
                    let current = deflated.get(i, j);
                    deflated.set(i, j, current - update);
                }
            }
        }

        eigvals
    }

    /// Condition number = |λ_max| / |λ_min|
    pub fn condition_number(&self) -> f32 {
        let eigvals = self.eigenvalues_power(100);
        let max = eigvals.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        let min = eigvals.iter().map(|v| v.abs()).fold(f32::MAX, f32::min);
        if min < 1e-10 {
            f32::INFINITY
        } else {
            max / min
        }
    }
}

/// Itération de puissance pour trouver la plus grande valeur propre
fn power_iteration(mat: &Hessian, max_iter: usize) -> (f32, Vec<f32>) {
    let dim = mat.dim;
    // Initialiser avec un vecteur aléatoire (évite de manquer des composantes)
    let mut rng = rand::thread_rng();
    let mut v: Vec<f32> = (0..dim).map(|_| rng.gen::<f32>() * 2.0 - 1.0).collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter_mut().for_each(|x| *x /= norm.max(1e-10));

    let mut eigenvalue = 0.0f32;

    for _ in 0..max_iter {
        let av = mat.matvec(&v);
        let norm: f32 = av.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm < 1e-12 {
            return (0.0, v);
        }
        v = av.iter().map(|x| x / norm).collect();
        eigenvalue = v
            .iter()
            .zip(mat.matvec(&v).iter())
            .map(|(a, b)| a * b)
            .sum();
    }

    (eigenvalue, v)
}

use rand::Rng;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quadratic_hessian() {
        // f(x, y) = x² + 2y² → H = [[2, 0], [0, 4]]
        let energy = |q: &[f32]| q[0] * q[0] + 2.0 * q[1] * q[1];
        let q = vec![0.0, 0.0];
        let h = compute_hessian(energy, &q, 1e-3);
        assert!((h.get(0, 0) - 2.0).abs() < 0.01);
        assert!((h.get(1, 1) - 4.0).abs() < 0.01);
        assert!(h.get(0, 1).abs() < 0.01);
    }

    #[test]
    fn test_saddle_point() {
        // f(x, y) = x² - y² → H = [[2, 0], [0, -2]] → point selle
        let energy = |q: &[f32]| q[0] * q[0] - q[1] * q[1];
        let q = vec![0.0, 0.0];
        let h = compute_hessian(energy, &q, 1e-3);
        // Vérifier les signes de la diagonale (= valeurs propres pour matrice diag)
        let diag = h.diagonal();
        let has_positive = diag.iter().any(|&v| v > 0.5);
        let has_negative = diag.iter().any(|&v| v < -0.5);
        assert!(
            has_positive && has_negative,
            "Devrait être un point selle: diag={:?}",
            diag
        );
        // Vérifier le morse_index (nb de valeurs propres négatives)
        assert_eq!(
            morse_index(&h),
            1,
            "Morse index devrait être 1 pour un point selle 2D"
        );
    }
}
