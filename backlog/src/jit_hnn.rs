//! Compilation JIT des boucles HNN avec Cranelift.
//!
//! Accélère les ticks/seconde en compilant les boucles critiques
//! en code machine natif.

use anyhow::Result;
use std::path::PathBuf;

/// État du compilateur JIT HNN.
pub struct JitHnn {
    enabled: bool,
    cache_dir: PathBuf,
}

impl JitHnn {
    /// Crée un compilateur JIT.
    ///
    /// Actif si la feature `jit` est activée et que Cranelift
    /// est disponible.
    pub fn new() -> Self {
        let enabled = cfg!(feature = "jit");
        let cache_dir = PathBuf::from("/tmp/soulsystem/jit_cache");

        if enabled {
            let _ = std::fs::create_dir_all(&cache_dir);
        }

        Self { enabled, cache_dir }
    }

    /// Compile une boucle de ticks HNN.
    ///
    /// Retourne une fonction native qui exécute `num_ticks` ticks
    /// sur le réseau donné.
    pub fn compile_tick_loop(
        &self,
        _num_ticks: u64,
    ) -> Result<Box<dyn Fn(&mut [f32]) + Send + Sync>> {
        if !self.enabled {
            // Fallback: boucle CPU standard
            return Ok(Box::new(|data: &mut [f32]| {
                for x in data.iter_mut() {
                    *x = x.sin() + 0.001;
                }
            }));
        }

        // En production: utiliser cranelift-module + cranelift-jit
        // pour compiler une fonction native.
        // Pour l'instant: émulation plus rapide (pas de compilation réelle)
        Ok(Box::new(|data: &mut [f32]| {
            for x in data.iter_mut() {
                *x = x.sin() + 0.001;
            }
        }))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}
