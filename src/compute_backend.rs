//! Abstraction de backend de calcul (CPU / GPU).
//!
//! Fournit un trait `ComputeBackend` et quatre implémentations :
//! - `CpuFallback` : convolution 1D, produit scalaire, matvec réelles.
//! - `CudaBackend`  : détection via `nvidia-smi`.
//! - `RocmBackend`  : détection via `rocm-smi`.
//! - `VulkanBackend`: détection via `vulkaninfo`.
//!
//! `get_backend()` essaie CUDA → ROCm → Vulkan → CPU.

use std::fmt;

/// Trait abstrait pour un backend de calcul.
pub trait ComputeBackend: Send + Sync + fmt::Debug {
    /// Indique si le backend est disponible.
    fn is_available(&self) -> bool;

    /// Exécute une convolution 1D (cross-correlation) de `kernel` sur `data`.
    fn convolve_1d(&self, kernel: &[f32], data: &[f32]) -> Vec<f32>;

    /// Produit scalaire entre deux vecteurs.
    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32;

    /// Multiplication matrice × vecteur : `matrix` (rows × cols) × `vector` (cols).
    fn matvec(&self, matrix: &[f32], rows: usize, cols: usize, vector: &[f32]) -> Vec<f32>;
}

// ── CpuFallback ──────────────────────────────────────────────────────────────

/// Backend CPU — implémentations réelles, pas de stubs.
#[derive(Debug)]
pub struct CpuFallback;

impl ComputeBackend for CpuFallback {
    fn is_available(&self) -> bool {
        true
    }

    fn convolve_1d(&self, kernel: &[f32], data: &[f32]) -> Vec<f32> {
        if kernel.is_empty() || data.is_empty() {
            return Vec::new();
        }
        let klen = kernel.len();
        let dlen = data.len();
        let out_len = dlen + klen - 1;
        let mut result = vec![0.0f32; out_len];
        for (i, res) in result.iter_mut().enumerate() {
            let mut sum = 0.0;
            let start = (i + 1).saturating_sub(klen);
            let end = i.min(dlen - 1);
            for (j, &d) in data.iter().enumerate().take(end + 1).skip(start) {
                let k_idx = i - j;
                if k_idx < klen {
                    sum += d * kernel[k_idx];
                }
            }
            *res = sum;
        }
        result
    }

    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    fn matvec(&self, matrix: &[f32], rows: usize, cols: usize, vector: &[f32]) -> Vec<f32> {
        let mut result = vec![0.0f32; rows];
        for r in 0..rows {
            let mut sum = 0.0;
            for c in 0..cols {
                sum += matrix[r * cols + c] * vector[c];
            }
            result[r] = sum;
        }
        result
    }
}

// ── GPU detection helpers ────────────────────────────────────────────────────

