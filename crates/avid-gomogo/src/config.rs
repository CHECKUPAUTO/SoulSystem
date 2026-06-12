use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Configuration globale de GOMOGO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GomogoConfig {
    pub scanner: ScannerConfig,
    pub proxy: ProxyConfig,
    pub captcha: CaptchaConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub default_concurrency: usize,
    pub default_delay_ms: u64,
    pub default_timeout_secs: u64,
    pub user_agent_rotation: bool,
    pub custom_user_agents: Vec<String>,
    pub follow_redirects: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub default_listen: String,
    pub default_cert_dir: String,
    pub domain_whitelist: Vec<String>,
    pub domain_blacklist: Vec<String>,
    pub modification_rules: Vec<ModificationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModificationRule {
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub scope: RuleScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleScope {
    Request,
    Response,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptchaConfig {
    pub default_port: u16,
    pub default_bind: String,
    pub task_timeout_secs: u64,
    pub result_ttl_secs: u64,
    pub max_pending_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub json_format: bool,
    pub file_output: Option<String>,
}

impl Default for GomogoConfig {
    fn default() -> Self {
        Self {
            scanner: ScannerConfig {
                default_concurrency: 10,
                default_delay_ms: 200,
                default_timeout_secs: 10,
                user_agent_rotation: false,
                custom_user_agents: vec![],
                follow_redirects: true,
            },
            proxy: ProxyConfig {
                default_listen: "127.0.0.1:8080".into(),
                default_cert_dir: "./certs".into(),
                domain_whitelist: vec![],
                domain_blacklist: vec![],
                modification_rules: vec![],
            },
            captcha: CaptchaConfig {
                default_port: 8080,
                default_bind: "127.0.0.1".into(),
                task_timeout_secs: 300,
                result_ttl_secs: 600,
                max_pending_tasks: 1000,
            },
            logging: LoggingConfig {
                level: "info".into(),
                json_format: false,
                file_output: None,
            },
        }
    }
}

impl GomogoConfig {
    /// Charge la configuration depuis un fichier TOML
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        match path {
            Some(p) if p.exists() => {
                let content = std::fs::read_to_string(p)?;
                Ok(toml::from_str(&content)?)
            }
            _ => {
                // Chercher dans les emplacements standards
                if let Some(cfg) = Self::find_config_file() {
                    let content = std::fs::read_to_string(&cfg)?;
                    Ok(toml::from_str(&content)?)
                } else {
                    Ok(Self::default())
                }
            }
        }
    }

    fn find_config_file() -> Option<PathBuf> {
        // 1. Dossier courant
        let cwd = PathBuf::from("config.toml");
        if cwd.exists() {
            return Some(cwd);
        }

        // 2. $HOME/.config/gomogo/config.toml
        if let Some(home) = dirs::config_dir() {
            let cfg = home.join("gomogo").join("config.toml");
            if cfg.exists() {
                return Some(cfg);
            }
        }

        None
    }

    /// Crée un fichier de configuration par défaut
    pub fn generate_default(path: &Path) -> anyhow::Result<()> {
        let toml_str = toml::to_string_pretty(&Self::default())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}
