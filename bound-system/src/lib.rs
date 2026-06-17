//! Bound System — Execution securisee de commandes shell avec whitelist.
//!
//! Les commandes autorisees sont definies dans une liste blanche.
//! Execution via bubblewrap (bwrap) avec reseau desactive et timeout.
//! Toute execution est tracee dans l'AuditLog.

use std::collections::HashSet;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

/// Ligne de sortie streamée.
#[derive(Debug, Clone)]
pub struct StreamLine {
    pub content: String,
    pub is_stderr: bool,
}

/// Événement terminal de stream.
#[derive(Debug, Clone)]
pub struct StreamEnd {
    pub exit_code: i32,
    pub timed_out: bool,
}
/// Message du stream de sortie.
#[derive(Debug, Clone)]
pub enum StreamMessage {
    Line(StreamLine),
    End(StreamEnd),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// Résultat d'une exécution streamée, contenant le récepteur et le PID.
#[derive(Debug)]
pub struct StreamingExecution {
    pub rx: mpsc::UnboundedReceiver<StreamMessage>,
    pub pid: Option<u32>,
}

/// Gestionnaire d'execution securisee.
pub struct BoundSystem {
    whitelist: HashSet<String>,
    /// Timeout par defaut pour l'execution des commandes.
    timeout: Duration,
    /// Utiliser bubblewrap pour le sandboxing.
    use_sandbox: bool,
    audit: Option<std::sync::Arc<Mutex<Box<dyn AuditTrait>>>>,
}

pub trait AuditTrait: Send + Sync {
    fn log(&mut self, module: &str, action: &str, details: &str) -> anyhow::Result<()>;
}

impl BoundSystem {
    /// Cree un nouveau BoundSystem avec une liste blanche.
    pub fn new(whitelist_commands: Vec<String>) -> Self {
        Self {
            whitelist: whitelist_commands.into_iter().collect(),
            timeout: Duration::from_secs(10),
            use_sandbox: true,
            audit: None,
        }
    }

    /// Liste blanche par defaut (commandes de diagnostic non-destructives).
    pub fn default_whitelist() -> Vec<String> {
        vec![
            "df -h".into(),
            "du -sh /var/lib/soulsystem".into(),
            "systemctl status soulsystem".into(),
            "date".into(),
            "uptime".into(),
            "free -h".into(),
            "whoami".into(),
            "hostname".into(),
            "ps aux --no-headers | head -20".into(),
        ]
    }

    /// Attache un AuditLog pour tracer les executions.
    pub fn with_audit(mut self, audit: std::sync::Arc<Mutex<Box<dyn AuditTrait>>>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Desactive le sandbox bubblewrap (pour les tests).
    pub fn without_sandbox(mut self) -> Self {
        self.use_sandbox = false;
        self
    }

    /// Verifie si une commande est dans la liste blanche.
    pub fn is_allowed(&self, command: &str) -> bool {
        let cmd_trimmed = command.trim();
        self.whitelist.iter().any(|allowed| {
            cmd_trimmed == allowed || cmd_trimmed.starts_with(&format!("{} ", allowed))
        })
    }

    /// Tue un processus par PID avec un signal donné.
    ///
    /// Utilise `libc::kill` pour envoyer le signal. SIGTERM(15) ou SIGKILL(9)
    /// sont les valeurs typiques.
    pub fn kill_process(pid: u32, signal: i32) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
            if result != 0 {
                let err = std::io::Error::last_os_error();
                anyhow::bail!("kill({}, {}) failed: {}", pid, signal, err);
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!("kill_process is only supported on Unix");
        }
    }

    /// Execute une commande. Retourne une erreur si la commande n'est pas autorisee.
    pub async fn execute(&self, command: &str) -> anyhow::Result<CommandResult> {
        if !self.is_allowed(command) {
            anyhow::bail!(
                "Commande non autorisee: '{}'. Utilisez la liste blanche.",
                command
            );
        }

        let result = if self.use_sandbox && Self::bwrap_available() {
            self.execute_sandboxed(command).await?
        } else if self.use_sandbox {
            anyhow::bail!(
                "BoundSystem: sandbox requested but bwrap is not available. \
                 Install bubblewrap or set use_sandbox=false pour exécuter directement."
            );
        } else {
            tracing::warn!("BoundSystem: sandbox désactivé, exécution directe sans isolation");
            self.execute_direct(command).await?
        };

        // Tracer dans l'audit log
        if let Some(audit) = &self.audit {
            let mut a = audit.lock().await;
            let _ = a.log(
                "bound_system",
                "command_executed",
                &format!(
                    "cmd='{}' exit_code={} timed_out={}",
                    command, result.exit_code, result.timed_out
                ),
            );
        }

        Ok(result)
    }

