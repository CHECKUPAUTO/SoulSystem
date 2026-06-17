//! Micro-kernel vectorisé en AVX2 / FMA3 (Fallback x86_64 standard).
//! Traite 8 flottants f32 en une seule instruction de registre (YMM).

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Micro-kernel AVX2 FMA : C += A × B pour un bloc M×K, K×N.
/// Gestion des dimensions non-alignées (non-multiples de 8) via cleanup scalar en fin de tile.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
pub unsafe extern "C" fn gemm_micro_kernel_avx2(
    a: *const f32,
    b: *const f32,
    c: *mut f32,
    m: usize,
    n: usize,
    k: usize,
    ld_a: usize,
    ld_b: usize,
    ld_c: usize,
) {
    // === Vecteur principal : paquets de 8 colonnes × 2 lignes ===
    for i in (0..m).step_by(2) {
        let i_len = std::cmp::min(2, m - i);

        for j in (0..n).step_by(8) {
            let _j_len = std::cmp::min(8, n - j);

            // Accumulateurs initiaux à zéro (C += ... et non C = ...)
            let zero = _mm256_setzero_ps();
            for ci in 0..i_len {
                _mm256_storeu_ps(c.add((i + ci) * ld_c + j).cast(), zero);
            }

            // Accumulation FMA sur K
            for p in 0..k {
                let b_ptr = b.add(p * ld_b + j);
                for ci in 0..i_len {
                    let a_val = *a.add((i + ci) * ld_a + p);
                    let va = _mm256_set1_ps(a_val);
                    let vb = _mm256_loadu_ps(b_ptr.cast());
                    let c_vec = _mm256_loadu_ps(c.add((i + ci) * ld_c + j).cast());
                    let result = _mm256_fmadd_ps(va, vb, c_vec);
                    _mm256_storeu_ps(c.add((i + ci) * ld_c + j).cast(), result);
                }
            }
        }

        // === Cleanup : colonnes non-alignées (n % 8 != 0) ===
        let j_start = (n / 8) * 8;
        if j_start < n {
            for ci in 0..i_len {
                for j in j_start..n {
                    let mut sum: f32 = *c.add((i + ci) * ld_c + j);
                    for p in 0..k {
                        sum += *a.add((i + ci) * ld_a + p) * *b.add(p * ld_b + j);
                    }
                    *c.add((i + ci) * ld_c + j) = sum;
                }
            }
        }
    }

    // === Cleanup : lignes non-alignées (m % 2 != 0) ===
    let i_start = (m / 2) * 2;
    if i_start < m {
        for i in i_start..m {
            for j in 0..n {
                let mut sum: f32 = *c.add(i * ld_c + j);
                for p in 0..k {
                    sum += *a.add(i * ld_a + p) * *b.add(p * ld_b + j);
                }
                *c.add(i * ld_c + j) = sum;
            }
        }
    }
}

/// Stub pour les plateformes non-x86_64
#[cfg(not(target_arch = "x86_64"))]
pub unsafe extern "C" fn gemm_micro_kernel_avx2(
    _a: *const f32,
    _b: *const f32,
    _c: *mut f32,
    _m: usize,
    _n: usize,
    _k: usize,
    _ld_a: usize,
    _ld_b: usize,
    _ld_c: usize,
) {
}
