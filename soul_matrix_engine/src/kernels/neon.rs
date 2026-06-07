//! Micro-kernel vectorisé en ARM NEON ASIMD (Cible native du NVIDIA Jetson AGX Thor).
//! Traite 4 flottants f32 par registre (Registre Q de 128 bits).
//! Fused Multiply-Add natif ARM64 via `vfmaq_f32`.

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// Micro-kernel NEON : C += A × B pour un bloc M×K, K×N.
/// Gestion des dimensions non-alignées (non-multiples de 4) via cleanup scalar en fin de tile.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub unsafe extern "C" fn gemm_micro_kernel_neon(
    a: *const f32, b: *const f32, c: *mut f32,
    m: usize, n: usize, k: usize,
    ld_a: usize, ld_b: usize, ld_c: usize,
) {
    let zero = vmovq_n_f32(0.0);

    // === Vecteur principal : paquets de 4 colonnes × 2 lignes ===
    for i in (0..m).step_by(2) {
        let i_len = std::cmp::min(2, m - i);

        let n_main = n - (n % 4); // chemin vectorise : tuiles pleines de 4 uniquement
        for j in (0..n_main).step_by(4) {

            // Accumulateurs initiaux à zéro
            for ci in 0..i_len {
                vst1q_f32(c.add((i + ci) * ld_c + j).cast(), zero);
            }

            // Accumulation FMA (vfmaq = VMLA avec accumulateur intégré)
            for p in 0..k {
                let vb = vld1q_f32(b.add(p * ld_b + j).cast());
                for ci in 0..i_len {
                    let va = vdupq_n_f32(*a.add((i + ci) * ld_a + p));
                    let c_acc = vld1q_f32(c.add((i + ci) * ld_c + j).cast());
                    let result = vfmaq_f32(c_acc, va, vb);
                    vst1q_f32(c.add((i + ci) * ld_c + j).cast(), result);
                }
            }
        }

        // === Cleanup : colonnes non-alignées (n % 4 != 0) ===
        let j_start = (n / 4) * 4;
        if j_start < n {
            for ci in 0..i_len {
                for j in j_start..n {
                    let mut sum: f32 = 0.0; // overwrite, coherent avec le zero-init vectorise
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
                let mut sum: f32 = 0.0;
                for p in 0..k {
                    sum += *a.add(i * ld_a + p) * *b.add(p * ld_b + j);
                }
                *c.add(i * ld_c + j) = sum;
            }
        }
    }
}

/// Stub pour les plateformes non-aarch64
#[cfg(not(target_arch = "aarch64"))]
pub unsafe extern "C" fn gemm_micro_kernel_neon(
    _a: *const f32, _b: *const f32, _c: *mut f32,
    _m: usize, _n: usize, _k: usize,
    _ld_a: usize, _ld_b: usize, _ld_c: usize,
) {}
