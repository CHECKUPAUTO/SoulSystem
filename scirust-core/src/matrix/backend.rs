//! Trait for SIMD-accelerated matrix operations.
//!
//! Each backend (CPU SIMD, CUDA, WebGPU) implements this trait to provide
//! optimized BLAS-like primitives.

use super::view::{MatrixShape, MatrixView, MatrixViewMut};

/// Compute backend for matrix and vector operations.
pub trait SimdBackend {
    /// Human-readable backend name (e.g. "AVX2", "CUDA", "WebGPU").
    fn name(&self) -> &str;

    /// Single-precision AXPY: `y = a*x + y`.
    fn saxpy_f32(&self, n: usize, a: f32, x: &[f32], y: &mut [f32]);

    /// Double-precision AXPY.
    fn daxpy_f64(&self, n: usize, a: f64, x: &[f64], y: &mut [f64]);

    /// Single-precision dot product.
    fn sdot_f32(&self, n: usize, x: &[f32], y: &[f32]) -> f32;

    /// Double-precision dot product.
    fn ddot_f64(&self, n: usize, x: &[f64], y: &[f64]) -> f64;

    /// Single-precision matrix-vector multiply: `y = A*x`.
    fn sgemv_f32(&self, a: &MatrixView<f32>, x: &[f32], y: &mut MatrixViewMut<f32>);

    /// Single-precision matrix-matrix multiply: `C = A*B`.
    fn sgemm_f32(&self, a: &MatrixView<f32>, b: &MatrixView<f32>, c: &mut MatrixViewMut<f32>);

    /// ReLU activation (element-wise max(0, x)).
    fn relu_f32(&self, n: usize, x: &mut [f32]);
}

// ── Default CPU backend (no SIMD intrinsics required) ────────────────────────

/// Pure-Rust CPU backend — no SIMD intrinsics, always available.
pub struct CpuBackend;

impl SimdBackend for CpuBackend {
    fn name(&self) -> &str {
        "cpu-scalar"
    }

    fn saxpy_f32(&self, n: usize, a: f32, x: &[f32], y: &mut [f32]) {
        for i in 0..n {
            y[i] = a.mul_add(x[i], y[i]);
        }
    }

    fn daxpy_f64(&self, n: usize, a: f64, x: &[f64], y: &mut [f64]) {
        for i in 0..n {
            y[i] = a.mul_add(x[i], y[i]);
        }
    }

    fn sdot_f32(&self, n: usize, x: &[f32], y: &[f32]) -> f32 {
        let mut sum = 0.0;
        for i in 0..n {
            sum = x[i].mul_add(y[i], sum);
        }
        sum
    }

    fn ddot_f64(&self, n: usize, x: &[f64], y: &[f64]) -> f64 {
        let mut sum = 0.0;
        for i in 0..n {
            sum = x[i].mul_add(y[i], sum);
        }
        sum
    }

    fn sgemv_f32(&self, a: &MatrixView<f32>, x: &[f32], y: &mut MatrixViewMut<f32>) {
        let (rows, cols) = a.shape();
        for r in 0..rows {
            let mut acc = 0.0;
            for c in 0..cols {
                acc = a[(r, c)].mul_add(x[c], acc);
            }
            y[(r, 0)] = acc;
        }
    }

    fn sgemm_f32(&self, a: &MatrixView<f32>, b: &MatrixView<f32>, c: &mut MatrixViewMut<f32>) {
        let (m, k) = a.shape();
        let (_, n) = b.shape();
        for i in 0..m {
            for j in 0..n {
                let mut acc = 0.0;
                for p in 0..k {
                    acc = a[(i, p)].mul_add(b[(p, j)], acc);
                }
                c[(i, j)] = acc;
            }
        }
    }

    fn relu_f32(&self, n: usize, x: &mut [f32]) {
        for v in &mut x[..n] {
            *v = v.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_backend_dot_product() {
        let be = CpuBackend;
        let x = [1.0f32, 2.0, 3.0];
        let y = [4.0f32, 5.0, 6.0];
        assert_eq!(be.sdot_f32(3, &x, &y), 32.0);
    }

    #[test]
    fn cpu_backend_relu() {
        let be = CpuBackend;
        let mut x = [-1.0f32, 0.0, 2.0, -3.0];
        be.relu_f32(4, &mut x);
        assert_eq!(x, [0.0, 0.0, 2.0, 0.0]);
    }

    #[test]
    fn cpu_sgemm() {
        let be = CpuBackend;
        let a_data = [1.0f32, 2.0, 3.0, 4.0]; // 2x2
        let b_data = [5.0f32, 6.0, 7.0, 8.0]; // 2x2
        let mut c_data = [0.0f32; 4];
        let av = MatrixView::new(&a_data, 2, 2);
        let bv = MatrixView::new(&b_data, 2, 2);
        let mut cv = MatrixViewMut::new(&mut c_data, 2, 2);
        be.sgemm_f32(&av, &bv, &mut cv);
        // [1 2; 3 4] * [5 6; 7 8] = [19 22; 43 50]
        assert!((cv[(0, 0)] - 19.0).abs() < 1e-6);
        assert!((cv[(1, 1)] - 50.0).abs() < 1e-6);
    }
}
