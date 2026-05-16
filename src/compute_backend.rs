//! Abstraction de backend de calcul (CPU / GPU).
//!
//! Fournit un trait `ComputeBackend` et deux implémentations :
//! - `CpuFallback` : toujours disponible, exécution sur CPU.
//! - `CudaBackend` : disponible si `CUDA_VISIBLE_DEVICES` est défini.
//!
//! La sélection se fait via la feature `gpu` ou la fonction `get_backend()`.

/// Trait abstrait pour un backend de calcul.
pub trait ComputeBackend: Send + Sync {
    /// Indique si le backend est disponible.
    fn is_available(&self) -> bool;

    /// Exécute un noyau de calcul sur des données.
    ///
    /// `kernel` : paramètres du noyau (vecteur de f32).
    /// `data`   : données d'entrée.
    ///
    /// Retourne un vecteur de même taille que `data`.
    fn execute_kernel(
        &self,
        kernel: &[f32],
        data: &[f32],
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>>;
}

/// Backend CPU toujours disponible.
pub struct CpuFallback;

impl ComputeBackend for CpuFallback {
    fn is_available(&self) -> bool {
        true
    }

    fn execute_kernel(
        &self,
        _kernel: &[f32],
        data: &[f32],
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        // Convolution simpliste : passe-passe identité
        // Dans une vraie implémentation, on ferait une convolution 1D
        Ok(data.to_vec())
    }
}

/// Backend CUDA (stub).
pub struct CudaBackend;

impl ComputeBackend for CudaBackend {
    fn is_available(&self) -> bool {
        std::env::var("CUDA_VISIBLE_DEVICES").is_ok()
    }

    fn execute_kernel(
        &self,
        _kernel: &[f32],
        data: &[f32],
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        // Stub : retourne les données telles quelles
        Ok(data.to_vec())
    }
}

/// Retourne le backend approprié selon la configuration.
///
/// Si la feature `gpu` est activée, tente d'utiliser CUDA.
/// Sinon, utilise le fallback CPU.
pub fn get_backend() -> Box<dyn ComputeBackend> {
    if cfg!(feature = "gpu") {
        Box::new(CudaBackend)
    } else {
        Box::new(CpuFallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_fallback_available() {
        let backend = CpuFallback;
        assert!(backend.is_available());
    }

    #[test]
    fn test_cpu_fallback_execute() {
        let backend = CpuFallback;
        let result = backend
            .execute_kernel(&[1.0, 2.0], &[3.0, 4.0, 5.0])
            .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_get_backend_default() {
        let backend = get_backend();
        assert!(backend.is_available());
    }
}
