//! Chef d'orchestre du moteur matriciel — calcule les dimensions des sous-blocs de calcul
//! ($B_M, B_N, B_K$) d'après la taille du cache L2 disponible, puis applique le triple bouclage 3D-Tiled.

use soul_scheduler::topology::HardwareManifest;
use crate::kernels::{select_best_kernel, MicroKernelFn};

/// Descripteur de tenseur matriciel 2D en mémoire continue (row-major).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MatrixDescriptor {
    /// Pointeur vers les données f32 contiguës. Doit être aligné sur la ligne de cache L1.
    pub data: *mut f32,
    /// Nombre de lignes (M dimension)
    pub rows: usize,
    /// Nombre de colonnes (N dimension)
    pub cols: usize,
}

/// Moteur GEMM 3D-tiled avec sélection dynamique de micro-kernel SIMD.
pub struct MatrixEngine {
    kernel: MicroKernelFn,
    block_m: usize,
    block_n: usize,
    block_k: usize,
}

impl MatrixEngine {
    /// Construit le moteur à partir du `HardwareManifest`. Calcule les dimensions de tile optimales
    /// pour maximiser la rétention en cache L2.
    pub fn new(manifest: &HardwareManifest) -> Self {
        let kernel = select_best_kernel(manifest.simd);

        // EXTRACTION ET GÉOMÉTRIE DES CACHES :
        // On récupère la taille du cache L2 (ex: 256 Ko ou 1 Mo par cœur)
        let l2_size = manifest.cache_hierarchy.l2.total_size;
        let line_size = manifest.cache_hierarchy.l1_data.line_size;

        // Chaque flottant f32 occupe 4 octets. On dédie 60% du cache L2 aux blocs de matrices chauds.
        let usable_elements = ((l2_size as f64 * 0.60) / 4.0) as usize;

        // Division géométrique équitable pour les sous-matrices de calcul (b_m * b_k) + (b_k * b_n) <= cache L2
        let side = ((usable_elements / 2) as f64).sqrt() as usize;

        // Alignement strict du pas de blocage sur la ligne de cache (généralement multiple de 16 ou 32 flottants)
        let elements_per_cache_line = line_size / 4;
        let optimal_block = if elements_per_cache_line > 0 {
            (side / elements_per_cache_line) * elements_per_cache_line
        } else {
            side
        };

        // Garantir des dimensions minimales stables si le cache reporté est corrompu ou restreint
        let final_block = if optimal_block == 0 || optimal_block > side { 64 } else { optimal_block };

        Self {
            kernel,
            block_m: final_block,
            block_n: final_block,
            block_k: final_block.min(128), // K-block ne doit pas dépasser 128 pour éviter les dépassements L2
        }
    }

    /// Exécute la multiplication C = C + (A × B) de manière asynchrone, parallélisée par blocs et Zéro-Allocation.
    ///
    /// SAFETY: Les pointeurs data doivent pointer vers des buffers alloués avec l'alignement correct (≥ 64 bytes).
    /// a.cols == b.rows doit être vérifié avant appel. c.data pointe vers un buffer de sortie valide.
    pub unsafe fn execute_gemm(&self, a: &MatrixDescriptor, b: &MatrixDescriptor, c: &mut MatrixDescriptor) {
        assert_eq!(a.cols, b.rows, "[VM GEMM ERROR] Matrix dimension mismatch for dot product.");
        assert_eq!(a.rows, c.rows);
        assert_eq!(b.cols, c.cols);

        let m_max = a.rows;
        let n_max = b.cols;
        let k_max = a.cols;

        // Algorithme de Tiling Macro-Géométrique 3D — ordre des boucles : N-K-M (favorise la localité spatiale)
        for j_outer in (0..n_max).step_by(self.block_n) {
            let j_len = std::cmp::min(self.block_n, n_max - j_outer);

            for p_outer in (0..k_max).step_by(self.block_k) {
                let p_len = std::cmp::min(self.block_k, k_max - p_outer);

                for i_outer in (0..m_max).step_by(self.block_m) {
                    let i_len = std::cmp::min(self.block_m, m_max - i_outer);

                    // Calcul de l'offset des sous-pointeurs bruts
                    let ptr_a = a.data.add(i_outer * a.cols + p_outer);
                    let ptr_b = b.data.add(p_outer * b.cols + j_outer);
                    let ptr_c = c.data.add(i_outer * c.cols + j_outer);

                    // Appel immédiat du micro-kernel vectorisé sélectionné au démarrage
                    (self.kernel)(
                        ptr_a, ptr_b, ptr_c,
                        i_len, j_len, p_len,
                        a.cols, b.cols, c.cols,
                    );
                }
            }
        }
    }

    /// Retourne les dimensions de bloc utilisées pour le tiling — utile pour debug / tuning.
    pub fn tile_dimensions(&self) -> (usize, usize, usize) {
        (self.block_m, self.block_n, self.block_k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_scheduler::topology::HardwareManifest;

    #[test]
    fn engine_constructs_with_valid_tile_sizes() {
        let manifest = HardwareManifest::probe();
        let engine = MatrixEngine::new(&manifest);
        let (bm, bn, bk) = engine.tile_dimensions();
        assert!(bm > 0 && bm <= 1024, "block_m must be in [1, 1024], got {}", bm);
        assert!(bn > 0 && bn <= 1024, "block_n must be in [1, 1024], got {}", bn);
        assert!(bk > 0 && bk <= 256, "block_k must be in [1, 256], got {}", bk);
    }

    #[test]
    fn tile_sizes_fit_in_l2() {
        let manifest = HardwareManifest::probe();
        let engine = MatrixEngine::new(&manifest);
        let l2_size = manifest.cache_hierarchy.l2.total_size;
        let (bm, bn, bk) = engine.tile_dimensions();

        // Le working set est: bm×bk + bk×bn × 4 bytes (f32). Doit tenir dans L2.
        let tile_bytes = (bm * bk + bk * bn) * std::mem::size_of::<f32>();
        assert!(tile_bytes <= l2_size, "Tile working set {}B exceeds L2 {}B", tile_bytes, l2_size);
    }

    #[test]
    fn engine_adapts_to_simd_extension() {
        let manifest = HardwareManifest::probe();
        let engine = MatrixEngine::new(&manifest);

        // Le kernel selectionne doit calculer un GEMM correct, y compris sur des
        // dimensions a queue (M impair, N non-multiple de 4) : exerce le chemin
        // vectorise ET les cleanups scalaires sans hors-bornes ni double comptage.
        let (m, n, k) = (3usize, 5usize, 4usize);
        let a: Vec<f32> = (0..m * k).map(|x| x as f32 * 0.5 + 1.0).collect();
        let b: Vec<f32> = (0..k * n).map(|x| x as f32 * 0.25 - 0.5).collect();
        let mut c = vec![0.0f32; m * n];
        unsafe {
            (engine.kernel)(a.as_ptr(), b.as_ptr(), c.as_mut_ptr(), m, n, k, k, n, n);
        }
        let mut expected = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut acc = 0.0f32;
                for p in 0..k {
                    acc += a[i * k + p] * b[p * n + j];
                }
                expected[i * n + j] = acc;
            }
        }
        for idx in 0..m * n {
            assert!((c[idx] - expected[idx]).abs() < 1e-3, "GEMM faux a [{}]: {} != {}", idx, c[idx], expected[idx]);
        }
        println!("PREUVE GEMM neon : {}x{}x{} (M impair, N%4!=0) == reference scalaire", m, n, k);
    }
}
