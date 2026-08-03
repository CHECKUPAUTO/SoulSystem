//! Configuration TOML de SYNERGIE.
//!
//! Sections : agent / ecosystem / embed / persistence / api / action / detectors.

use crate::detect::DetectConfig;
use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use soulsystem_common::secrets::SecretString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub agent: AgentCfg,
    pub ecosystem: EcosystemCfg,
    pub embed: EmbedCfg,
    pub persistence: PersistenceCfg,
    pub api: ApiCfg,
    pub action: ActionCfg,
    pub detectors: DetectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCfg {
    pub tick_seconds: u64,
    pub action_threshold: f32,
    pub max_actions_per_tick: usize,
    pub dry_run: bool,
    pub notify_on_action: bool,
    #[serde(default = "default_git_window")]
    pub git_window: usize,
    #[serde(default = "default_stale_after_days")]
    pub stale_after_days: i64,
}

fn default_git_window() -> usize {
    300
}
fn default_stale_after_days() -> i64 {
    14
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemCfg {
    pub roots: Vec<PathBuf>,
    pub max_depth: usize,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,

    #[serde(skip)]
    pub ignore_regex: Option<Regex>,
}

fn default_true() -> bool {
    true
}
fn default_max_file_size() -> usize {
    2 * 1024 * 1024
} // 2 MiB

impl EcosystemCfg {
    pub fn compile_ignore(&mut self) {
        if self.ignore.is_empty() {
            self.ignore_regex = None;
            return;
        }
        let joined = self.ignore.join("|");
        self.ignore_regex = Regex::new(&joined).ok();
    }

    pub fn ignore_patterns_matches(&self, path: &str) -> bool {
        if let Some(re) = &self.ignore_regex {
            re.is_match(path)
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedCfg {
    pub enabled: bool,
    /// Mode "local" (scirust-core) ou "http" (TRIBE distant).
    #[serde(default = "default_embed_mode")]
    pub mode: String,
    #[serde(default = "default_embed_endpoint")]
    pub endpoint: String,
    pub dim: usize,
    #[serde(default = "default_embed_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f32,
}

fn default_embed_mode() -> String {
    "local".to_string()
}
fn default_embed_endpoint() -> String {
    "http://127.0.0.1:7440/embed".to_string()
}
fn default_embed_timeout() -> u64 {
    15
}
fn default_similarity_threshold() -> f32 {
    0.86
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceCfg {
    pub path: PathBuf,
    pub reports_dir: PathBuf,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

fn default_max_history() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCfg {
    pub bind: String,
    pub port: u16,
    #[serde(default = "default_ws_path")]
    pub ws_path: String,
    /// Comma-free list of exact origins permitted to make cross-origin
    /// browser requests. Empty (the default) permits none — INV-NET-4.
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// Deprecated. This used to default to `true`, so an operator who simply
    /// omitted the field got `Access-Control-Allow-Origin: *` on an API whose
    /// `auth_token` also defaults to `None`.
    ///
    /// It is still parsed so an existing config is not silently reinterpreted:
    /// setting it now logs a warning and is **not** honoured. Use
    /// `cors_origins` to name the origins you actually want.
    #[serde(default)]
    pub cors_allow_any: bool,
    #[serde(default)]
    pub auth_token: Option<SecretString>,
}

fn default_ws_path() -> String {
    "/ws".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCfg {
    #[serde(default)]
    pub auto_apply_safe_patches: bool,
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default)]
    pub telegram_bot_token: Option<SecretString>,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    #[serde(default)]
    pub github_proposals_repo: Option<PathBuf>,
    #[serde(default)]
    pub github_remote: Option<String>,
}

fn default_branch_prefix() -> String {
    "synergie/auto/".to_string()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("lecture {}", path.display()))?;
        let mut cfg: Self =
            toml::from_str(&text).with_context(|| format!("parse TOML {}", path.display()))?;
        cfg.ecosystem.compile_ignore();
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    pub fn try_load_default() -> Self {
        let candidates = [
            std::env::var("SYNERGIE_CONFIG").ok().map(PathBuf::from),
            Some(PathBuf::from("/etc/synergie/synergie.toml")),
            Some(PathBuf::from("./synergie.toml")),
        ];
        for opt in candidates.iter().flatten() {
            if opt.exists() {
                if let Ok(c) = Self::load(opt) {
                    return c;
                }
            }
        }
        Self::default()
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(t) = std::env::var("TELEGRAM_BOT_TOKEN") {
            if !t.is_empty() {
                self.action.telegram_bot_token = Some(SecretString::new(t));
            }
        }
        if let Ok(c) = std::env::var("TELEGRAM_CHAT_ID") {
            if !c.is_empty() {
                self.action.telegram_chat_id = Some(c);
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut eco = EcosystemCfg {
            roots: vec![
                PathBuf::from("/root/soullink-brain"),
                PathBuf::from("/root/soullink-organs"),
                PathBuf::from("/root/soulsystem-gateway"),
                PathBuf::from("/root/.soulsystem/workspace"),
                PathBuf::from("/root/openevolve"),
                PathBuf::from("/mnt/nvme_secondary/system_root/AVID"),
                PathBuf::from("/root/aris"),
                PathBuf::from("/root/cortex"),
            ],
            max_depth: 6,
            ignore: vec![
                "/target/".into(),
                "/node_modules/".into(),
                r"/\.venv/".into(),
                r"/\.git/".into(),
                "/dist/".into(),
                "/build/".into(),
                r"\.lock$".into(),
                "/__pycache__/".into(),
            ],
            respect_gitignore: true,
            max_file_size: 2 * 1024 * 1024,
            ignore_regex: None,
        };
        eco.compile_ignore();
        Self {
            agent: AgentCfg {
                tick_seconds: 21600,
                action_threshold: 3.0,
                max_actions_per_tick: 3,
                dry_run: false,
                notify_on_action: true,
                git_window: 300,
                stale_after_days: 14,
            },
            ecosystem: eco,
            embed: EmbedCfg {
                enabled: true,
                mode: "local".into(),
                endpoint: "http://127.0.0.1:7440/embed".into(),
                dim: 128,
                timeout_seconds: 15,
                similarity_threshold: 0.86,
            },
            persistence: PersistenceCfg {
                path: PathBuf::from("/mnt/nvme/soullink_brain/synergies/synergie.sled"),
                reports_dir: PathBuf::from("/mnt/nvme/soullink_brain/synergies/reports"),
                max_history: 50,
            },
            api: ApiCfg {
                bind: "127.0.0.1".into(),
                port: 7460,
                ws_path: "/ws".into(),
                cors_origins: Vec::new(),
                cors_allow_any: false,
                auth_token: None,
            },
            action: ActionCfg {
                auto_apply_safe_patches: false,
                branch_prefix: "synergie/auto/".into(),
                telegram_bot_token: None,
                telegram_chat_id: None,
                github_proposals_repo: None,
                github_remote: Some("origin".into()),
            },
            detectors: DetectConfig::default(),
        }
    }
}
