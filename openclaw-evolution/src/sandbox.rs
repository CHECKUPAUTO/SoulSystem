use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Résultat d'exécution sandbox
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    /// Le code a-t-il compilé ?
    pub compiled: bool,
    /// Le binaire a-t-il tourné sans erreur ?
    pub ran_ok: bool,
    /// Nombre de tests passés / total
    pub tests_passed: usize,
    pub tests_total: usize,
    /// Temps de compilation (ms)
    pub compile_time_ms: u64,
    /// Temps d'exécution (ms)
    pub run_time_ms: u64,
    /// Sortie stdout
    pub stdout: String,
    /// Sortie stderr
    pub stderr: String,
    /// Erreur éventuelle
    pub error: Option<String>,
}

impl SandboxResult {
    /// Échec de compilation
    pub fn compile_fail(error: String, compile_time_ms: u64) -> Self {
        Self {
            compiled: false,
            ran_ok: false,
            tests_passed: 0,
            tests_total: 0,
            compile_time_ms,
            run_time_ms: 0,
            stdout: String::new(),
            stderr: error.clone(),
            error: Some(error),
        }
    }

    /// Calcule un score de performance normalisé [0.0, 1.0]
    pub fn performance_score(&self) -> f32 {
        if !self.compiled {
            return 0.0;
        }

        let mut score = 0.2; // bonus compilation

        if self.ran_ok {
            score += 0.2; // bonus exécution sans crash
        }

        // Score de tests
        if self.tests_total > 0 {
            score += 0.4 * (self.tests_passed as f32 / self.tests_total as f32);
        } else if self.ran_ok {
            score += 0.2; // pas de tests mais a tourné
        }

        // Bonus rapidité (compile < 10s = bonus, run < 1s = bonus)
        if self.compile_time_ms < 10_000 {
            score += 0.1;
        }
        if self.run_time_ms > 0 && self.run_time_ms < 1_000 {
            score += 0.1;
        }

        score.min(1.0)
    }

    /// Score de stabilité [0.0, 1.0]
    pub fn stability_score(&self) -> f32 {
        let mut score = 0.0f32;

        if self.compiled {
            score += 0.4;
        }
        if self.ran_ok {
            score += 0.3;
        }
        if self.error.is_none() {
            score += 0.2;
        }
        // Pas de warnings = bonus
        if self.compiled && !self.stderr.contains("warning") {
            score += 0.1;
        }

        score.min(1.0)
    }
}

// ─────────────────────────────────────────────
// Sandbox d'exécution
// ─────────────────────────────────────────────

pub struct Sandbox {
    base_dir: PathBuf,
    timeout: Duration,
}

