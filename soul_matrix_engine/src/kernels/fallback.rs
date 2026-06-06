//! Algorithme scalaire générique avec déroulement de boucle agressif (loop unrolling x8).
//! Utilisé si aucune extension SIMD n'est disponible ou en cas d'incompatibilité de plateforme.

/// Micro-kernel fallback scalar : C += A × B, avec auto-vectorisation par le compilateur.
pub unsafe extern "C" fn gemm_micro_kernel_fallback(
    a: *const f32, b: *const f32, c: *mut f32,
    m: usize, n: usize, k: usize,
    ld_a: usize, ld_b: usize, ld_c: usize,
) {
    // Déroulement de boucle x8 sur la dimension K pour le prefetch hardware.
    for i in 0..m {
        let a_row = a.add(i * ld_a);
        let c_row = c.add(i * ld_c);

        for p in (0..k).step_by(8) {
            let rem_k = std::cmp::min(8, k - p);
            for j in 0..n {
                // Inner loop: accumule A[i][p] * B[p][j] + C[i][j]
                // Le compilateur x86_64 auto-vectorise cette boucle en SIMD.
                let mut acc = *c_row.add(j);
                for pk in 0..rem_k {
                    acc += *a_row.add(p + pk) * *b.add((p + pk) * ld_b + j);
                }
                *c_row.add(j) = acc;
            }
        }

        // Cleanup K non-aligné
        let k_start = (k / 8) * 8;
        if k_start < k {
            let c_row = c.add(i * ld_c);
            for j in 0..n {
                let mut acc = *c_row.add(j);
                for pk in k_start..k {
                    acc += *a_row.add(pk) * *b.add(pk * ld_b + j);
                }
                *c_row.add(j) = acc;
            }
        }
    }
}
