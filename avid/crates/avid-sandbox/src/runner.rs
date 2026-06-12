use std::collections::HashMap;
use std::process::Stdio;
use std::time::Instant;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::{ExecStatus, SandboxConfig};
use crate::error::SandboxError;
use crate::limits;

/// Result of sandbox execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

/// The opaque sandbox handle.
pub struct Sandbox {
    #[allow(clippy::redundant_pub_crate)]
    pub(crate) config: SandboxConfig,
}

/// Write files to temp dir and execute the entrypoint under sandbox constraints.
#[allow(clippy::too_many_lines, clippy::redundant_pub_crate)]
pub(crate) async fn run_sandboxed(
    config: &SandboxConfig,
    files: &HashMap<String, String>,
    entrypoint: &str,
) -> Result<ExecutionResult, SandboxError> {
    let start = Instant::now();
    let tempdir = write_files_to_temp(files).await?;

    if !files.contains_key(entrypoint) {
        return Err(SandboxError::Config(format!(
            "entrypoint {entrypoint} not in submission files"
        )));
    }

    let python = config.python_path.to_string_lossy().to_string();
    let temp_path = tempdir.path().to_path_buf();

    // Build command
    let mut cmd = if config.use_netns {
        let mut c = Command::new("unshare");
        c.arg("-n").arg("--").arg(&python).arg(entrypoint);
        c
    } else {
        let mut c = Command::new(&python);
        c.arg(entrypoint);
        c
    };

    cmd.current_dir(&temp_path);
    cmd.env_clear();
    cmd.env("PATH", "/usr/bin:/bin");
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    limits::configure_sandbox_command(cmd.as_std_mut(), config);

    let mut child = cmd
        .spawn()
        .map_err(|e| SandboxError::Spawn(e.to_string()))?;
    let pgid = child
        .id()
        .ok_or_else(|| SandboxError::Spawn("no child pid".to_string()))?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let result = tokio::time::timeout(config.wall_timeout, async {
        let (stdout_data, stderr_data) = tokio::join!(
            read_capped(stdout_handle, config.stdout_cap),
            read_capped(stderr_handle, config.stderr_cap),
        );
        let status = child.wait().await;
        (stdout_data, stderr_data, status)
    })
    .await;

    match result {
        Ok((stdout_data, stderr_data, Ok(exit_status))) => {
            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
            let stdout = String::from_utf8_lossy(&stdout_data.0).to_string();
            let stderr = String::from_utf8_lossy(&stderr_data.0).to_string();
            let status = if exit_status.success() {
                ExecStatus::Success
            } else {
                ExecStatus::NonZeroExit
            };
            Ok(ExecutionResult {
                status,
                exit_code: exit_status.code(),
                stdout,
                stderr,
                duration_ms,
                stdout_truncated: stdout_data.1,
                stderr_truncated: stderr_data.1,
            })
        }
        Ok((stdout_data, _stderr_data, Err(e))) => {
            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
            Ok(ExecutionResult {
                status: ExecStatus::SpawnError,
                exit_code: None,
                stdout: String::from_utf8_lossy(&stdout_data.0).to_string(),
                stderr: format!("wait error: {e}"),
                duration_ms,
                stdout_truncated: stdout_data.1,
                stderr_truncated: true,
            })
        }
        Err(_elapsed) => {
            let pid_raw = i32::try_from(pgid)
                .map_err(|_| SandboxError::Config("process group id overflow".to_string()))?;

            // Kill the entire process group
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid_raw),
                nix::sys::signal::Signal::SIGKILL,
            );

            // Also explicitly kill the child just in case
            let _ = child.kill().await;
            let _ = child.wait().await;

            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(0);
            Ok(ExecutionResult {
                status: ExecStatus::Timeout,
                exit_code: None,
                stdout: String::new(),
                stderr: String::from("[timeout]"),
                duration_ms,
                stdout_truncated: false,
                stderr_truncated: false,
            })
        }
    }
}

async fn write_files_to_temp(
    files: &HashMap<String, String>,
) -> Result<tempfile::TempDir, SandboxError> {
    let tempdir = tempfile::TempDir::new().map_err(SandboxError::Io)?;
    for (path, content) in files {
        if path.contains("..") || path.starts_with('/') {
            return Err(SandboxError::Config(format!("invalid file path: {path}")));
        }
        let dest = tempdir.path().join(path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(SandboxError::Io)?;
        }
        tokio::fs::write(&dest, content)
            .await
            .map_err(SandboxError::Io)?;
    }
    Ok(tempdir)
}

async fn read_capped(pipe: Option<impl AsyncReadExt + Unpin>, cap: usize) -> (Vec<u8>, bool) {
    let mut reader = match pipe {
        Some(r) => r.take(cap as u64 + 1),
        None => return (Vec::new(), false),
    };
    let mut buf = Vec::new();
    let _ = reader.read_to_end(&mut buf).await;
    let truncated = buf.len() > cap;
    if truncated {
        buf.truncate(cap);
        buf.extend_from_slice(b"\n[truncated]");
    }
    (buf, truncated)
}