impl Sandbox {
    pub fn new(base_dir: PathBuf, timeout_secs: u64) -> Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self {
            base_dir,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    /// Évaluer un code_patch d'agent dans un sandbox isolé
    pub async fn evaluate(&self, agent_id: &str, code: &str) -> SandboxResult {
        if code.trim().is_empty() {
            return SandboxResult {
                compiled: false,
                ran_ok: false,
                tests_passed: 0,
                tests_total: 0,
                compile_time_ms: 0,
                run_time_ms: 0,
                stdout: String::new(),
                stderr: "Code vide".into(),
                error: Some("Code vide — pas d'évaluation".into()),
            };
        }

        // Créer un répertoire sandbox temporaire
        let sandbox_dir = self.base_dir.join(format!("sandbox_{}", agent_id));
        let src_dir = sandbox_dir.join("src");

        if let Err(e) = std::fs::create_dir_all(&src_dir) {
            return SandboxResult::compile_fail(format!("Erreur création sandbox: {}", e), 0);
        }

        // Écrire le Cargo.toml minimal
        let cargo_toml = r#"[package]
name = "agent_sandbox"
version = "0.1.0"
edition = "2021"
"#;

        if let Err(e) = std::fs::write(sandbox_dir.join("Cargo.toml"), cargo_toml) {
            return SandboxResult::compile_fail(format!("Erreur écriture Cargo.toml: {}", e), 0);
        }

        // Injecter le code de l'agent avec un harness de test
        let full_code = Self::wrap_agent_code(code);
        if let Err(e) = std::fs::write(src_dir.join("main.rs"), &full_code) {
            return SandboxResult::compile_fail(format!("Erreur écriture main.rs: {}", e), 0);
        }

        // Phase 1 : Compilation
        let compile_start = Instant::now();
        let compile_result = self
            .run_command(
                "cargo",
                &[
                    "build",
                    "--release",
                    "--manifest-path",
                    sandbox_dir.join("Cargo.toml").to_str().unwrap_or(""),
                ],
            )
            .await;
        let compile_time_ms = compile_start.elapsed().as_millis() as u64;

        let (compile_ok, compile_stderr) = match compile_result {
            Ok((success, _, stderr)) => (success, stderr),
            Err(e) => {
                self.cleanup(&sandbox_dir);
                return SandboxResult::compile_fail(
                    format!("Erreur compilation: {}", e),
                    compile_time_ms,
                );
            }
        };

        if !compile_ok {
            self.cleanup(&sandbox_dir);
            return SandboxResult::compile_fail(compile_stderr, compile_time_ms);
        }

        info!("Sandbox {}: compilé en {}ms", agent_id, compile_time_ms);

        // Phase 2 : Exécution
        let bin_path = sandbox_dir.join("target/release/agent_sandbox");
        let run_start = Instant::now();
        let run_result = self.run_command(bin_path.to_str().unwrap_or(""), &[]).await;
        let run_time_ms = run_start.elapsed().as_millis() as u64;

        let (ran_ok, stdout, stderr) = match run_result {
            Ok((success, stdout, stderr)) => (success, stdout, stderr),
            Err(e) => {
                self.cleanup(&sandbox_dir);
                return SandboxResult {
                    compiled: true,
                    ran_ok: false,
                    tests_passed: 0,
                    tests_total: 0,
                    compile_time_ms,
                    run_time_ms,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    error: Some(format!("Erreur exécution: {}", e)),
                };
            }
        };

        // Phase 3 : Parser les résultats de tests
        let (tests_passed, tests_total) = Self::parse_test_results(&stdout);

        info!(
            "Sandbox {}: exécuté en {}ms — tests {}/{}, ok={}",
            agent_id, run_time_ms, tests_passed, tests_total, ran_ok
        );

        self.cleanup(&sandbox_dir);

        SandboxResult {
            compiled: true,
            ran_ok,
            tests_passed,
            tests_total,
            compile_time_ms,
            run_time_ms,
            stdout,
            stderr,
            error: if ran_ok {
                None
            } else {
                Some("Exécution échouée".into())
            },
        }
    }

    /// Envelopper le code agent avec un harness de test
    fn wrap_agent_code(code: &str) -> String {
        // Si le code contient déjà fn main, on l'utilise tel quel
        // Sinon on l'enveloppe dans un main avec des tests
        if code.contains("fn main") {
            // Ajouter un harness de sortie standardisé
            format!(
                r#"{}

// ── Harness OpenClaw ──
// Le code agent contient déjà main()
"#,
                code
            )
        } else {
            // Envelopper dans un main avec mini-tests
            format!(
                r#"use std::time::Instant;

fn agent_code() -> Result<Vec<(String, bool)>, Box<dyn std::error::Error>> {{
    let mut results: Vec<(String, bool)> = Vec::new();

    // ── Code agent ──
    {}

    Ok(results)
}}

fn main() {{
    let start = Instant::now();
    match agent_code() {{
        Ok(results) => {{
            let total = results.len();
            let passed = results.iter().filter(|(_, ok)| *ok).count();
            println!("OPENCLAW_TESTS:{{}}/{{}}", passed, total);
            println!("OPENCLAW_TIME_US:{{}}", start.elapsed().as_micros());
            println!("OPENCLAW_STATUS:OK");
        }}
        Err(e) => {{
            println!("OPENCLAW_STATUS:ERROR:{{}}", e);
        }}
    }}
}}"#,
                code
            )
        }
    }

    /// Parser les résultats de tests depuis stdout
    fn parse_test_results(stdout: &str) -> (usize, usize) {
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("OPENCLAW_TESTS:") {
                let parts: Vec<&str> = rest.split('/').collect();
                if parts.len() == 2 {
                    let passed = parts[0].parse().unwrap_or(0);
                    let total = parts[1].parse().unwrap_or(0);
                    return (passed, total);
                }
            }
        }
        // Si pas de ligne OPENCLAW_TESTS, considérer 1/1 si le programme a tourné
        if stdout.contains("OPENCLAW_STATUS:OK") {
            (1, 1)
        } else {
            (0, 1)
        }
    }

    /// Exécuter une commande avec timeout
    async fn run_command(&self, cmd: &str, args: &[&str]) -> Result<(bool, String, String)> {
        let output = tokio::time::timeout(self.timeout, Command::new(cmd).args(args).output())
            .await
            .map_err(|_| anyhow::anyhow!("Timeout après {:?}", self.timeout))?
            .map_err(|e| anyhow::anyhow!("Erreur commande: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((output.status.success(), stdout, stderr))
    }

    /// Nettoyer le répertoire sandbox
    fn cleanup(&self, dir: &Path) {
        if let Err(e) = std::fs::remove_dir_all(dir) {
            warn!("Erreur nettoyage sandbox {:?}: {}", dir, e);
        }
    }
}
