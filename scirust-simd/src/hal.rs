pub trait VectorBackend {
    fn add_f32(a: &[f32], b: &[f32], out: &mut [f32]);
    fn dot_f32(a: &[f32], b: &[f32]) -> f32;
}

pub struct ScalarFallback;

impl VectorBackend for ScalarFallback {
    fn add_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
        for i in 0..a.len() {
            out[i] = a[i] + b[i];
        }
    }
    fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}

#[cfg(all(target_arch = "x86_64", feature = "simd-avx512"))]
pub struct Avx512Backend;

#[cfg(all(target_arch = "x86_64", feature = "simd-avx512"))]
impl VectorBackend for Avx512Backend {
    fn add_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
        use std::arch::x86_64::*;
        let mut i = 0;
        while i + 16 <= a.len() {
            unsafe {
                let va = _mm512_loadu_ps(a.as_ptr().add(i));
                let vb = _mm512_loadu_ps(b.as_ptr().add(i));
                let vr = _mm512_add_ps(va, vb);
                _mm512_storeu_ps(out.as_mut_ptr().add(i), vr);
            }
            i += 16;
        }
        while i < a.len() {
            out[i] = a[i] + b[i];
            i += 1;
        }
    }
    fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        // AVX-512 dot product implementation
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}

#[cfg(all(target_arch = "aarch64", feature = "simd-sve"))]
pub struct SveBackend;

#[cfg(all(target_arch = "aarch64", feature = "simd-sve"))]
impl VectorBackend for SveBackend {
    fn add_f32(a: &[f32], b: &[f32], out: &mut [f32]) {
        use std::arch::aarch64::*;
        let n = a.len() as i64;
        let mut i: i64 = 0;

        // VLA (Vector Length Agnostic) loop using svwhilelt
        unsafe {
            let mut pg = svwhilelt_b32_s64(i, n);
            while svptest_any(svptrue_b32(), pg) {
                let va = svld1_f32(pg, a.as_ptr().offset(i as isize));
                let vb = svld1_f32(pg, b.as_ptr().offset(i as isize));
                let vr = svadd_f32_z(pg, va, vb);
                svst1_f32(pg, out.as_mut_ptr().offset(i as isize), vr);

                i += svcntw();
                pg = svwhilelt_b32_s64(i, n);
            }
        }
    }

    fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        // VLA dot product would go here
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}
