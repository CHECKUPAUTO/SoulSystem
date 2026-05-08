//! Autocode — auto-évolution par modification de code source (LLM-powered v0.4.0)

use std::fs;
use std::path::Path;
use std::process::Stdio;
use tracing::{info, warn};
use tokio::sync::Mutex;
use std::sync::Arc;

use crate::CoreState;
use crate::llm::LlmEngine;
use crate::sandbox::Sandbox;

/// Résultat d'une tentative d'évolution
#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub success: bool,
    pub patch_file: String,
    pub test_output: String,
    pub commit_hash: Option<String>,
    pub energy_delta: f64,
    pub llm_generated: bool,
}

/// Moteur d'auto-codage
pub struct AutoCoder {
    pub project_dir: String,
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
        let src_dir = Path::new(&self.project_dir).join("src");

        if let Ok(entries) = fs::read_dir(&src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let rel = path.strip_prefix(&self.project_dir).unwrap_or(&path);
                        let rel_str = rel.to_string_lossy();

                        if content.contains("unwrap()") {
                            let count = content.matches("unwrap()").count();
                            opportunities.push(ImprovementOpportunity {
                                file: rel_str.to_string(),
                                kind: OpportunityKind::ReplaceUnwrap,
                                description: format!("{} unwrap() trouvés", count),
                                priority: count as u8,
                            });
                        }
                        if content.contains("todo!") || content.contains("unimplemented!") {
                            let count = content.matches("todo!").count() + content.matches("unimplemented!").count();
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

        opportunities.sort_by(|a, b| b.priority.cmp(&a.priority));
        opportunities
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
                "-m", &format!("Auto-evolution: {}", file_path),
                "--author", "Tarek <tarek@avid.dev>",
            ])
            .current_dir(&self.project_dir)
            .output();

        match git_commit {
            Ok(o) if o.status.success() => {
                let hash = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Ok(hash)
            }
            Ok(o) => Err(format!("git commit failed: {}", String::from_utf8_lossy(&o.stderr))),
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
    RemoveStub,
    ReplacePrintln,
    AddTests,
    OptimizeAlgorithm,
}

/// Évolue automatiquement le projet (fallback sans LLM)
pub async fn auto_evolve(state: Arc<Mutex<CoreState>>) -> EvolutionResult {
    let coder = AutoCoder::new("/root/openclaw-u", "cargo test");
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
    info!("🔍 OPPORTUNITÉ (fallback): {} — {}", best.file, best.description);

    let patch = format!(
        r#"// PATCH: Amélioration identifiée
// Fichier: {}
// Problème: {}
// TODO: Implémentation manuelle nécessaire
"#,
        best.file, best.description
    );
    let patch_path = "/root/openclaw-u/src/upgrade_patch.rs".to_string();
    let _ = fs::write(&patch_path, patch);

    let mut st = state.lock().await;
    st.log_event("auto_evolve", 0.1, &format!("placeholder_{}", best.description));

    EvolutionResult {
        success: true,
        patch_file: patch_path,
        test_output: "Placeholder patch generated".to_string(),
        commit_hash: None,
        energy_delta: 0.1,
        llm_generated: false,
    }
}

/// Évolue automatiquement le projet (LLM + SANDBOX)
pub async fn auto_evolve_llm(state: Arc<Mutex<CoreState>>, llm: LlmEngine) -> EvolutionResult {
    let coder = AutoCoder::new("/root/openclaw-u", "cargo test");
    let opportunities = coder.scan_for_improvements();

    if opportunities.is_empty() {
        let mut st = state.lock().await;
        st.log_event("auto_evolve", 0.1, "no_opportunities_found");
        return EvolutionResult {
            success: true, patch_file: String::new(),
            test_output: "No improvements needed".to_string(),
            commit_hash: None, energy_delta: 0.1, llm_generated: false,
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
        OpportunityKind::ReplaceUnwrap => "Remplace unwrap() par expect() avec messages descriptifs",
        OpportunityKind::RemoveStub => "Implémente les fonctions marquées todo!() ou unimplemented!()",
        OpportunityKind::ReplacePrintln => "Remplace println! par tracing::info! pour logging structuré",
        OpportunityKind::AddTests => "Ajoute des tests unitaires pour les fonctions publiques",
        OpportunityKind::OptimizeAlgorithm => "Optimise l'algorithme pour meilleure performance",
    };

    let generated_code = llm.generate_code(&best.file, issue, &current_code).await;

    match generated_code {
        Some(new_code) => {
            info!("✨ LLM a généré {} octets de code", new_code.len());

            // === SANDBOX VALIDATION ===
            let sandbox = Sandbox::new("/root/openclaw-u", "cargo test");
            let validation = sandbox.validate_patch(&best.file, &new_code).await;

            if validation.success {
                info!("✅ SANDBOX OK — compilation + tests passent");

                // Promote to production
                if let Err(e) = sandbox.promote_to_production(&best.file) {
                    warn!("❌ PROMOTE FAILED: {} — rollback", e);
                    let mut st = state.lock().await;
                    st.log_event("auto_evolve_llm", -0.4, &format!("promote_FAIL_{}", e));
                    return EvolutionResult {
                        success: false, patch_file: String::new(),
                        test_output: format!("Promote failed: {}", e),
                        commit_hash: None, energy_delta: -0.4, llm_generated: true,
                    };
                }

                info!("🚀 PROMOTED to production: {}", best.file);

                // Commit
                let commit = coder.commit_patch(&best.file);
                let mut st = state.lock().await;
                st.evolution_count += 1;
                st.log_event("auto_evolve_llm", 0.8, &format!("sandbox_OK_{}", best.description));

                EvolutionResult {
                    success: true,
                    patch_file: best.file.clone(),
                    test_output: validation.output,
                    commit_hash: commit.ok(),
                    energy_delta: 0.8,
                    llm_generated: true,
                }
            } else {
                warn!("❌ SANDBOX FAIL — compile:{} tests:{}",
                    if validation.compilation_ok { "OK" } else { "FAIL" },
                    if validation.tests_passed { "OK" } else { "FAIL" }
                );
                let mut st = state.lock().await;
                st.log_event("auto_evolve_llm", -0.3, &format!("sandbox_FAIL_{}", validation.output));

                EvolutionResult {
                    success: false, patch_file: String::new(),
                    test_output: validation.output,
                    commit_hash: None, energy_delta: -0.3, llm_generated: true,
                }
            }
        }
        None => {
            warn!("❌ LLM n'a pas généré de code — fallback");
            auto_evolve(state).await
        }
    }
}
