//! Chargeur à chaud de code machine natif pour l'auto-évolution des agents.
//! Utilise `dlopen`/`dlsym`/`dlclose` du système POSIX pour charger des modules .so compilés
//! et les injecter dynamiquement dans le planificateur sans redémarrer le superviseur.

use std::ffi::CString;
use soul_scheduler::queue::Task;
use soul_scheduler::scheduler::AgentScheduler;

/// Chargeur de modules dynamiques — supporte le hot-swap de routines agents au runtime.
pub struct DynamicModuleLoader;

impl DynamicModuleLoader {
    /// Charge un module compilé (.so) et extrait le point d'entrée d'exécution d'agent.
    ///
    /// SAFETY: Le chemin du fichier doit pointer vers une bibliothèque partagée valide.
    /// Le symbole doit exister dans la bibliothèque avec la signature `extern "C" fn(*mut u8)`.
    /// La bibliothèque reste chargée en mémoire jusqu'à ce que dlclose soit appelé explicitement.
    pub unsafe fn load_agent_routine(library_path: &str) -> Option<(*mut libc::c_void, extern "C" fn(*mut u8))> {
        let c_path = CString::new(library_path).ok()?;

        // RT_NOW : résolution immédiate de tous les symboles du binaire importé.
        let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW);
        if handle.is_null() {
            eprintln!(
                "[EVOLUTION ERROR] Failed to load native binary at: {}",
                library_path
            );
            return None;
        }

        // Chercher le symbole "soul_agent_main" — convention de nommage standard SoulSystem.
        let c_symbol = CString::new("soul_agent_main").ok()?;
        let symbol = libc::dlsym(handle, c_symbol.as_ptr());

        if symbol.is_null() {
            libc::dlclose(handle);
            eprintln!(
                "[EVOLUTION ERROR] Symbol 'soul_agent_main' not found in {}",
                library_path
            );
            return None;
        }

        // Transmutation sûre : la bibliothèque exporte exactement le type attendu.
        let routine: extern "C" fn(*mut u8) = std::mem::transmute(symbol);
        Some((handle, routine))
    }

    /// Libère un module précédemment chargé via `load_agent_routine`.
    pub unsafe fn unload_module(handle: *mut libc::c_void) {
        if !handle.is_null() {
            libc::dlclose(handle);
        }
    }

    /// Injecte dynamiquement un comportement auto-généré dans le planificateur de tâches courant.
    /// Charge le .so, extrait la routine, crée une Task et l'envoie au core spécifié.
    ///
    /// SAFETY: scheduler_ptr doit pointer vers un AgentScheduler valide et non mutuellement exclu.
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
        std::path::Path::new(library_path).exists() && library_path.ends_with(".so")
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