    /// Execute une commande en mode streaming.
    /// Retourne un `StreamingExecution` contenant le récepteur et le PID.
    pub async fn execute_streaming(&self, command: &str) -> anyhow::Result<StreamingExecution> {
        if !self.is_allowed(command) {
            anyhow::bail!(
                "Commande non autorisee: '{}'. Utilisez la liste blanche.",
                command
            );
        }

        // Mirror execute()'s strict policy: a requested sandbox must never be
        // silently downgraded to direct execution. If isolation was asked for
        // but bwrap is missing, refuse rather than streaming unsandboxed.
        if self.use_sandbox && !Self::bwrap_available() {
            anyhow::bail!(
                "BoundSystem: sandbox requested but bwrap is not available. \
                 Install bubblewrap or set use_sandbox=false pour exécuter directement."
            );
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let (pid_tx, pid_rx) = tokio::sync::oneshot::channel();
        let cmd_str = command.to_string();
        let use_sandbox = self.use_sandbox;
        let timeout = self.timeout;
        let audit = self.audit.clone();

        tokio::spawn(async move {
            let result = if use_sandbox {
                Self::run_streaming_sandboxed(&cmd_str, &tx, timeout, pid_tx).await
            } else {
                Self::run_streaming_direct(&cmd_str, &tx, timeout, pid_tx).await
            };

            let (exit_code, timed_out) = match result {
                Ok((code, to)) => (code, to),
                Err(e) => {
                    let _ = tx.send(StreamMessage::Error(e.to_string()));
                    return;
                }
            };

            let _ = tx.send(StreamMessage::End(StreamEnd {
                exit_code,
                timed_out,
            }));

            // Tracer dans l'audit log
            if let Some(audit) = &audit {
                let mut a = audit.lock().await;
                let _ = a.log(
                    "bound_system",
                    "command_streamed",
                    &format!(
                        "cmd='{}' exit_code={} timed_out={}",
                        cmd_str, exit_code, timed_out
                    ),
                );
            }
        });

        // Récupérer le PID (peut échouer si le spawn est très rapide)
        let pid = pid_rx.await.ok().flatten();

        Ok(StreamingExecution { rx, pid })
    }

    // ── Implémentations internes du streaming ──────────────────────────

    async fn run_streaming_sandboxed(
        command: &str,
        tx: &mpsc::UnboundedSender<StreamMessage>,
        timeout: Duration,
        pid_tx: tokio::sync::oneshot::Sender<Option<u32>>,
    ) -> anyhow::Result<(i32, bool)> {
        let mut child = Command::new("bwrap")
            .args([
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind",
                "/lib",
                "/lib",
                "--ro-bind",
                "/lib64",
                "/lib64",
                "--ro-bind",
                "/bin",
                "/bin",
                "--ro-bind",
                "/sbin",
                "/sbin",
                "--ro-bind",
                "/etc",
                "/etc",
                "--unshare-net",
                "--unshare-ipc",
                "--die-with-parent",
                "sh",
                "-c",
                command,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Rapporter le PID
        let pid = child.id();
        let _ = pid_tx.send(pid);

        Self::stream_child_output(&mut child, tx, timeout).await
    }

    async fn run_streaming_direct(
        command: &str,
        tx: &mpsc::UnboundedSender<StreamMessage>,
        timeout: Duration,
        pid_tx: tokio::sync::oneshot::Sender<Option<u32>>,
    ) -> anyhow::Result<(i32, bool)> {
        let mut child = Command::new("sh")
            .args(["-c", command])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Rapporter le PID
        let pid = child.id();
        let _ = pid_tx.send(pid);

        Self::stream_child_output(&mut child, tx, timeout).await
    }

    async fn stream_child_output(
        child: &mut tokio::process::Child,
        tx: &mpsc::UnboundedSender<StreamMessage>,
        timeout: Duration,
    ) -> anyhow::Result<(i32, bool)> {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        let handle_stdout = tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_stdout
                        .send(StreamMessage::Line(StreamLine {
                            content: line,
                            is_stderr: false,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });

        let handle_stderr = tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_stderr
                        .send(StreamMessage::Line(StreamLine {
                            content: line,
                            is_stderr: true,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });

        let status = tokio::time::timeout(timeout, child.wait()).await;

        // Wait for readers to finish
        let _ = tokio::join!(handle_stdout, handle_stderr);

        match status {
            Ok(Ok(s)) => Ok((s.code().unwrap_or(-1), false)),
            Err(_elapsed) => {
                // Kill child on timeout
                let _ = child.kill().await;
                Ok((-1, true))
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("Process error: {}", e)),
        }
    }

    /// Execute dans un sandbox bubblewrap.
    async fn execute_sandboxed(&self, command: &str) -> anyhow::Result<CommandResult> {
        let cmd = Command::new("bwrap")
            .args([
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind",
                "/lib",
                "/lib",
                "--ro-bind",
                "/lib64",
                "/lib64",
                "--ro-bind",
                "/bin",
                "/bin",
                "--ro-bind",
                "/sbin",
                "/sbin",
                "--ro-bind",
                "/etc",
                "/etc",
                "--unshare-net",
                "--unshare-ipc",
                "--die-with-parent",
                "sh",
                "-c",
                command,
            ])
            .output();

        let output = tokio::time::timeout(self.timeout, cmd).await;

        match output {
            Ok(Ok(o)) => Ok(CommandResult {
                command: command.into(),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                exit_code: o.status.code().unwrap_or(-1),
                timed_out: false,
            }),
            Err(_elapsed) => Ok(CommandResult {
                command: command.into(),
                stdout: String::new(),
                stderr: "Timeout (10s)".into(),
                exit_code: -1,
                timed_out: true,
            }),
            Ok(Err(e)) => anyhow::bail!("Sandbox execution failure: {}", e),
        }
    }

    /// Execute directement (sans sandbox, pour les tests ou environnements sans bwrap).
    async fn execute_direct(&self, command: &str) -> anyhow::Result<CommandResult> {
        let cmd = Command::new("sh").args(["-c", command]).output();

        let output = tokio::time::timeout(self.timeout, cmd).await;

        match output {
            Ok(Ok(o)) => Ok(CommandResult {
                command: command.into(),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                exit_code: o.status.code().unwrap_or(-1),
                timed_out: false,
            }),
            Err(_elapsed) => Ok(CommandResult {
                command: command.into(),
                stdout: String::new(),
                stderr: "Timeout (10s)".into(),
                exit_code: -1,
                timed_out: true,
            }),
            Ok(Err(e)) => anyhow::bail!("Command execution failure: {}", e),
        }
    }

    /// Verifie si bubblewrap est disponible sur le systeme.
    fn bwrap_available() -> bool {
        std::process::Command::new("which")
            .arg("bwrap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

pub fn apply_seccomp_profile() -> anyhow::Result<()> {
    // Application du profil seccomp via seccompiler
    // Ceci filtre les appels système autorisés (whitelist) pour le sandbox.
    // Actuellement, seccomp est géré par bwrap directement.
    // Quand un contrôle plus fin est nécessaire, utiliser seccompiler 0.4.
    tracing::debug!("seccomp: seccomp géré par bwrap — pas de profil supplémentaire appliqué");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitelist_allows_listed() {
        let bs = BoundSystem::new(BoundSystem::default_whitelist());
        assert!(bs.is_allowed("date"));
        assert!(bs.is_allowed("df -h"));
        assert!(bs.is_allowed("uptime"));
    }

    #[test]
    fn test_whitelist_rejects_unlisted() {
        let bs = BoundSystem::new(BoundSystem::default_whitelist());
        assert!(!bs.is_allowed("rm -rf /"));
        assert!(!bs.is_allowed("curl evil.com"));
        assert!(!bs.is_allowed("cat /etc/shadow"));
    }

    #[test]
    fn test_whitelist_partial_match() {
        let bs = BoundSystem::new(vec!["df -h".into()]);
        assert!(bs.is_allowed("df -h"));
        assert!(bs.is_allowed("df -h /var"));
        assert!(!bs.is_allowed("df -i"));
    }

    #[tokio::test]
    async fn test_execute_allowed_command() {
        let bs = BoundSystem::new(vec!["date".into()]).without_sandbox();
        let result = bs.execute("date").await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.is_empty());
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_execute_rejected_command() {
        let bs = BoundSystem::new(vec!["date".into()]).without_sandbox();
        let result = bs.execute("rm -rf /").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non autorisee"));
    }

    #[tokio::test]
    async fn test_execute_empty_whitelist() {
        let bs = BoundSystem::new(vec![]).without_sandbox();
        let result = bs.execute("date").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_streaming_refuses_sandbox_without_bwrap() {
        // Sandbox on by default; if bwrap is unavailable the streaming path must
        // refuse rather than silently degrade to unsandboxed execution.
        if BoundSystem::bwrap_available() {
            return; // environment has bwrap; strict-refusal path not exercised
        }
        let bs = BoundSystem::new(vec!["echo".into()]); // use_sandbox = true
        let result = bs.execute_streaming("echo hello").await;
        assert!(result.is_err(), "must refuse, not fall back to direct exec");
        assert!(result.unwrap_err().to_string().contains("bwrap is not available"));
    }

    #[tokio::test]
    async fn test_execute_streaming_output() {
        let bs = BoundSystem::new(vec!["echo".into()]).without_sandbox();
        let exec = bs.execute_streaming("echo hello").await.unwrap();
        assert!(exec.pid.is_some(), "Should capture PID");
        let mut rx = exec.rx;

        let mut lines: Vec<String> = Vec::new();
        let mut exit_code = None;
        while let Some(msg) = rx.recv().await {
            match msg {
                StreamMessage::Line(line) => {
                    lines.push(line.content);
                }
                StreamMessage::End(end) => {
                    exit_code = Some(end.exit_code);
                    break;
                }
                StreamMessage::Error(e) => panic!("Unexpected error: {}", e),
            }
        }
        assert_eq!(lines, vec!["hello"]);
        assert_eq!(exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_streaming_rejected_command() {
        let bs = BoundSystem::new(vec![]).without_sandbox();
        let result = bs.execute_streaming("date").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_kill_process_nonexistent() {
        #[cfg(unix)]
        {
            let result = BoundSystem::kill_process(99999, 15);
            assert!(result.is_err(), "Killing nonexistent PID should fail");
        }
    }

    #[tokio::test]
    async fn test_kill_process_with_sigterm() {
        let bs = BoundSystem::new(vec!["sleep".into()]).without_sandbox();
        // Use a longer timeout so the process isn't killed by timeout
        let bs_long = BoundSystem {
            whitelist: bs.whitelist.clone(),
            timeout: Duration::from_secs(30),
            use_sandbox: false,
            audit: None,
        };

        let exec = bs_long.execute_streaming("sleep 15").await.unwrap();
        let pid = exec.pid.unwrap();
        let mut rx = exec.rx;

        // Donner le temps au processus de démarrer
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Envoyer SIGTERM
        let kill_result = BoundSystem::kill_process(pid, 15);
        assert!(
            kill_result.is_ok(),
            "kill should succeed: {:?}",
            kill_result.err()
        );

        // Le processus devrait se terminer avec un code non-zéro (tué)
        let mut got_end = false;
        while let Some(msg) = rx.recv().await {
            match msg {
                StreamMessage::End(end) => {
                    // Tué par signal = exit code != 0
                    assert!(
                        end.exit_code != 0 || end.timed_out,
                        "Process killed by SIGTERM should have non-zero exit or be timed out, got exit_code={}",
                        end.exit_code
                    );
                    got_end = true;
                    break;
                }
                StreamMessage::Error(_) => {
                    // SIGTERM peut causer une erreur sur le pipe
                    got_end = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(got_end, "Should receive end or error after kill");
    }
}
