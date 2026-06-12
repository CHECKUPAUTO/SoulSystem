//! SIMD-accelerated tensor operations using `scirust-simd` kernels.
//!
//! These functions dispatch to AVX2/SSE2/NEON at runtime when available,
//! falling back to scalar loops.

use crate::tensor::Tensor;

// ---------------------------------------------------------------------------
// Element-wise unary ops with SIMD acceleration
// ---------------------------------------------------------------------------

/// Element-wise: `y = x + scalar`
pub fn add_scalar_f32(t: &Tensor<f32>, scalar: f32) -> Tensor<f32> {
    let data = t.data();
    let mut out = Vec::with_capacity(data.len());
    // Use SIMD for f32 addition via the `simd_add_one` pattern
    // For generic scalar add, we use a SIMD-friendly loop
    let n = data.len();
    let mut i = 0;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        if std::arch::is_x86_feature_detected!("avx2") {
            let s = _mm256_set1_ps(scalar);
            out.reserve(n);
            // write directly
            let mut tmp = vec![0.0f32; n];
            while i + 8 <= n {
                let vx = _mm256_loadu_ps(data.as_ptr().add(i));
                let vr = _mm256_add_ps(vx, s);
                _mm256_storeu_ps(tmp.as_mut_ptr().add(i), vr);
                i += 8;
            }
            out = tmp;
        } else if std::arch::is_x86_feature_detected!("sse2") {
            let s = _mm_set1_ps(scalar);
            let mut tmp = vec![0.0f32; n];
            while i + 4 <= n {
                let vx = _mm_loadu_ps(data.as_ptr().add(i));
                let vr = _mm_add_ps(vx, s);
                _mm_storeu_ps(tmp.as_mut_ptr().add(i), vr);
                i += 4;
            }
            out = tmp;
        }
    }

    // Scalar fallback for remaining elements
    if out.len() != n {
        out = Vec::with_capacity(n);
        for &x in data {
            out.push(x + scalar);
        }
    } else {
        for j in i..n {
            out[j] = data[j] + scalar;
        }
    }

    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise: `y = x * scalar`
pub fn mul_scalar_f32(t: &Tensor<f32>, scalar: f32) -> Tensor<f32> {
    let data = t.data();
    let n = data.len();
    let mut out = vec![0.0f32; n];
    let mut i = 0;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        if std::arch::is_x86_feature_detected!("avx2") {
            let s = _mm256_set1_ps(scalar);
            while i + 8 <= n {
                let vx = _mm256_loadu_ps(data.as_ptr().add(i));
                let vr = _mm256_mul_ps(vx, s);
                _mm256_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 8;
            }
        } else if std::arch::is_x86_feature_detected!("sse2") {
            let s = _mm_set1_ps(scalar);
            while i + 4 <= n {
                let vx = _mm_loadu_ps(data.as_ptr().add(i));
                let vr = _mm_mul_ps(vx, s);
                _mm_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 4;
            }
        }
    }

    while i < n {
        out[i] = data[i] * scalar;
        i += 1;
    }
    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise ReLU: `y = max(0, x)`
pub fn relu_f32(t: &Tensor<f32>) -> Tensor<f32> {
    let data = t.data();
    let n = data.len();
    let mut out = vec![0.0f32; n];
    let mut i = 0;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        if std::arch::is_x86_feature_detected!("avx2") {
            let zero = _mm256_setzero_ps();
            while i + 8 <= n {
                let vx = _mm256_loadu_ps(data.as_ptr().add(i));
                let vr = _mm256_max_ps(vx, zero);
                _mm256_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 8;
            }
        } else if std::arch::is_x86_feature_detected!("sse2") {
            let zero = _mm_setzero_ps();
            while i + 4 <= n {
                let vx = _mm_loadu_ps(data.as_ptr().add(i));
                let vr = _mm_max_ps(vx, zero);
                _mm_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 4;
            }
        }
    }

    while i < n {
        out[i] = if data[i] > 0.0 { data[i] } else { 0.0 };
        i += 1;
    }
    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise Sigmoid: `y = 1 / (1 + exp(-x))`
