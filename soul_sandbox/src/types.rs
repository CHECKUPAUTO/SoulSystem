use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Erreurs du sandbox.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("commande vide")]
    Empty,
    #[error("commande interdite: {0}")]
    Forbidden(String),
    #[error("commande non listée en whitelist: {0}")]
    NotWhitelisted(String),
    #[error("chemin sensible interdit: {0}")]
    SensitivePath(String),
    #[error("binaire shell interdit (bash -c, sh -c, eval) — passer par argv direct")]
    ShellEscape(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Catégorie de menace détectée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatKind {
    DestructiveRecursive,
    ForkBomb,
    RawDiskWrite,
    SystemConfigWrite,
    ShellEscape,
    DownloadExec,
    SensitivePath,
    ShellBypass,
    EvalSource,
}

/// Verdict détaillé d'une exécution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxVerdict {
    pub command: String,
    pub command_normalized: String,
    pub binary: String,
    pub allowed: bool,
    pub reason: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub threats: Vec<String>,
}

/// Type de flux (stdout ou stderr) pour le streaming (C.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}
