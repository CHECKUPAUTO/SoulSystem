//! Bound System — Execution securisee de commandes shell avec whitelist.
//!
//! Les commandes autorisees sont definies dans une liste blanche.
//! Execution via bubblewrap (bwrap) avec reseau desactive et timeout.
//! Toute execution est tracee dans l'AuditLog.

use crate::audit_log::AuditLog;
use std::collections::HashSet;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

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

/// Gestionnaire d'execution securisee.
pub struct BoundSystem {
    whitelist: HashSet<String>,
    /// Timeout par defaut pour l'execution des commandes.
    timeout: Duration,
    /// Utiliser bubblewrap pour le sandboxing.
    use_sandbox: bool,
    audit: Option<std::sync::Arc<Mutex<AuditLog>>>,
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
    pub fn with_audit(mut self, audit: std::sync::Arc<Mutex<AuditLog>>) -> Self {
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
            // La commande doit commencer par la commande authorisee
            cmd_trimmed == allowed || cmd_trimmed.starts_with(&format!("{} ", allowed))
        })
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
        } else {
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
    /// Retourne un receiver qui recoit les lignes en temps reel.
    pub async fn execute_streaming(
        &self,
        command: &str,
    ) -> anyhow::Result<mpsc::UnboundedReceiver<StreamMessage>> {
        if !self.is_allowed(command) {
            anyhow::bail!(
                "Commande non autorisee: '{}'. Utilisez la liste blanche.",
                command
            );
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let cmd_str = command.to_string();
        let use_sandbox = self.use_sandbox && Self::bwrap_available();
        let timeout = self.timeout;
        let audit = self.audit.clone();

        tokio::spawn(async move {
            let result = if use_sandbox {
                Self::run_streaming_sandboxed(&cmd_str, &tx, timeout).await
            } else {
                Self::run_streaming_direct(&cmd_str, &tx, timeout).await
            };

            let (exit_code, timed_out) = match result {
                Ok((code, to)) => (code, to),
                Err(e) => {
                    let _ = tx.send(StreamMessage::Error(e.to_string()));
                    return;
                }
            };

            let end = StreamEnd {
                exit_code,
                timed_out,
            };
            let _ = tx.send(StreamMessage::End(end));

            if let Some(audit) = audit {
                let mut a = audit.lock().await;
                let _ = a.log(
                    "bound_system",
                    "command_streamed",
                    &format!("cmd='{}' exit_code={}", cmd_str, exit_code),
                );
            }
        });

        Ok(rx)
    }

    async fn run_streaming_sandboxed(
        command: &str,
        tx: &mpsc::UnboundedSender<StreamMessage>,
        timeout: Duration,
    ) -> anyhow::Result<(i32, bool)> {
        let mut child = Command::new("bwrap")
            .args([
                "--ro-bind", "/usr", "/usr",
                "--ro-bind", "/lib", "/lib",
                "--ro-bind", "/lib64", "/lib64",
                "--ro-bind", "/bin", "/bin",
                "--ro-bind", "/sbin", "/sbin",
                "--ro-bind", "/etc", "/etc",
                "--unshare-net",
                "--unshare-ipc",
                "--die-with-parent",
                "sh", "-c", command,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        Self::stream_child_output(&mut child, tx, timeout).await
    }

    async fn run_streaming_direct(
        command: &str,
        tx: &mpsc::UnboundedSender<StreamMessage>,
        timeout: Duration,
    ) -> anyhow::Result<(i32, bool)> {
        let mut child = Command::new("sh")
            .args(["-c", command])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

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
    async fn test_execute_streaming_output() {
        let bs = BoundSystem::new(vec!["echo".into()]).without_sandbox();
        let mut rx = bs
            .execute_streaming("echo hello")
            .await
            .unwrap();

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
}
