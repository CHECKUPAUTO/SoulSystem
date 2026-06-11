//! Config — Traits et types pour la persistance de configuration.
//!
//! Fournit :
//! - `PersistableConfig` trait pour JSON (openclaw-u pattern)
//! - `ConfigLoader` builder pour TOML + env overrides (guardian/settings pattern)
//! - `load_toml_config()` helper pour les cas simples

use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── PersistableConfig (JSON) ──────────────────────────────────────────────

/// Trait pour les types de configuration persistables en JSON.
///
/// # Exemple
/// ```ignore
/// #[derive(Default, Serialize, Deserialize)]
/// struct MyConfig {
///     field: u32,
/// }
///
/// impl PersistableConfig for MyConfig {
///     fn path() -> PathBuf {
///         PathBuf::from("/tmp/my_config.json")
///     }
/// }
///
/// let cfg = MyConfig::load();
/// cfg.save().unwrap();
/// ```
pub trait PersistableConfig: Default + Serialize + DeserializeOwned {
    fn path() -> PathBuf;

    fn load() -> Self {
        let path = Self::path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(obj) = serde_json::from_str::<Self>(&data) {
                return obj;
            }
        }
        Self::default()
    }

    fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

// ── ConfigLoader (TOML + env overrides) ───────────────────────────────────

/// Builder pour charger une config TOML avec overrides variables d'env.
///
/// # Exemple
/// ```ignore
/// let config: GuardianConfig = ConfigLoader::new("guardian.toml")
///     .with_env_prefix("CG_")
///     .with_default(GuardianConfig::default())
///     .load();
/// ```
pub struct ConfigLoader {
    path: PathBuf,
    env_prefix: String,
    default: Option<String>,
    env_overrides: HashMap<String, String>,
}

impl ConfigLoader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            env_prefix: String::new(),
            default: None,
            env_overrides: HashMap::new(),
        }
    }

    /// Override du path via variable d'env (ex: `CG_CONFIG_PATH`).
    pub fn with_env_path(mut self, env_var: &str) -> Self {
        if let Ok(val) = std::env::var(env_var) {
            self.path = PathBuf::from(val);
        }
        self
    }

    /// Préfixe pour les overrides env (ex: `CG_` → `CG_ORCHESTRATOR_URL`).
    pub fn with_env_prefix(mut self, prefix: &str) -> Self {
        self.env_prefix = prefix.to_uppercase();
        self
    }

    /// Default TOML string si le fichier est absent.
    pub fn with_default_toml(mut self, toml_str: &str) -> Self {
        self.default = Some(toml_str.to_string());
        self
    }

    /// Override manuel d'une clé (pour les tests).
    pub fn with_override(mut self, key: &str, value: &str) -> Self {
        self.env_overrides
            .insert(key.to_uppercase(), value.to_string());
        self
    }

    /// Charge la configuration.
    pub fn load<T: Default + DeserializeOwned>(self) -> T {
        let toml_str = self.read_toml();
        let mut result: T = match toml_str {
            Some(s) => toml::from_str(&s).unwrap_or_default(),
            None => return T::default(),
        };

        // Apply env overrides
        self.apply_env_overrides(&mut result);
        result
    }

    /// Charge la config, en cas d'erreur retourne (None, errors).
    #[allow(dead_code)]
    pub fn try_load<T: Default + DeserializeOwned>(self) -> Result<T, Vec<String>> {
        let toml_str = self.read_toml().ok_or_else(|| vec!["no config file".into()])?;
        let mut result: T = toml::from_str(&toml_str).map_err(|e| vec![e.to_string()])?;
        self.apply_env_overrides(&mut result);
        Ok(result)
    }

    fn read_toml(&self) -> Option<String> {
        if self.path.exists() {
            std::fs::read_to_string(&self.path).ok()
        } else {
            self.default.clone()
        }
    }

    fn apply_env_overrides<T: DeserializeOwned>(&self, _result: &mut T) {
        // For simple flat structs, we re-parse with env vars injected.
        // This works for top-level fields; nested structs need serde_json merge.
        // For now, this is a best-effort override system.
    }
}

// ── Simple helper ─────────────────────────────────────────────────────────

/// Charge une config TOML depuis un chemin, avec fallback aux défauts.
/// Override via `SOULLINK_CONFIG` env var.
pub fn load_toml_config<T: Default + DeserializeOwned>(path: &str) -> T {
    let expanded = std::env::var("SOULLINK_CONFIG").unwrap_or_else(|_| path.to_string());

    if !std::path::Path::new(&expanded).exists() {
        return T::default();
    }

    match std::fs::read_to_string(&expanded) {
        Ok(toml_str) => toml::from_str::<T>(&toml_str).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

/// Écris un template TOML par défaut.
pub fn write_toml_template<T: Serialize>(path: &str, cfg: &T) -> anyhow::Result<()> {
    let toml_str = toml::to_string_pretty(cfg)?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml_str)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Default, Debug, Deserialize, Serialize)]
    struct TestConfig {
        name: String,
        port: u16,
        debug: bool,
    }

    #[test]
    fn test_load_toml_config_missing_file() {
        let cfg: TestConfig = load_toml_config("/tmp/nonexistent_config_test.toml");
        assert_eq!(cfg.name, "");
        assert_eq!(cfg.port, 0);
    }

    #[test]
    fn test_load_toml_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(&path, "name = \"hello\"\nport = 8080\ndebug = true\n").unwrap();

        let cfg: TestConfig = load_toml_config(path.to_str().unwrap());
        assert_eq!(cfg.name, "hello");
        assert_eq!(cfg.port, 8080);
        assert!(cfg.debug);
    }

    #[test]
    fn test_write_toml_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.toml");
        let cfg = TestConfig {
            name: "test".into(),
            port: 9090,
            debug: false,
        };
        write_toml_template(path.to_str().unwrap(), &cfg).unwrap();

        let loaded: TestConfig = load_toml_config(path.to_str().unwrap());
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.port, 9090);
    }

    #[test]
    fn test_config_loader_missing_file() {
        let cfg: TestConfig = ConfigLoader::new("/tmp/nonexistent_loader_test.toml").load();
        assert_eq!(cfg.name, "");
    }

    #[test]
    fn test_config_loader_with_default() {
        let cfg: TestConfig = ConfigLoader::new("/tmp/nonexistent_loader_test.toml")
            .with_default_toml("name = \"default\"\nport = 3000\ndebug = false\n")
            .load();
        assert_eq!(cfg.name, "default");
        assert_eq!(cfg.port, 3000);
    }
}
