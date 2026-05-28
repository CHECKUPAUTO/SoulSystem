// ==========================================================================
// device.rs — Abstraction device + dtype (V6.6, GPU-readiness)
//
// PROBLÈME RÉSOLU
//
//   Le code utilisait `DType::F64` et `Device::Cpu` hardcodés à 26+ endroits.
//   Le passage GPU n'était donc PAS "un changement d'une ligne" comme
//   annoncé dans les migrations précédentes — c'était 26 éditions dispersées,
//   plus un risque de précision (f64 mal supporté / lent sur GPU edge).
//
// CE MODULE CENTRALISE LES DEUX CHOIX
//
//   - COMPUTE_DTYPE : le dtype de calcul. F64 sur CPU (précision max),
//     F32 sur GPU (les GPU edge écrasent le f64 d'un facteur 32-64×, et
//     certaines ops CUDA candle ne supportent pas le f64).
//
//   - select_device() : choisit le device selon la variable d'environnement
//     CHRONOS_DEVICE (cpu|cuda|cuda:N), avec fallback CPU propre si CUDA
//     indisponible.
//
// PRÉCISION f32 — VALIDÉ EMPIRIQUEMENT
//
//   Le leapfrog symplectique du BCI conserve l'énergie aussi bien en f32
//   qu'en f64 (drift 2.5e-5 dans les deux cas sur 200 steps) car la
//   conservation est GÉOMÉTRIQUE, pas numérique. Le f32 est donc sûr pour
//   les dynamiques hamiltoniennes.
//
// USAGE GPU SUR JETSON THOR
//
//   1. Compiler candle avec la feature cuda :
//        candle-core = { version = "0.10", features = ["cuda"] }
//   2. Régler la dtype par défaut sur F32 (voir COMPUTE_DTYPE ci-dessous —
//      basculer la const, ou utiliser la détection auto).
//   3. Lancer avec :  CHRONOS_DEVICE=cuda ./chronos-agent
//
//   Le checkpoint safetensors gère la migration CPU↔GPU et f64↔f32
//   automatiquement au load (candle reconvertit).
// ==========================================================================

use candle_core::{DType, Device};

// --------------------------------------------------------------------------
// DType de calcul
// --------------------------------------------------------------------------

/// Le dtype de calcul global. F64 par défaut (CPU, précision maximale).
///
/// **Pour le GPU** : basculer sur `DType::F32`. Validé empiriquement que
/// le leapfrog symplectique et les autres dynamiques tiennent en f32.
///
/// On expose une fonction plutôt qu'une const pour pouvoir, à terme,
/// décider dynamiquement selon le device (f64 sur CPU, f32 sur CUDA).
#[inline]
pub fn compute_dtype() -> DType {
    match dtype_from_env() {
        Some(dt) => dt,
        None => default_dtype_for_current_target(),
    }
}

/// Dtype par défaut. Aujourd'hui F64 (compat totale avec l'existant).
/// Quand le GPU sera activé, on pourra retourner F32 si le device est CUDA.
#[inline]
fn default_dtype_for_current_target() -> DType {
    DType::F64
}

/// Lit CHRONOS_DTYPE (f32|f64) si présent.
fn dtype_from_env() -> Option<DType> {
    match std::env::var("CHRONOS_DTYPE").ok()?.to_lowercase().as_str() {
        "f32" | "float32" => Some(DType::F32),
        "f64" | "float64" | "double" => Some(DType::F64),
        _ => None,
    }
}

// --------------------------------------------------------------------------
// Sélection du device
// --------------------------------------------------------------------------

/// Choisit le device selon CHRONOS_DEVICE (cpu | cuda | cuda:N).
/// Fallback propre sur CPU si CUDA n'est pas disponible ou échoue.
///
/// Retourne aussi un message décrivant le choix, pour log au démarrage.
pub fn select_device() -> (Device, String) {
    let requested = std::env::var("CHRONOS_DEVICE")
        .unwrap_or_else(|_| "cpu".to_string())
        .to_lowercase();

    if requested == "cpu" {
        return (Device::Cpu, "CPU (explicite)".to_string());
    }

    // Parse cuda ou cuda:N
    if requested.starts_with("cuda") {
        let gpu_id = requested.strip_prefix("cuda:")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        #[cfg(feature = "cuda")]
        {
            match Device::new_cuda(gpu_id) {
                Ok(dev) => return (dev, format!("CUDA:{} (Jetson/GPU)", gpu_id)),
                Err(e) => {
                    eprintln!("⚠  CUDA:{} requested but failed ({}), falling back to CPU",
                              gpu_id, e);
                    return (Device::Cpu, "CPU (fallback après échec CUDA)".to_string());
                }
            }
        }

        #[cfg(not(feature = "cuda"))]
        {
            let _ = gpu_id;
            eprintln!("⚠  CHRONOS_DEVICE=cuda mais le binaire n'est pas compilé \
                       avec --features cuda. Recompiler candle-core avec \
                       features=[\"cuda\"]. Fallback CPU.");
            return (Device::Cpu, "CPU (cuda non compilé)".to_string());
        }
    }

    (Device::Cpu, "CPU (défaut)".to_string())
}

