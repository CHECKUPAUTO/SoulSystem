//! Sous-module de micro-kernels GEMM — signature brute (pointeurs alignés, zéro allocation)
//! et sélection dynamique au runtime selon le matériel détecté.

use soul_scheduler::topology::VectorExtension;

/// Signature universelle pour un micro-kernel de multiplication de blocs matriciels.
/// Opération : `C[i][j] += A[i][p] * B[p][j]` pour i∈[0,m), p∈[0,k), j∈[0,n).
/// Les pointeurs doivent être alignés sur les frontières de cache (64/128 octets).
pub type MicroKernelFn = unsafe extern "C" fn(
    a: *const f32, // Bloc de la matrice A
    b: *const f32, // Bloc de la matrice B
    c: *mut f32,   // Bloc de la matrice de destination C (accumulation)
    m: usize,      // Hauteur du bloc A / C
    n: usize,      // Largeur du bloc B / C
    k: usize,      // Profondeur commune (colonnes A / lignes B)
    ld_a: usize,   // Leading dimension (largeur réelle en mémoire) de A
    ld_b: usize,   // Leading dimension (largeur réelle en mémoire) de B
    ld_c: usize,   // Leading dimension (largeur réelle en mémoire) de C
);

// --- Modules de kernels par plateforme ---

#[cfg(target_arch = "x86_64")]
pub mod avx2;
#[cfg(target_arch = "x86_64")]
pub mod avx512;
pub mod fallback;
#[cfg(target_arch = "aarch64")]
pub mod neon;

/// Résout dynamiquement au runtime le pointeur de fonction vers le meilleur micro-kernel disponible.
/// Retourne toujours une fonction valide — sur plateforme non supportée, fallback scalar est utilisé.
pub fn select_best_kernel(simd_extension: VectorExtension) -> MicroKernelFn {
    match simd_extension {
        VectorExtension::Avx512 => {
            #[cfg(target_arch = "x86_64")]
            {
                avx512::gemm_micro_kernel_avx512
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                fallback::gemm_micro_kernel_fallback
            }
        }
        VectorExtension::Avx2 => {
            #[cfg(target_arch = "x86_64")]
            {
                avx2::gemm_micro_kernel_avx2
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                fallback::gemm_micro_kernel_fallback
            }
        }
        VectorExtension::Neon => {
            #[cfg(target_arch = "aarch64")]
            {
                neon::gemm_micro_kernel_neon
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                fallback::gemm_micro_kernel_fallback
            }
        }
        VectorExtension::None => fallback::gemm_micro_kernel_fallback,
    }
}
