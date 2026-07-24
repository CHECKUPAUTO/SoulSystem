//! Chargeur à chaud de code machine natif pour l'auto-évolution des agents.
//! Utilise le chargeur natif de chaque plateforme pour charger des modules
//! `.so`, `.dylib` ou `.dll` et les injecter dynamiquement dans le
//! planificateur sans redémarrer le superviseur.

#[allow(unused_imports)]
use soul_scheduler::queue::Task;
#[allow(unused_imports)]
use soul_scheduler::scheduler::AgentScheduler;
/// Répertoires autorisés pour le chargement de modules dynamiques.
/// Toute bibliothèque hors de ces chemins est rejetée pour éviter le
/// chargement de code arbitraire.
#[cfg(target_os = "linux")]
const TRUSTED_MODULE_PATHS: &[&str] = &["/usr/lib/soul_system", "/opt/soul_system/modules"];
#[cfg(target_os = "macos")]
const TRUSTED_MODULE_PATHS: &[&str] = &[
    "/Library/Application Support/SoulSystem/modules",
    "/usr/local/lib/soul_system",
];
#[cfg(windows)]
const TRUSTED_MODULE_PATHS: &[&str] = &[
    r"C:\Program Files\SoulSystem\modules",
    r"C:\ProgramData\SoulSystem\modules",
];
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
const TRUSTED_MODULE_PATHS: &[&str] = &[];

fn is_trusted_module_path(path: &std::path::Path) -> bool {
    TRUSTED_MODULE_PATHS
        .iter()
        .any(|trusted| path.starts_with(std::path::Path::new(trusted)))
}

fn has_native_library_extension(path: &std::path::Path) -> bool {
    let extension = path.extension().and_then(|value| value.to_str());
    #[cfg(target_os = "linux")]
    return extension == Some("so");
    #[cfg(target_os = "macos")]
    return extension == Some("dylib");
    #[cfg(windows)]
    return extension.is_some_and(|value| value.eq_ignore_ascii_case("dll"));
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = extension;
        false
    }
}

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
    /// La bibliothèque reste chargée en mémoire jusqu'à ce que
    /// `unload_module` soit appelé explicitement.
    ///
    /// # Sécurité
    /// Le chemin doit être dans un répertoire de confiance (TRUSTED_MODULE_PATHS).
    /// Seul le symbole `soul_agent_main` est résolu — pas de chargement de symboles arbitraires.
    pub unsafe fn load_agent_routine(
        library_path: &str,
    ) -> Option<(*mut libc::c_void, extern "C" fn(*mut u8))> {
        // Validation du chemin : la bibliothèque doit être dans un répertoire
        // de confiance et avoir l'extension native de la plateforme.
        let path = std::path::Path::new(library_path);
        let canonical = path.canonicalize().ok()?;
        if !is_trusted_module_path(&canonical) || !has_native_library_extension(&canonical) {
            tracing::error!(
                path = %library_path,
                "Module path rejected: untrusted directory or unsupported extension"
            );
            return None;
        }

        let library = match libloading::Library::new(&canonical) {
            Ok(library) => library,
            Err(error) => {
                tracing::error!(
                    path = %library_path,
                    load_error = %error,
                    "Failed to load native module"
                );
                return None;
            }
        };

        // Chercher le symbole "soul_agent_main" — convention de nommage standard SoulSystem.
        let routine = match library.get::<extern "C" fn(*mut u8)>(b"soul_agent_main\0") {
            Ok(symbol) => *symbol,
            Err(error) => {
                tracing::error!(
                    path = %library_path,
                    symbol_error = %error,
                    "Symbol 'soul_agent_main' not found in module"
                );
                return None;
            }
        };
        let handle = Box::into_raw(Box::new(library)).cast::<libc::c_void>();
        Some((handle, routine))
    }

    /// # Safety
    /// handle must be a valid pointer returned by `load_agent_routine`.
    pub unsafe fn unload_module(handle: *mut libc::c_void) {
        if !handle.is_null() {
            drop(Box::from_raw(handle.cast::<libloading::Library>()));
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
        if !path.exists() || !has_native_library_extension(path) {
            return false;
        }
        // Vérifier que le chemin est dans un répertoire de confiance
        if let Ok(canonical) = path.canonicalize() {
            return is_trusted_module_path(&canonical);
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
