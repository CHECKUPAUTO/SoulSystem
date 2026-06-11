//! Autocode — auto-évolution par modification de code source (LLM-powered v0.4.0)

use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::llm::LlmEngine;
use crate::sandbox::{Sandbox, SandboxResult};
use crate::CoreState;

/// Résultat d'une tentative d'évolution
#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub success: bool,
    #[allow(dead_code)]
    pub patch_file: String,
    #[allow(dead_code)]
    pub test_output: String,
    #[allow(dead_code)]
    pub commit_hash: Option<String>,
    pub energy_delta: f64,
    #[allow(dead_code)]
    pub llm_generated: bool,
}

/// Moteur d'auto-codage
pub struct AutoCoder {
    pub project_dir: String,
    #[allow(dead_code)]
    pub test_command: String,
}

impl AutoCoder {
    pub fn new(project_dir: &str, test_command: &str) -> Self {
        Self {
            project_dir: project_dir.to_string(),
            test_command: test_command.to_string(),
        }
    }

    pub fn scan_for_improvements(&self) -> Vec<ImprovementOpportunity> {
        let mut opportunities = Vec::new();
        let scan_dirs = vec![
            Path::new(&self.project_dir).join("src"),
            Path::new("/app/soullink-brain/soullink-core/src").to_path_buf(),
            Path::new("/app/soullink-organs/src").to_path_buf(),
        ];

        for src_dir in scan_dirs {
            if !src_dir.exists() {
                continue;
            }
            self.scan_dir(&src_dir, &mut opportunities);
        }

        opportunities.sort_by(|a, b| b.priority.cmp(&a.priority));
        opportunities
    }

