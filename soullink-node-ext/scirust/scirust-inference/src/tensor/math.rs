//! Linear algebra operations on tensors.
//!
//! Provides matrix multiplication, broadcasting, reductions, etc.
//! All operations are designed for the inference engine and use
//! SIMD-accelerated kernels where possible.

use crate::tensor::Tensor;

// ---------------------------------------------------------------------------
// Matrix Multiply (GEMM)
// ---------------------------------------------------------------------------

/// Standard matrix multiplication: `C = A @ B`
///
/// `A` shape: `[M, K]`, `B` shape: `[K, N]`, result: `[M, N]`
///
/// Uses a tiled approach with SIMD inner loop for cache efficiency.
pub fn matmul_f32(a: &Tensor<f32>, b: &Tensor<f32>) -> Result<Tensor<f32>, String> {
    if a.shape().len() != 2 || b.shape().len() != 2 {
        return Err("matmul requires 2D tensors".to_string());
    }
    let m = a.shape()[0];
    let k = a.shape()[1];
    let k2 = b.shape()[0];
    let n = b.shape()[1];

    if k != k2 {
        return Err(format!(
            "matmul shape mismatch: A[{},{}] @ B[{},{}]",
            m, k, k2, n
        ));
    }

    let a_data = a.data();
    let b_data = b.data();
    let mut c = vec![0.0f32; m * n];

    // Tiled GEMM with tile size 64 for cache efficiency
    const TILE: usize = 64;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        let has_avx2 = std::arch::is_x86_feature_detected!("avx2");
        let has_sse2 = std::arch::is_x86_feature_detected!("sse2");

        for i0 in (0..m).step_by(TILE) {
            let imax = (i0 + TILE).min(m);
            for j0 in (0..n).step_by(TILE) {
                let jmax = (j0 + TILE).min(n);
                for k0 in (0..k).step_by(TILE) {
                    let kmax = (k0 + TILE).min(k);

                    for i in i0..imax {
                        for kk in k0..kmax {
                            let aik = a_data[i * k + kk];
                            let mut j = j0;

                            if has_avx2 {
                                let vaik = _mm256_set1_ps(aik);
                                while j + 8 <= jmax {
                                    let vb = _mm256_loadu_ps(b_data.as_ptr().add(kk * n + j));
                                    let vc = _mm256_loadu_ps(c.as_ptr().add(i * n + j));
                                    let vr = _mm256_fmadd_ps(vaik, vb, vc);
                                    _mm256_storeu_ps(c.as_mut_ptr().add(i * n + j), vr);
                                    j += 8;
                                }
                            } else if has_sse2 {
                                let vaik = _mm_set1_ps(aik);
                                while j + 4 <= jmax {
                                    let vb = _mm_loadu_ps(b_data.as_ptr().add(kk * n + j));
                                    let vc = _mm_loadu_ps(c.as_ptr().add(i * n + j));
                                    let vr = _mm_add_ps(_mm_mul_ps(vaik, vb), vc);
                                    _mm_storeu_ps(c.as_mut_ptr().add(i * n + j), vr);
                                    j += 4;
                                }
                            }

                            while j < jmax {
                                c[i * n + j] += aik * b_data[kk * n + j];
                                j += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        for i in 0..m {
            for kk in 0..k {
                let aik = a_data[i * k + kk];
                for j in 0..n {
                    c[i * n + j] += aik * b_data[kk * n + j];
                }
            }
        }
    }

    Tensor::from_vec(c, &[m, n])
}

/// Matrix-vector product: `y = A @ x`
pub fn matvec_f32(a: &Tensor<f32>, x: &Tensor<f32>) -> Result<Tensor<f32>, String> {
    if a.shape().len() != 2 || x.shape().len() != 1 {
        return Err("matvec requires a 2D matrix and 1D vector".to_string());
    }
    let m = a.shape()[0];
    let n = a.shape()[1];
    if x.shape()[0] != n {
        return Err(format!("matvec shape: A[{},{}] @ x[{}]", m, n, x.shape()[0]));
    }

    let a_data = a.data();
    let x_data = x.data();
    let mut y = vec![0.0f32; m];

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        if std::arch::is_x86_feature_detected!("avx2") {
            for i in 0..m {
                let mut j = 0;
                let mut sum = _mm256_setzero_ps();
                while j + 8 <= n {
                    let va = _mm256_loadu_ps(a_data.as_ptr().add(i * n + j));
                    let vx = _mm256_loadu_ps(x_data.as_ptr().add(j));
                    sum = _mm256_fmadd_ps(va, vx, sum);
                    j += 8;
                }
                // Horizontal sum
                let sum_hi = _mm256_extractf128_ps(sum, 1);
                let sum_lo = _mm256_castps256_ps128(sum);
                let sum128 = _mm_add_ps(sum_lo, sum_hi);
                let sum64 = _mm_add_ps(sum128, _mm_shuffle_ps(sum128, sum128, 0b11101110));
                let sum32 = _mm_add_ss(sum64, _mm_shuffle_ps(sum64, sum64, 0b01010101));
                y[i] = _mm_cvtss_f32(sum32);
                while j < n {
                    y[i] += a_data[i * n + j] * x_data[j];
                    j += 1;
                }
            }
            return Tensor::from_vec(y, &[m]);
        }
    }

    // Scalar fallback
    for i in 0..m {
        let row_start = i * n;
        for j in 0..n {
            y[i] += a_data[row_start + j] * x_data[j];
        }
    }
    Tensor::from_vec(y, &[m])
}

// ---------------------------------------------------------------------------
// Broadcasting
// ---------------------------------------------------------------------------

/// Broadcast a tensor to a target shape.
/// Supports standard NumPy-style broadcasting rules.
pub fn broadcast_to_f32(t: &Tensor<f32>, target: &[usize]) -> Result<Tensor<f32>, String> {
    if t.shape() == target {
        return Ok(t.clone());
    }

    // Pad shape with 1s on the left
    let mut padded = vec![1usize; target.len() - t.shape().len()];
    padded.extend_from_slice(t.shape());

    // Check broadcast compatibility
    for (p, t) in padded.iter().zip(target.iter()) {
        if *p != 1 && p != t {
            return Err(format!(
                "Cannot broadcast {:?} to {:?}",
                t, target
            ));
        }
    }

    let total: usize = target.iter().product();
    let mut out = vec![0.0f32; total];

    // For each output index, compute source index
    for flat in 0..total {
        let mut src_idx = 0usize;
        let mut src_stride = 1usize;
        let mut remaining = flat;

        for d in (0..target.len()).rev() {
            let pos = remaining % target[d];
            remaining /= target[d];

            if padded[d] > 1 {
                src_idx += pos * src_stride;
            }
            src_stride *= padded[d];
        }

        out[flat] = t.data()[src_idx];
    }

    Tensor::from_vec(out, target)
}

// ---------------------------------------------------------------------------
// Reductions
// ---------------------------------------------------------------------------

/// Sum of all elements.
pub fn sum_f32(t: &Tensor<f32>) -> f32 {
    t.data().iter().sum()
}

/// Mean of all elements.
pub fn mean_f32(t: &Tensor<f32>) -> f32 {
    let s: f32 = t.data().iter().sum();
    s / t.len() as f32
}

/// Max of all elements.
pub fn max_f32(t: &Tensor<f32>) -> f32 {
    t.data().iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

/// Min of all elements.
pub fn min_f32(t: &Tensor<f32>) -> f32 {
    t.data().iter().copied().fold(f32::INFINITY, f32::min)
}

/// Sum along a specific axis (keeps dims).
pub fn sum_axis_f32(t: &Tensor<f32>, axis: usize) -> Result<Tensor<f32>, String> {
    if axis >= t.shape().len() {
        return Err(format!("Axis {} out of range for shape {:?}", axis, t.shape()));
    }

    let mut out_shape = t.shape().to_vec();
    out_shape[axis] = 1;
    let out_len: usize = out_shape.iter().product();
    let mut out = vec![0.0f32; out_len];

    let inner = t.shape()[axis];
    let _outer = t.len() / inner;

    // Compute pre and post stride
    let pre_stride: usize = t.shape()[axis..].iter().product();
    let post_stride = pre_stride / inner;

    for i in 0..t.len() {
        let out_idx = (i / pre_stride) * post_stride + (i % post_stride);
        out[out_idx] += t.data()[i];
    }

    Tensor::from_vec(out, &out_shape)
}

/// Mean along a specific axis.
pub fn mean_axis_f32(t: &Tensor<f32>, axis: usize) -> Result<Tensor<f32>, String> {
    let mut summed = sum_axis_f32(t, axis)?;
    let axis_len = t.shape()[axis] as f32;
    let data = summed.data_mut();
    for x in data.iter_mut() {
        *x /= axis_len;
    }
    Ok(summed)
}

// ---------------------------------------------------------------------------
// Softmax
// ---------------------------------------------------------------------------

/// Softmax along the last axis (for classification).
pub fn softmax_f32(t: &Tensor<f32>) -> Tensor<f32> {
    if t.shape().is_empty() {
        return t.clone();
    }

    let last_axis = t.shape().len() - 1;
    let outer = if t.shape().len() > 1 {
        t.shape()[..last_axis].iter().product()
    } else {
        1
    };
    let inner = t.shape()[last_axis];

    let data = t.data();
    let mut out = vec![0.0f32; t.len()];

    for batch in 0..outer {
        let start = batch * inner;
        // Find max for numerical stability
        let max_val = data[start..start + inner]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);

        // Compute exp(x - max) and sum
        let mut sum = 0.0f32;
        for i in 0..inner {
            let e = (data[start + i] - max_val).exp();
            out[start + i] = e;
            sum += e;
        }

        // Normalize
        let inv_sum = 1.0 / sum;
        for i in 0..inner {
            out[start + i] *= inv_sum;
        }
    }

    Tensor::from_vec(out, t.shape()).unwrap()
}

// ---------------------------------------------------------------------------
// f64 variants
// ---------------------------------------------------------------------------

pub fn matmul_f64(a: &Tensor<f64>, b: &Tensor<f64>) -> Result<Tensor<f64>, String> {
    if a.shape().len() != 2 || b.shape().len() != 2 {
        return Err("matmul requires 2D tensors".to_string());
    }
    let m = a.shape()[0];
    let k = a.shape()[1];
    let k2 = b.shape()[0];
    let n = b.shape()[1];

    if k != k2 {
        return Err(format!(
            "matmul shape mismatch: A[{},{}] @ B[{},{}]",
            m, k, k2, n
        ));
    }

    let a_data = a.data();
    let b_data = b.data();
    let mut c = vec![0.0f64; m * n];

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        let has_avx2 = std::arch::is_x86_feature_detected!("avx2");

        for i in 0..m {
            for kk in 0..k {
                let aik = a_data[i * k + kk];
                let mut j = 0;

                if has_avx2 {
                    let vaik = _mm256_set1_pd(aik);
                    while j + 4 <= n {
                        let vb = _mm256_loadu_pd(b_data.as_ptr().add(kk * n + j));
                        let vc = _mm256_loadu_pd(c.as_ptr().add(i * n + j));
                        let vr = _mm256_fmadd_pd(vaik, vb, vc);
                        _mm256_storeu_pd(c.as_mut_ptr().add(i * n + j), vr);
                        j += 4;
                    }
                }

                while j < n {
                    c[i * n + j] += aik * b_data[kk * n + j];
                    j += 1;
                }
            }
        }
    }

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        for i in 0..m {
            for kk in 0..k {
                let aik = a_data[i * k + kk];
                for j in 0..n {
                    c[i * n + j] += aik * b_data[kk * n + j];
                }
            }
        }
    }

    Tensor::from_vec(c, &[m, n])
}

pub fn softmax_f64(t: &Tensor<f64>) -> Tensor<f64> {
    if t.shape().is_empty() {
        return t.clone();
    }
    let last_axis = t.shape().len() - 1;
    let outer = if t.shape().len() > 1 {
        t.shape()[..last_axis].iter().product()
    } else {
        1
    };
    let inner = t.shape()[last_axis];
    let data = t.data();
    let mut out = vec![0.0f64; t.len()];

    for batch in 0..outer {
        let start = batch * inner;
        let max_val = data[start..start + inner]
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let mut sum = 0.0f64;
        for i in 0..inner {
            let e = (data[start + i] - max_val).exp();
            out[start + i] = e;
            sum += e;
        }
        let inv_sum = 1.0 / sum;
        for i in 0..inner {
            out[start + i] *= inv_sum;
        }
    }
    Tensor::from_vec(out, t.shape()).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matmul_2x2() {
        let a = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let b = Tensor::<f32>::from_vec(vec![5.0, 6.0, 7.0, 8.0], &[2, 2]).unwrap();
        let c = matmul_f32(&a, &b).unwrap();
        // [1*5+2*7, 1*6+2*8] = [19, 22]
        // [3*5+4*7, 3*6+4*8] = [43, 50]
        assert_eq!(*c.get_2d(0, 0).unwrap(), 19.0);
        assert_eq!(*c.get_2d(0, 1).unwrap(), 22.0);
        assert_eq!(*c.get_2d(1, 0).unwrap(), 43.0);
        assert_eq!(*c.get_2d(1, 1).unwrap(), 50.0);
    }

    #[test]
    fn test_matmul_rectangular() {
        let a = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let b = Tensor::<f32>::from_vec(vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0], &[3, 2]).unwrap();
        let c = matmul_f32(&a, &b).unwrap();
        assert_eq!(c.shape(), &[2, 2]);
    }

    #[test]
    fn test_matmul_error() {
        let a = Tensor::<f32>::new(&[2, 3]);
        let b = Tensor::<f32>::new(&[4, 5]);
        assert!(matmul_f32(&a, &b).is_err());
    }

    #[test]
    fn test_softmax() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0], &[3]).unwrap();
        let s = softmax_f32(&t);
        let sum: f32 = s.data().iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_broadcast() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0], &[3]).unwrap();
        let b = broadcast_to_f32(&t, &[2, 3]).unwrap();
        assert_eq!(b.shape(), &[2, 3]);
        for i in 0..3 {
            assert_eq!(*b.get_2d(0, i).unwrap(), t.data()[i]);
            assert_eq!(*b.get_2d(1, i).unwrap(), t.data()[i]);
        }
    }

    #[test]
    fn test_sum_axis() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let s = sum_axis_f32(&t, 0).unwrap();
        assert_eq!(s.shape(), &[1, 2]);
    }
}
