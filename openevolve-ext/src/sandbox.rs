//! Sandbox pour exécution sécurisée de code compilé.
//! Utilise bubblewrap (bwrap) pour l'isolation si disponible.

use anyhow::Result;
use std::path::Path;
use std::process::Output;

/// Configuration de la sandbox (backward compat)
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub timeout_secs: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self { timeout_secs: 30 }
    }
}

/// Vérifie si bubblewrap est disponible
pub fn bwrap_available() -> bool {
    which::which("bwrap").is_ok()
}

/// Exécute dans bwrap si disponible, sinon sans sandbox
pub async fn run_in_sandbox(
    code: &str,
    _language: &str,
    _work_dir: &Path,
    _config: &SandboxConfig,
) -> Result<Output> {
    if bwrap_available() {
        run_in_bwrap_sandbox(code)
    } else {
        run_unsandboxed(code)
    }
}

fn run_in_bwrap_sandbox(program: &str) -> Result<Output> {
    // Write temp file
    let tmpfile = std::env::temp_dir().join(format!("openevolve_{}.rs", std::process::id()));
    std::fs::write(&tmpfile, program)?;

    let output = std::process::Command::new("bwrap")
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
            "--tmpfs",
            "/tmp",
            "--unshare-net",
            "--die-with-parent",
            "--",
            "rustc",
            tmpfile.to_str().unwrap(),
        ])
        .output()?;

    let _ = std::fs::remove_file(&tmpfile);
    Ok(output)
}

fn run_unsandboxed(program: &str) -> Result<Output> {
    let tmpfile = std::env::temp_dir().join(format!("openevolve_{}.rs", std::process::id()));
    std::fs::write(&tmpfile, program)?;

    let output = std::process::Command::new("rustc")
        .arg(tmpfile.to_str().unwrap())
        .output()?;

    let _ = std::fs::remove_file(&tmpfile);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bwrap_available() {
        let _avail = bwrap_available();
        // Just check it doesn't panic
    }
}