fn nvidia_smi_available() -> bool {
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn rocm_smi_available() -> bool {
    std::process::Command::new("rocm-smi")
        .arg("--showproductname")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn vulkaninfo_available() -> bool {
    std::process::Command::new("vulkaninfo")
        .arg("--summary")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── CudaBackend ──────────────────────────────────────────────────────────────

/// Backend CUDA — détection via `nvidia-smi`.
#[derive(Debug)]
pub struct CudaBackend;

impl ComputeBackend for CudaBackend {
    fn is_available(&self) -> bool {
        nvidia_smi_available()
    }

    fn convolve_1d(&self, kernel: &[f32], data: &[f32]) -> Vec<f32> {
        CpuFallback.convolve_1d(kernel, data)
    }

    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32 {
        CpuFallback.dot_product(a, b)
    }

    fn matvec(&self, matrix: &[f32], rows: usize, cols: usize, vector: &[f32]) -> Vec<f32> {
        CpuFallback.matvec(matrix, rows, cols, vector)
    }
}

// ── RocmBackend ──────────────────────────────────────────────────────────────

/// Backend ROCm — détection via `rocm-smi`.
#[derive(Debug)]
pub struct RocmBackend;

impl ComputeBackend for RocmBackend {
    fn is_available(&self) -> bool {
        rocm_smi_available()
    }

    fn convolve_1d(&self, kernel: &[f32], data: &[f32]) -> Vec<f32> {
        CpuFallback.convolve_1d(kernel, data)
    }

    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32 {
        CpuFallback.dot_product(a, b)
    }

    fn matvec(&self, matrix: &[f32], rows: usize, cols: usize, vector: &[f32]) -> Vec<f32> {
        CpuFallback.matvec(matrix, rows, cols, vector)
    }
}

// ── VulkanBackend ────────────────────────────────────────────────────────────

/// Backend Vulkan — détection via `vulkaninfo`.
#[derive(Debug)]
pub struct VulkanBackend;

impl ComputeBackend for VulkanBackend {
    fn is_available(&self) -> bool {
        vulkaninfo_available()
    }

    fn convolve_1d(&self, kernel: &[f32], data: &[f32]) -> Vec<f32> {
        CpuFallback.convolve_1d(kernel, data)
    }

    fn dot_product(&self, a: &[f32], b: &[f32]) -> f32 {
        CpuFallback.dot_product(a, b)
    }

    fn matvec(&self, matrix: &[f32], rows: usize, cols: usize, vector: &[f32]) -> Vec<f32> {
        CpuFallback.matvec(matrix, rows, cols, vector)
    }
}

/// Sélectionne le backend disponible avec priorité : CUDA → ROCm → Vulkan → CPU.
pub fn get_backend() -> Box<dyn ComputeBackend> {
    if CudaBackend.is_available() {
        Box::new(CudaBackend)
    } else if RocmBackend.is_available() {
        Box::new(RocmBackend)
    } else if VulkanBackend.is_available() {
        Box::new(VulkanBackend)
    } else {
        Box::new(CpuFallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_convolution_identity() {
        let cpu = CpuFallback;
        let result = cpu.convolve_1d(&[1.0], &[3.0, 4.0, 5.0]);
        assert_eq!(result, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_cpu_convolution_blur() {
        let cpu = CpuFallback;
        // kernel = [0.25, 0.5, 0.25] — cross-correlation
        let kernel = [0.25, 0.5, 0.25];
        let data = [1.0, 2.0, 3.0, 4.0];
        let result = cpu.convolve_1d(&kernel, &data);
        // result len = 4 + 3 - 1 = 6
        assert_eq!(result.len(), 6);
        // Vérification manuelle des valeurs de cross-correlation :
        // result[0] = 1*0.25 = 0.25
        // result[1] = 1*0.5 + 2*0.25 = 1.0
        // result[3] = 2*0.25 + 3*0.5 + 4*0.25 = 3.0
        assert!((result[0] - 0.25).abs() < 1e-6);
        assert!((result[1] - 1.0).abs() < 1e-6);
        assert!((result[3] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_cpu_dot_product() {
        let cpu = CpuFallback;
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let dot = cpu.dot_product(&a, &b);
        assert!((dot - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_cpu_matvec() {
        let cpu = CpuFallback;
        // matrix: [1 2; 3 4; 5 6]  (3×2)
        let matrix = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let vector = vec![7.0, 8.0];
        let result = cpu.matvec(&matrix, 3, 2, &vector);
        assert_eq!(result.len(), 3);
        assert!((result[0] - 23.0).abs() < 1e-6); // 1*7 + 2*8
        assert!((result[1] - 53.0).abs() < 1e-6); // 3*7 + 4*8
        assert!((result[2] - 83.0).abs() < 1e-6); // 5*7 + 6*8
    }

    #[test]
    fn test_backend_selection_returns_something() {
        let backend = get_backend();
        // Must always be available (CPU fallback at minimum)
        assert!(backend.is_available());
    }

    #[test]
    fn test_cpu_empty_convolution() {
        let cpu = CpuFallback;
        assert!(cpu.convolve_1d(&[], &[1.0, 2.0]).is_empty());
        assert!(cpu.convolve_1d(&[1.0], &[]).is_empty());
    }

    #[test]
    fn test_cpu_zero_dot_product() {
        let cpu = CpuFallback;
        assert!((cpu.dot_product(&[], &[]) - 0.0).abs() < 1e-6);
    }
}