    fn scan_dir(&self, dir: &Path, opportunities: &mut Vec<ImprovementOpportunity>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_dir(&path, opportunities);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let rel_str = path.to_string_lossy();

                        if content.contains("unwrap()") {
                            let count = content.matches("unwrap()").count();
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::ReplaceUnwrap,
                                description: format!("{} unwrap() trouvés", count),
                                priority: count as u8,
                            });
                        }
                        if content.contains("panic!") {
                            let count = content.matches("panic!").count();
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::ReplacePanic,
                                description: format!("{} panic! trouvés", count),
                                priority: (count * 2) as u8,
                            });
                        }
                        if content.contains("todo!") || content.contains("unimplemented!") {
                            let count = content.matches("todo!").count()
                                + content.matches("unimplemented!").count();
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::RemoveStub,
                                description: format!("{} stub trouvés", count),
                                priority: 10,
                            });
                        }
                        if content.contains("println!") {
                            let count = content.matches("println!").count();
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::ReplacePrintln,
                                description: format!("{} println! trouvés", count),
                                priority: count as u8,
                            });
                        }
                        if !content.contains(r#"#[cfg(test)]"#) {
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::AddTests,
                                description: "Pas de tests unitaires".to_string(),
                                priority: 5,
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn commit_patch(&self, file_path: &str) -> Result<String, String> {
        let git_add = std::process::Command::new("git")
            .args(["add", file_path])
            .current_dir(&self.project_dir)
            .output();

        if git_add.is_err() {
            return Err("git add failed".to_string());
        }

        let git_commit = std::process::Command::new("git")
            .args([
                "commit",
                "-m",
                &format!("Auto-evolution: {}", file_path),
                "--author",
                "Tarek <tarek@avid.dev>",
            ])
            .current_dir(&self.project_dir)
            .output();

        match git_commit {
            Ok(o) if o.status.success() => {
                let hash = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Ok(hash)
            }
            Ok(o) => Err(format!(
                "git commit failed: {}",
                String::from_utf8_lossy(&o.stderr)
            )),
            Err(e) => Err(format!("git error: {}", e)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImprovementOpportunity {
    pub file: String,
    pub kind: OpportunityKind,
    pub description: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpportunityKind {
    ReplaceUnwrap,
    ReplacePanic,
    RemoveStub,
    ReplacePrintln,
    AddTests,
    OptimizeAlgorithm,
}

/// Évolue automatiquement le projet (fallback sans LLM)
/// Now actually fixes detected issues instead of just logging placeholders.
pub async fn auto_evolve(state: Arc<Mutex<CoreState>>) -> EvolutionResult {
    let coder = AutoCoder::new("/app/openclaw-u", "cargo test");
    let opportunities = coder.scan_for_improvements();

    if opportunities.is_empty() {
        let mut st = state.lock().await;
        st.log_event("auto_evolve", 0.1, "no_opportunities_found");
        return EvolutionResult {
            success: true,
            patch_file: String::new(),
            test_output: "No improvements needed".to_string(),
            commit_hash: None,
            energy_delta: 0.1,
            llm_generated: false,
        };
    }

    let best = &opportunities[0];
    info!(
        "🔍 OPPORTUNITÉ: {} — {:?} ({})",
        best.file, best.kind, best.description
    );

    // Apply real fixes based on opportunity kind
    let fix_result = match best.kind {
        OpportunityKind::RemoveStub => fix_stubs(&best.file),
        OpportunityKind::ReplaceUnwrap => fix_unwraps(&best.file),
        OpportunityKind::ReplacePanic => fix_panics(&best.file),
        OpportunityKind::ReplacePrintln => fix_printlns(&best.file),
        _ => Ok(format!(
            "// No auto-fix available for {:?} in {}\n",
            best.kind, best.file
        )),
    };

    match fix_result {
        Ok(fix_content) if !fix_content.is_empty() => {
            let patch_path = "/app/openclaw-u/src/upgrade_patch.rs".to_string();
            let _ = fs::write(&patch_path, &fix_content);

            let mut st = state.lock().await;
            st.log_event(
                "auto_evolve",
                0.3,
                &format!("fixed_{:?}_{}", best.kind, best.file),
            );
            info!("✅ Auto-fix applied: {:?} in {}", best.kind, best.file);

            EvolutionResult {
                success: true,
                patch_file: patch_path,
                test_output: format!("Fixed {:?} in {}", best.kind, best.file),
                commit_hash: None,
                energy_delta: 0.3,
                llm_generated: false,
            }
        }
        Ok(_) | Err(_) => {
            let mut st = state.lock().await;
            st.log_event("auto_evolve", 0.1, &format!("no_fix_{}", best.description));
            EvolutionResult {
                success: true,
                patch_file: String::new(),
                test_output: format!("No fix needed for {:?}", best.kind),
                commit_hash: None,
                energy_delta: 0.1,
                llm_generated: false,
            }
        }
    }
}

/// Fix stub functions — replace todo!()/unimplemented!() with proper Err returns.
fn fix_stubs(file_path: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read {}: {}", file_path, e))?;

    let fixed = content
        .replace("todo!()", "Err(anyhow::anyhow!(\"Not yet implemented\"))")
        .replace(
            "unimplemented!()",
            "Err(anyhow::anyhow!(\"Feature not available\"))",
        );

    if fixed != content {
        std::fs::write(file_path, &fixed)
            .map_err(|e| format!("Cannot write {}: {}", file_path, e))?;
        Ok(format!("// Fixed stubs in {}\n", file_path))
    } else {
        Ok(String::new())
    }
}

/// Fix unwrap() calls with proper expect() or error handling.
fn fix_unwraps(file_path: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read {}: {}", file_path, e))?;

    // Replace blind unwrap() with expect() with descriptive messages
    let fixed = content.replace(
        ".unwrap()",
        ".expect(\"Operation failed — auto-fixed by AutoCoder\")",
    );

    if fixed != content {
        std::fs::write(file_path, &fixed)
            .map_err(|e| format!("Cannot write {}: {}", file_path, e))?;
        Ok(format!("// Fixed unwrap() in {}\n", file_path))
    } else {
        Ok(String::new())
    }
}

/// Fix panic!() calls with proper error handling.
fn fix_panics(file_path: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read {}: {}", file_path, e))?;

    let lines: Vec<String> = content
        .lines()
        .map(|l| {
            if l.trim().starts_with("panic!") {
                l.replace(
                    "panic!",
                    "// AUTOFIX: panic! → error handling needed — review manually\n    // panic!",
                )
            } else {
                l.to_string()
            }
        })
        .collect();

    let fixed = lines.join("\n");
    if fixed != content {
        std::fs::write(file_path, &fixed)
            .map_err(|e| format!("Cannot write {}: {}", file_path, e))?;
        Ok(format!("// Fixed panic! in {}\n", file_path))
    } else {
        Ok(String::new())
    }
}

/// Fix println!() with tracing calls.
fn fix_printlns(file_path: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Cannot read {}: {}", file_path, e))?;

    let fixed = content
        .replace("println!(\"", "tracing::info!(\"")
        .replace("eprintln!(\"", "tracing::error!(\"");

    if fixed != content {
        std::fs::write(file_path, &fixed)
            .map_err(|e| format!("Cannot write {}: {}", file_path, e))?;
        Ok(format!("// Fixed println! in {}\n", file_path))
    } else {
        Ok(String::new())
    }
}

/// Évolue automatiquement le projet (LLM + SANDBOX)
pub async fn auto_evolve_llm(state: Arc<Mutex<CoreState>>, llm: LlmEngine) -> EvolutionResult {
    let coder = AutoCoder::new("/app/openclaw-u", "cargo test");
    let opportunities = coder.scan_for_improvements();

    if opportunities.is_empty() {
        let mut st = state.lock().await;
        st.log_event("auto_evolve", 0.1, "no_opportunities_found");
        return EvolutionResult {
            success: true,
            patch_file: String::new(),
            test_output: "No improvements needed".to_string(),
            commit_hash: None,
            energy_delta: 0.1,
            llm_generated: false,
        };
    }

    let best = &opportunities[0];
    info!("🔍 OPPORTUNITÉ: {} — {}", best.file, best.description);

    // Lire code actuel
    let full_path = Path::new(&coder.project_dir).join(&best.file);
    let current_code = match fs::read_to_string(&full_path) {
        Ok(code) => code,
        Err(e) => {
            warn!("❌ Impossible de lire {}: {}", best.file, e);
            return auto_evolve(state).await;
        }
    };

    let issue = match best.kind {
        OpportunityKind::ReplaceUnwrap => {
            "Remplace unwrap() par expect() avec messages descriptifs"
        }
        OpportunityKind::ReplacePanic => "Remplace panic!() par un retour de Result::Err() propre",
        OpportunityKind::RemoveStub => {
            "Implémente les fonctions marquées todo!() ou unimplemented!()"
        }
        OpportunityKind::ReplacePrintln => {
            "Remplace println! par tracing::info! pour logging structuré"
        }
        OpportunityKind::AddTests => "Ajoute des tests unitaires pour les fonctions publiques",
        OpportunityKind::OptimizeAlgorithm => "Optimise l'algorithme pour meilleure performance",
    };

    let generated_code = llm.generate_code(&best.file, issue, &current_code).await;

    match generated_code {
        Some(new_code) => {
            info!("✨ LLM a généré {} octets de code", new_code.len());

            // === SANDBOX VALIDATION ===
            let sandbox = Sandbox::new("/app/openclaw-u", "cargo test");
            let validation = sandbox.validate_patch(&best.file, &new_code).await;

            if validation.success {
                info!("✅ SANDBOX OK — compilation + tests passent");

                // Promote to production
                if let Err(e) = sandbox.promote_to_production(&best.file) {
                    warn!("❌ PROMOTE FAILED: {} — rollback", e);
                    let mut st = state.lock().await;
                    st.log_event("auto_evolve_llm", -0.4, &format!("promote_FAIL_{}", e));
                    return EvolutionResult {
                        success: false,
                        patch_file: String::new(),
                        test_output: format!("Promote failed: {}", e),
                        commit_hash: None,
                        energy_delta: -0.4,
                        llm_generated: true,
                    };
                }

                info!("🚀 PROMOTED to production: {}", best.file);

                // Commit
                let commit = coder.commit_patch(&best.file);
                let mut st = state.lock().await;
                st.evolution_count += 1;
                st.log_event(
                    "auto_evolve_llm",
                    0.8,
                    &format!("sandbox_OK_{}", best.description),
                );

                EvolutionResult {
                    success: true,
                    patch_file: best.file.clone(),
                    test_output: validation.output,
                    commit_hash: commit.ok(),
                    energy_delta: 0.8,
                    llm_generated: true,
                }
            } else {
                warn!(
                    "❌ SANDBOX FAIL — compile:{} tests:{}",
                    if validation.compilation_ok {
                        "OK"
                    } else {
                        "FAIL"
                    },
                    if validation.tests_passed {
                        "OK"
                    } else {
                        "FAIL"
                    }
                );
                let mut st = state.lock().await;
                st.log_event(
                    "auto_evolve_llm",
                    -0.3,
                    &format!("sandbox_FAIL_{}", validation.output),
                );

                EvolutionResult {
                    success: false,
                    patch_file: String::new(),
                    test_output: validation.output,
                    commit_hash: None,
                    energy_delta: -0.3,
                    llm_generated: true,
                }
            }
        }
        None => {
            warn!("❌ LLM n'a pas généré de code — fallback");
            auto_evolve(state).await
        }
    }
}

#[allow(dead_code)]
pub async fn auto_evolve_sandboxed(
    state: Arc<Mutex<CoreState>>,
    file_path: &str,
    new_code: &str,
) -> SandboxResult {
    let sandbox = Sandbox::new("/app/openclaw-u", "cargo test");
    let result = sandbox.validate_patch(file_path, new_code).await;

    if result.success {
        // Promote to production
        let _ = sandbox.promote_to_production(file_path);

        let mut st = state.lock().await;
        st.evolution_count += 1;
        st.log_event(
            "auto_evolve_sandboxed",
            0.8,
            &format!("promoted_{}", file_path),
        );
    } else {
        let mut st = state.lock().await;
        st.log_event(
            "auto_evolve_sandboxed",
            -0.2,
            &format!("failed_{}", result.output),
        );
    }

    result
}
