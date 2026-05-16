//! Configuration centralisée de SoulSystem.
//!
//! Lit le fichier `soulsystem.toml` à la racine du projet (ou un chemin
//! spécifié via `SOULSYSTEM_CONFIG_FILE`) puis surcharge les valeurs par
//! les variables d'environnement `SOULSYSTEM_*`.

use serde::Deserialize;
use std::path::PathBuf;

/// Chemins configurés pour SoulSystem.
#[derive(Debug, Clone, Deserialize)]
pub struct Paths {
    #[serde(default = "default_config_dir")]
    pub config_dir: PathBuf,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
}

fn default_config_dir() -> PathBuf {
    PathBuf::from("/opt/soulsystem/config")
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/soulsystem/data")
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/soulsystem")
}

/// Configuration racine de SoulSystem.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub paths: Paths,
}

impl Settings {
    /// Charge la configuration depuis `soulsystem.toml`, puis écrase les
    /// valeurs par les variables d'environnement `SOULSYSTEM_*`.
    pub fn new() -> anyhow::Result<Self> {
        let config_path = std::env::var("SOULSYSTEM_CONFIG_FILE")
            .unwrap_or_else(|_| "soulsystem.toml".to_string());

        let config_content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    "Fichier de configuration {} introuvable, utilisation des valeurs par défaut",
                    config_path
                );
                return Ok(Self::default_from_env());
            }
            Err(e) => anyhow::bail!("Impossible de lire {}: {}", config_path, e),
        };

        let mut settings: Settings = toml::from_str(&config_content)?;
        settings.apply_env_overrides();
        Ok(settings)
    }

    /// Applique les surcharges d'environnement.
    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("SOULSYSTEM_CONFIG_DIR") {
            self.paths.config_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("SOULSYSTEM_DATA_DIR") {
            self.paths.data_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("SOULSYSTEM_LOG_DIR") {
            self.paths.log_dir = PathBuf::from(v);
        }
    }

    /// Valeurs par défaut avec surcharges d'environnement (sans fichier).
    fn default_from_env() -> Self {
        let mut settings = Settings {
            paths: Paths {
                config_dir: default_config_dir(),
                data_dir: default_data_dir(),
                log_dir: default_log_dir(),
            },
        };
        settings.apply_env_overrides();
        settings
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            config_dir: default_config_dir(),
            data_dir: default_data_dir(),
            log_dir: default_log_dir(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            paths: Paths::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_paths() {
        let settings = Settings::default();
        assert_eq!(
            settings.paths.config_dir,
            PathBuf::from("/opt/soulsystem/config")
        );
    }

    #[test]
    fn test_env_override() {
        let tmp = TempDir::new().unwrap();
        let config_file = tmp.path().join("test_config.toml");
        let content = r#"
[paths]
config_dir = "/tmp/test_config"
"#;
        std::fs::write(&config_file, content).unwrap();

        std::env::set_var("SOULSYSTEM_CONFIG_FILE", config_file.to_str().unwrap());
        let settings = Settings::new().unwrap();
        assert_eq!(settings.paths.config_dir, PathBuf::from("/tmp/test_config"));
    }

    #[test]
    fn test_missing_file_defaults() {
        std::env::set_var("SOULSYSTEM_CONFIG_FILE", "/nonexistent/path.toml");
        let settings = Settings::new().unwrap();
        assert_eq!(
            settings.paths.data_dir,
            PathBuf::from("/var/lib/soulsystem/data")
        );
    }

    #[test]
    fn test_env_var_override() {
        // Sauvegarder et restaurer
        let tmp = TempDir::new().unwrap();
        let config_file = tmp.path().join("env_test.toml");
        std::fs::write(&config_file, "[paths]\nconfig_dir = \"/from_file\"\n").unwrap();

        std::env::set_var("SOULSYSTEM_CONFIG_DIR", "/from_env");
        std::env::set_var("SOULSYSTEM_CONFIG_FILE", config_file.to_str().unwrap());

        let settings = Settings::new().unwrap();
        // La variable d'environnement écrase le fichier
        assert_eq!(settings.paths.config_dir, PathBuf::from("/from_env"));

        std::env::remove_var("SOULSYSTEM_CONFIG_DIR");
    }
}
