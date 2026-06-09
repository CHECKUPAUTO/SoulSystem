//! Chantier 2 : Le Noyau de Calcul Matriciel (GEMM) Vectorisé, Spécifique aux Architectures SIMD et Conscient des Caches.
//!
//! `MatrixEngine` utilise directement le `HardwareManifest` du planificateur pour segmenter géométriquement
//! les matrices en blocs de calcul (Tiling). Cette approche garantit que les données restent confinées dans les
//! caches ultra-rapides L1 et L2 du processeur, éliminant la latence de la RAM synchrone.
//! L'exécution bascule dynamiquement au runtime sur des micro-kernels en intrinsèques de bas niveau
//! (AVX-512, AVX2 ou ARM Neon) selon le silicium détecté au boot.

pub mod engine;
pub mod kernels;

pub use engine::{MatrixDescriptor, MatrixEngine};
pub use kernels::MicroKernelFn;
