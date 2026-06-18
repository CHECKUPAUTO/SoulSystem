//! Micro-kernel vectorisé en AVX-512 (Puces Intel/AMD Xeon modernes).
//! Traite 16 flottants f32 en une seule instruction de registre (ZMM).

// SIMD accumulator loops index fixed-size register tiles; the index drives
// pointer offsets, so the range-loop form is intentional here.
#![allow(clippy::needless_range_loop)]

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Micro-kernel AVX-512 FMA : C += A × B pour un bloc M×K, K×N.
/// Les vecteurs non-alignés sont gérés par des boucles de nettoyage scalaires en fin de bloc.
///
/// # Safety
/// - The running CPU must support AVX-512F/CD/VL/DQ (check
///   `is_x86_feature_detected!`).
/// - `a`, `b`, `c` must be valid for the `m×k`, `k×n`, `m×n` tiles respectively,
///   aligned for `f32`, with `c` not overlapping `a`/`b`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512cd,avx512vl,avx512dq")]
pub unsafe extern "C" fn gemm_micro_kernel_avx512(
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
    // === Vecteur principal : traitements par paquets de 16 colonnes et 4 lignes ===
    for i in (0..m).step_by(4) {
        let i_len = std::cmp::min(4, m - i);

        // Only full 16-wide column blocks: storing a 512-bit vector for a partial
        // block would write past the row. The `n % 16` tail is handled by the
        // scalar cleanup below.
        for j in (0..(n / 16) * 16).step_by(16) {
            // Chargement initial des accumulateurs C dans les registres ZMM (16×f32 chacun)
            let mut c_acc: [*const f32; 4] = [std::ptr::null(); 4];
            for ci in 0..i_len {
                c_acc[ci] = c.add((i + ci) * ld_c + j);
            }

            // Préchargement des registres ZMM initiaux (zero pour le premier passage, loadu pour les suivants — mais ici on fait C += ...)
            let _accum: [std::mem::MaybeUninit<[f32; 16]>; 4] = [
                std::mem::MaybeUninit::uninit(),
                std::mem::MaybeUninit::uninit(),
                std::mem::MaybeUninit::uninit(),
                std::mem::MaybeUninit::uninit(),
            ];
            // Initialiser à zéro via _mm512_setzero_ps (le micro-kernel fait C += A*B, pas C = A*B)
            let zero = _mm512_setzero_ps();
            for ci in 0..i_len {
                _mm512_storeu_ps(c_acc[ci].cast_mut(), zero);
            }

            // Boucle sur K — accumulation FMA
            for p in 0..k {
                let a_ptr = a.add(i * ld_a + p);
                let b_ptr = b.add(p * ld_b + j);

                // Prefetch L1d — charge le prochain bloc de B dans le cache
                if p + 8 < k {
                    _mm_prefetch(b.add((p + 8) * ld_b + j + 64) as *const i8, _MM_HINT_T0);
                }

                for ci in 0..i_len {
                    let va = _mm512_set1_ps(*a_ptr.add(ci * ld_a));
                    let vb = _mm512_loadu_ps(b_ptr.add(ci)); // stride entre éléments de B dans un tile

                    let c_vec = _mm512_loadu_ps(c_acc[ci]);
                    let result = _mm512_fmadd_ps(va, vb, c_vec);
                    _mm512_storeu_ps(c_acc[ci].cast_mut(), result);
                }
            }

            // Stockage des accumulateurs finaux
            for ci in 0..i_len {
                _mm512_storeu_ps(c_acc[ci].cast_mut(), _mm512_loadu_ps(c_acc[ci]));
            }
        }

        // === Cleanup : colonnes non-alignées (dimensions % 16 != 0) ===
        let j_start = (n / 16) * 16;
        if j_start < n {
            for ci in 0..i_len {
                for j in j_start..j_start + std::cmp::min(16, n - j_start) {
                    let mut sum: f32 = *c.add((i + ci) * ld_c + j);
                    for p in 0..k {
                        sum += *a.add((i + ci) * ld_a + p) * *b.add(p * ld_b + j);
                    }
                    *c.add((i + ci) * ld_c + j) = sum;
                }
            }
        }
    }
    // No row (`m % 4`) cleanup: the loop above uses `i_len = min(4, m - i)`, so
    // the partial final row-block is already processed; a second pass would
    // double-count those rows.
}

/// Stub pour les plateformes non-x86_64 (toujours disponible via fallback)
#[cfg(not(target_arch = "x86_64"))]
pub unsafe extern "C" fn gemm_micro_kernel_avx512(
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
    // Ne devrait jamais être appelé — le dispatcher redirige vers fallback.
}
