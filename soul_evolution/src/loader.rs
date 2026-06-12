//! Chargeur à chaud de code machine natif pour l'auto-évolution des agents.
//! Utilise `dlopen`/`dlsym`/`dlclose` du système POSIX pour charger des modules .so compilés
//! et les injecter dynamiquement dans le planificateur sans redémarrer le superviseur.

#[allow(unused_imports)]
use soul_scheduler::queue::Task;
#[allow(unused_imports)]
use soul_scheduler::scheduler::AgentScheduler;
use std::ffi::CString;

/// Répertoires autorisés pour le chargement de modules dynamiques.
/// Tout .so hors de ces chemins est rejeté pour éviter le chargement de code arbitraire.
const TRUSTED_MODULE_PATHS: &[&str] = &["/usr/lib/soul_system", "/opt/soul_system/modules"];

/// Chargeur de modules dynamiques — supporte le hot-swap de routines agents au runtime.
pub struct DynamicModuleLoader;

impl Default for DynamicModuleLoader {
    fn default() -> Self {
        Self
    }
}

impl DynamicModuleLoader {
    /// # Safety
    /// Le chemin du fichier doit pointer vers une bibliothèque partagée valide.
    /// Le symbole doit exister dans la bibliothèque avec la signature `extern "C" fn(*mut u8)`.
    /// La bibliothèque reste chargée en mémoire jusqu'à ce que dlclose soit appelé explicitement.
    ///
    /// # Sécurité
    /// Le chemin doit être dans un répertoire de confiance (TRUSTED_MODULE_PATHS).
    /// Seul le symbole `soul_agent_main` est résolu — pas de chargement de symboles arbitraires.
    pub unsafe fn load_agent_routine(
        library_path: &str,
    ) -> Option<(*mut libc::c_void, extern "C" fn(*mut u8))> {
        let c_path = CString::new(library_path).ok()?;

        // Validation du chemin : le .so doit être dans un répertoire de confiance
        let path = std::path::Path::new(library_path);
        let canonical = path.canonicalize().ok()?;
        let path_str = canonical.to_str()?;
        if !TRUSTED_MODULE_PATHS
            .iter()
            .any(|trusted| path_str.starts_with(trusted))
        {
            tracing::error!(
                path = %library_path,
                "Module path rejected: not in a trusted directory"
            );
            return None;
        }

        // RT_NOW : résolution immédiate de tous les symboles du binaire importé.
        let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            let err = libc::dlerror();
            let msg = if err.is_null() {
                "unknown error".to_string()
            } else {
                std::ffi::CStr::from_ptr(err)
                    .to_string_lossy()
                    .into_owned()
            };
            tracing::error!(
                path = %library_path,
                dlopen_error = %msg,
                "Failed to load native module"
            );
            return None;
        }

        // Chercher le symbole "soul_agent_main" — convention de nommage standard SoulSystem.
        let c_symbol = CString::new("soul_agent_main").ok()?;
        let symbol = libc::dlsym(handle, c_symbol.as_ptr());

        if symbol.is_null() {
            let err = libc::dlerror();
            let msg = if err.is_null() {
                "unknown error".to_string()
            } else {
                std::ffi::CStr::from_ptr(err)
                    .to_string_lossy()
                    .into_owned()
            };
            libc::dlclose(handle);
            tracing::error!(
                path = %library_path,
                dlsym_error = %msg,
                "Symbol 'soul_agent_main' not found in module"
            );
            return None;
        }

        // SAFETY: dlsym retourne un pointeur vers une fonction C.
        // On ne charge que le symbole "soul_agent_main" dont la signature est connue
        // et documentée par la convention de nommage du workspace.
        let routine: extern "C" fn(*mut u8) = std::mem::transmute(symbol);
        Some((handle, routine))
    }

    /// # Safety
    /// handle must be a valid pointer returned by `load_agent_routine`.
    pub unsafe fn unload_module(handle: *mut libc::c_void) {
        if !handle.is_null() {
            libc::dlclose(handle);
        }
    }

    /// # Safety
    /// scheduler_ptr doit pointer vers un AgentScheduler valide et non mutuellement exclu.
    pub unsafe fn hot_swap_task(
        scheduler_ptr: *mut AgentScheduler,
        core_id: usize,
        library_path: &str,
        context_ptr: *mut u8,
    ) -> bool {
        if scheduler_ptr.is_null() {
            return false;
        }

        let result = Self::load_agent_routine(library_path);
        match result {
            Some((_handle, new_routine)) => {
                let task = Task {
                    execute: new_routine,
                    context: context_ptr,
                };
                (*scheduler_ptr).submit_to(core_id, task)
            }
            None => false,
        }
    }

    /// Vérifie qu'une bibliothèque partagée est chargeable sans réellement l'ouvrir.
    pub fn can_load(library_path: &str) -> bool {
        let path = std::path::Path::new(library_path);
        if !path.exists() || !library_path.ends_with(".so") {
            return false;
        }
        // Vérifier que le chemin est dans un répertoire de confiance
        if let Ok(canonical) = path.canonicalize() {
            if let Some(path_str) = canonical.to_str() {
                return TRUSTED_MODULE_PATHS
                    .iter()
                    .any(|trusted| path_str.starts_with(trusted));
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_api_compiles() {
        // Vérifie que l'API ne panic pas sur un chemin invalide.
        let exists = DynamicModuleLoader::can_load("/non/existent/module.so");
        assert!(!exists);

        let non_so = DynamicModuleLoader::can_load("/some/path.txt");
        assert!(!non_so);
    }

    #[test]
    fn hot_swap_with_null_scheduler_returns_false() {
        unsafe {
            let result = DynamicModuleLoader::hot_swap_task(
                std::ptr::null_mut(),
                0,
                "/dev/null",
                std::ptr::null_mut(),
            );
            assert!(!result);
        }
    }
}