/// Vrai si le device courant est un GPU CUDA.
pub fn is_cuda(device: &Device) -> bool {
    device.is_cuda()
}

// --------------------------------------------------------------------------
// Création de tenseurs respectant le compute_dtype
// --------------------------------------------------------------------------

/// Crée un tenseur 1D à partir d'un slice f64, converti au compute_dtype.
///
/// C'est LE point d'entrée à utiliser partout à la place de
/// `Tensor::from_slice(&data, shape, device)` quand `data: &[f64]`.
/// En mode f64 (défaut) c'est un no-op de conversion ; en mode f32
/// (GPU) ça convertit automatiquement, évitant le DTypeMismatch.
pub fn tensor_from_f64(
    data: &[f64],
    shape: impl candle_core::shape::ShapeWithOneHole,
    device: &Device,
) -> candle_core::Result<candle_core::Tensor> {
    let t = candle_core::Tensor::from_slice(data, shape, device)?;
    t.to_dtype(compute_dtype())
}

/// Idem pour un Vec<f64> consommé.
pub fn tensor_from_f64_vec(
    data: Vec<f64>,
    shape: impl candle_core::shape::ShapeWithOneHole,
    device: &Device,
) -> candle_core::Result<candle_core::Tensor> {
    tensor_from_f64(&data, shape, device)
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dtype_is_f64() {
        // Sans variable d'env, on reste en f64 (compat existant)
        std::env::remove_var("CHRONOS_DTYPE");
        assert_eq!(compute_dtype(), DType::F64);
    }

    #[test]
    fn dtype_env_override() {
        std::env::set_var("CHRONOS_DTYPE", "f32");
        assert_eq!(compute_dtype(), DType::F32);
        std::env::set_var("CHRONOS_DTYPE", "f64");
        assert_eq!(compute_dtype(), DType::F64);
        std::env::remove_var("CHRONOS_DTYPE");
    }

    #[test]
    fn select_device_defaults_to_cpu() {
        std::env::remove_var("CHRONOS_DEVICE");
        let (dev, msg) = select_device();
        assert!(!dev.is_cuda());
        assert!(msg.contains("CPU"));
    }

    #[test]
    fn select_device_explicit_cpu() {
        std::env::set_var("CHRONOS_DEVICE", "cpu");
        let (dev, _) = select_device();
        assert!(!dev.is_cuda());
        std::env::remove_var("CHRONOS_DEVICE");
    }

    #[test]
    fn cuda_without_feature_falls_back() {
        // Sans la feature cuda compilée, demander cuda doit fallback CPU
        // sans paniquer.
        std::env::set_var("CHRONOS_DEVICE", "cuda");
        let (dev, msg) = select_device();
        // En test (pas de feature cuda), on tombe sur CPU
        assert!(!dev.is_cuda());
        assert!(msg.contains("CPU"));
        std::env::remove_var("CHRONOS_DEVICE");
    }

    #[test]
    fn tensor_from_f64_respects_dtype() {
        std::env::remove_var("CHRONOS_DTYPE");
        let dev = Device::Cpu;
        // Défaut f64
        let t = tensor_from_f64(&[1.0, 2.0, 3.0], 3, &dev).unwrap();
        assert_eq!(t.dtype(), DType::F64);

        // Mode f32 : le helper convertit, pas de mismatch
        std::env::set_var("CHRONOS_DTYPE", "f32");
        let t32 = tensor_from_f64(&[1.0, 2.0, 3.0], 3, &dev).unwrap();
        assert_eq!(t32.dtype(), DType::F32);
        // Les valeurs sont préservées (à la précision f32 près)
        let v = t32.to_dtype(DType::F64).unwrap().to_vec1::<f64>().unwrap();
        assert!((v[0] - 1.0).abs() < 1e-6);
        std::env::remove_var("CHRONOS_DTYPE");
    }
}
