//! Sandbox — Test d'évolution en isolation avant application en production

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::{info, warn};
use tokio::sync::Mutex;
use std::sync::Arc;
use crate::CoreState;

/// Environnement sandbox isolé
pub struct Sandbox {
    pub base_dir: PathBuf,
    pub original_dir: PathBuf,
    pub test_command: String,
}

#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub success: bool,
    pub compilation_ok: bool,
    pub tests_passed: bool,
    pub clippy_clean: bool,
    pub output: String,
    pub backup_path: PathBuf,
}

impl Sandbox {
    pub fn new(original_dir: &str, test_command: &str) -> Self {
        let base = Path::new("/tmp/openclaw-u-sandbox");
        let _ = fs::remove_dir_all(base);
        let _ = fs::create_dir_all(base);

        Self {
            base_dir: base.to_path_buf(),
            original_dir: Path::new(original_dir).to_path_buf(),
            test_command: test_command.to_string(),
        }
    }

    /// Copie le projet dans le sandbox
    pub fn setup(&self) -> Result<(), String> {
        info!("📦 SANDBOX setup — copie depuis {}", self.original_dir.display());

        // rsync ou cp -r
        let output = std::process::Command::new("cp")
            .args(["-r", &self.original_dir.to_string_lossy(), &self.base_dir.to_string_lossy()])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                info!("✅ SANDBOX copié dans {}", self.base_dir.display());
                Ok(())
            }
            Ok(o) => Err(format!("cp failed: {}", String::from_utf8_lossy(&o.stderr))),
            Err(e) => Err(format!("cp error: {}", e)),
        }
    }

    /// Applique un patch dans le sandbox
    pub fn apply_patch(&self,
        relative_path: &str,
        new_code: &str,
    ) -> Result<(), String> {
        let target = self.base_dir.join(relative_path);

        // Backup
        let backup = target.with_extension("rs.bak");
        if target.exists() {
            let _ = fs::copy(&target, &backup);
        }

        match fs::write(&target, new_code) {
            Ok(_) => {
                info!("📝 SANDBOX patch appliqué: {}", relative_path);
                Ok(())
            }
            Err(e) => {
                // Restore backup
                if backup.exists() {
                    let _ = fs::copy(&backup, &target);
                }
                Err(format!("Write failed: {}", e))
            }
        }
    }

    /// Compile dans le sandbox
    pub async fn compile(&self) -> Result<String, String> {
        info!("🔨 SANDBOX compilation...");

        let output = tokio::task::spawn_blocking({
            let dir = self.base_dir.clone();
            move || -> Result<std::process::Output, String> {
                std::process::Command::new("cargo")
                    .args(["check", "--message-format=short"])
                    .current_dir(&dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| format!("Process error: {}", e))
            }
        }).await.map_err(|e| format!("Spawn: {}", e))?;

        match output {
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                if result.status.success() {
                    info!("✅ SANDBOX compilation OK");
                    Ok(stderr.to_string())
                } else {
                    warn!("❌ SANDBOX compilation FAILED: {}", stderr);
                    Err(stderr.to_string())
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Lance les tests dans le sandbox
    pub async fn run_tests(&self) -> Result<String, String> {
        info!("🧪 SANDBOX tests...");

        let parts: Vec<&str> = self.test_command.split_whitespace().collect();
        if parts.is_empty() {
            return Err("Empty test command".to_string());
        }

        let output = tokio::task::spawn_blocking({
            let cmd = parts[0].to_string();
            let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            let dir = self.base_dir.clone();
            move || -> Result<std::process::Output, String> {
                let mut command = std::process::Command::new(&cmd);
                for arg in &args { command.arg(arg); }
                command.current_dir(&dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| format!("Process error: {}", e))
            }
        }).await.map_err(|e| format!("Spawn: {}", e))?;

        match output {
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                if result.status.success() {
                    info!("✅ SANDBOX tests PASS");
                    Ok(stderr.to_string())
                } else {
                    warn!("❌ SANDBOX tests FAIL: {}", stderr);
                    Err(stderr.to_string())
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Lance clippy dans le sandbox
    pub async fn run_clippy(&self) -> Result<String, String> {
        info!("🔍 SANDBOX clippy...");

        let output = tokio::task::spawn_blocking({
            let dir = self.base_dir.clone();
            move || -> Result<std::process::Output, String> {
                std::process::Command::new("cargo")
                    .args(["clippy", "--", "-D", "warnings"])
                    .current_dir(&dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| format!("Process error: {}", e))
            }
        }).await.map_err(|e| format!("Spawn: {}", e))?;

        match output {
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                if result.status.success() {
                    info!("✅ SANDBOX clippy CLEAN");
                    Ok(stderr.to_string())
                } else {
                    warn!("⚠️  SANDBOX clippy WARNINGS: {}", stderr);
                    Err(stderr.to_string())
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Pipeline complet : setup → patch → compile → test → clippy
    pub async fn validate_patch(
        &self,
        file_path: &str,
        new_code: &str,
    ) -> SandboxResult {
        let mut result = SandboxResult {
            success: false,
            compilation_ok: false,
            tests_passed: false,
            clippy_clean: false,
            output: String::new(),
            backup_path: self.base_dir.join(file_path).with_extension("rs.bak"),
        };

        // 1. Setup
        if let Err(e) = self.setup() {
            result.output = format!("Setup failed: {}", e);
            return result;
        }

        // 2. Apply patch
        if let Err(e) = self.apply_patch(file_path, new_code) {
            result.output = format!("Patch failed: {}", e);
            return result;
        }

        // 3. Compile
        match self.compile().await {
            Ok(out) => {
                result.compilation_ok = true;
                result.output.push_str(&format!("Compile: {}\n", out));
            }
            Err(e) => {
                result.output = format!("Compile failed: {}", e);
                return result;
            }
        }

        // 4. Tests
        match self.run_tests().await {
            Ok(out) => {
                result.tests_passed = true;
                result.output.push_str(&format!("Tests: {}\n", out));
            }
            Err(e) => {
                result.output.push_str(&format!("Tests failed: {}\n", e));
                // Tests fail but compilation OK = partial success
            }
        }

        // 5. Clippy
        match self.run_clippy().await {
            Ok(out) => {
                result.clippy_clean = true;
                result.output.push_str(&format!("Clippy: {}\n", out));
            }
            Err(e) => {
                result.output.push_str(&format!("Clippy warnings: {}\n", e));
            }
        }

        // Success = compile + tests
        result.success = result.compilation_ok && result.tests_passed;
        result
    }

    /// Copie le patch validé vers le vrai projet
    pub fn promote_to_production(
        &self,
        file_path: &str,
    ) -> Result<(), String> {
        let sandbox_file = self.base_dir.join(file_path);
        let prod_file = self.original_dir.join(file_path);

        // Backup production
        let prod_backup = prod_file.with_extension("rs.bak");
        if prod_file.exists() {
            let _ = fs::copy(&prod_file, &prod_backup);
        }

        match fs::copy(&sandbox_file, &prod_file) {
            Ok(_) => {
                info!("🚀 PROMOTED to production: {}", file_path);
                Ok(())
            }
            Err(e) => {
                // Restore backup
                if prod_backup.exists() {
                    let _ = fs::copy(&prod_backup, &prod_file);
                }
                Err(format!("Promote failed: {}", e))
            }
        }
    }
}

/// Évolue avec sandbox
pub async fn auto_evolve_sandboxed(
    state: Arc<Mutex<CoreState>>,
    file_path: &str,
    new_code: &str,
) -> SandboxResult {
    let sandbox = Sandbox::new("/root/openclaw-u", "cargo test");
    let result = sandbox.validate_patch(file_path, new_code).await;

    if result.success {
        // Promote to production
        let _ = sandbox.promote_to_production(file_path);

        let mut st = state.lock().await;
        st.evolution_count += 1;
        st.log_event("auto_evolve_sandboxed", 0.8, &format!("promoted_{}", file_path));
    } else {
        let mut st = state.lock().await;
        st.log_event("auto_evolve_sandboxed", -0.2, &format!("failed_{}", result.output));
    }

    result
}