pub fn sigmoid_f32(t: &Tensor<f32>) -> Tensor<f32> {
    let data = t.data();
    let out: Vec<f32> = data.iter().map(|&x| 1.0 / (1.0 + (-x).exp())).collect();
    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise Tanh: `y = tanh(x)`
pub fn tanh_f32(t: &Tensor<f32>) -> Tensor<f32> {
    let data = t.data();
    let out: Vec<f32> = data.iter().map(|&x| x.tanh()).collect();
    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise: `y = exp(x)`
pub fn exp_f32(t: &Tensor<f32>) -> Tensor<f32> {
    let data = t.data();
    let out: Vec<f32> = data.iter().map(|&x| x.exp()).collect();
    Tensor::from_vec(out, t.shape()).unwrap()
}

/// Element-wise: `y = 1.0 / (sqrt(x) + epsilon)` (Rsqrt)
pub fn rsqrt_f32(t: &Tensor<f32>, epsilon: f32) -> Tensor<f32> {
    let data = t.data();
    let out: Vec<f32> = data.iter().map(|&x| 1.0 / (x.sqrt() + epsilon)).collect();
    Tensor::from_vec(out, t.shape()).unwrap()
}

// ---------------------------------------------------------------------------
// Binary ops
// ---------------------------------------------------------------------------

/// Element-wise `c = a + b` with SIMD
pub fn add_f32(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
    assert_eq!(a.shape(), b.shape(), "Shape mismatch in add_f32");
    let da = a.data();
    let db = b.data();
    let n = da.len();
    let mut out = vec![0.0f32; n];
    let mut i = 0;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        use core::arch::x86_64::*;
        if std::arch::is_x86_feature_detected!("avx2") {
            while i + 8 <= n {
                let va = _mm256_loadu_ps(da.as_ptr().add(i));
                let vb = _mm256_loadu_ps(db.as_ptr().add(i));
                let vr = _mm256_add_ps(va, vb);
                _mm256_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 8;
            }
        } else if std::arch::is_x86_feature_detected!("sse2") {
            while i + 4 <= n {
                let va = _mm_loadu_ps(da.as_ptr().add(i));
                let vb = _mm_loadu_ps(db.as_ptr().add(i));
                let vr = _mm_add_ps(va, vb);
                _mm_storeu_ps(out.as_mut_ptr().add(i), vr);
                i += 4;
            }
        }
    }

    while i < n {
        out[i] = da[i] + db[i];
        i += 1;
    }
    Tensor::from_vec(out, a.shape()).unwrap()
}

/// Element-wise `c = a * b` with SIMD
pub fn mul_f32(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
    assert_eq!(a.shape(), b.shape(), "Shape mismatch in mul_f32");
    let da = a.data();
    let db = b.data();
    let n = da.len();
    let mut out = vec![0.0f32; n];

    scirust_simd::ops::mul_f32(da, db, &mut out);
    Tensor::from_vec(out, a.shape()).unwrap()
}

// ---------------------------------------------------------------------------
// f64 variants
// ---------------------------------------------------------------------------

pub fn add_scalar_f64(t: &Tensor<f64>, scalar: f64) -> Tensor<f64> {
    t.map(|&x| x + scalar)
}

pub fn mul_scalar_f64(t: &Tensor<f64>, scalar: f64) -> Tensor<f64> {
    t.map(|&x| x * scalar)
}

pub fn relu_f64(t: &Tensor<f64>) -> Tensor<f64> {
    t.map(|&x| if x > 0.0 { x } else { 0.0 })
}

pub fn sigmoid_f64(t: &Tensor<f64>) -> Tensor<f64> {
    t.map(|&x| 1.0 / (1.0 + (-x).exp()))
}

pub fn tanh_f64(t: &Tensor<f64>) -> Tensor<f64> {
    t.map(|&x| x.tanh())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relu() {
        let t = Tensor::<f32>::from_vec(vec![-2.0, -1.0, 0.0, 1.0, 2.0], &[5]).unwrap();
        let r = relu_f32(&t);
        assert_eq!(r.data(), &[0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_add_scalar() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0], &[3]).unwrap();
        let r = add_scalar_f32(&t, 10.0);
        assert_eq!(r.data(), &[11.0, 12.0, 13.0]);
    }

    #[test]
    fn test_binary_add() {
        let a = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0], &[3]).unwrap();
        let b = Tensor::<f32>::from_vec(vec![4.0, 5.0, 6.0], &[3]).unwrap();
        let r = add_f32(&a, &b);
        assert_eq!(r.data(), &[5.0, 7.0, 9.0]);
    }
}
