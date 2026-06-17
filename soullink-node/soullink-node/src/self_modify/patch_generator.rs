use crate::self_modify::analyzer::{CodeFinding, CodeReport, FindingCategory};
use crate::self_modify::mutation_strategies::MutationStrategy;
use std::fs;
use std::io::Write;
use uuid::Uuid;

/// Origin of a patch candidate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PatchOrigin {
    /// Generated from static AST analysis.
    StaticAnalysis,
    /// Generated from formal verification feedback.
    FormalFeedback,
    /// Generated from evolutionary search.
    Evolution,
}

/// A candidate patch produced by the generator.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchCandidate {
    pub id: Uuid,
    pub diff: String,
    pub strategy: MutationStrategy,
    pub origin: PatchOrigin,
    /// Files affected by this patch.
    pub affected_files: Vec<String>,
}

/// Generator trait for patch candidates.
pub trait PatchGenerator {
    fn generate(
        &self,
        report: &CodeReport,
    ) -> anyhow::Result<Vec<PatchCandidate>>;
}

/// Default patch generator that produces real patches based on code analysis.
pub struct DefaultPatchGenerator {
    workspace: std::path::PathBuf,
}

impl DefaultPatchGenerator {
    /// Create a new default patch generator.
    pub fn new(workspace: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    /// Generate a unified diff from original and modified content.
    fn compute_diff(original: &str, modified: &str, file_path: &str) -> String {
        let mut diff = String::new();
        diff.push_str(&format!("--- a/{}\n", file_path));
        diff.push_str(&format!("+++ b/{}\n", file_path));

        let orig_lines: Vec<&str> = original.lines().collect();
        let mod_lines: Vec<&str> = modified.lines().collect();

        // Simple line-by-line diff
        let max_len = orig_lines.len().max(mod_lines.len());
        let mut i = 0;
        let mut j = 0;

        while i < max_len || j < max_len {
            let orig = orig_lines.get(i);
            let modif = mod_lines.get(j);

            match (orig, modif) {
                (Some(o), Some(m)) if o == m => {
                    // Context line (unchanged)
                    if diff.lines().count() < 3 || !diff.ends_with(&format!(" {}\n", o)) {
                        diff.push_str(&format!(" {}\n", o));
                    }
                    i += 1;
                    j += 1;
                }
                (Some(o), Some(m)) => {
                    diff.push_str(&format!("-{}\n", o));
                    diff.push_str(&format!("+{}\n", m));
                    i += 1;
                    j += 1;
                }
                (Some(o), None) => {
                    diff.push_str(&format!("-{}\n", o));
                    i += 1;
                }
                (None, Some(m)) => {
                    diff.push_str(&format!("+{}\n", m));
                    j += 1;
                }
                (None, None) => break,
            }
        }

        diff
    }
}

impl PatchGenerator for DefaultPatchGenerator {
    fn generate(
        &self,
        report: &CodeReport,
    ) -> anyhow::Result<Vec<PatchCandidate>> {
        let mut candidates = Vec::new();

        // Group findings by file for batch patching
        let mut findings_by_file: std::collections::HashMap<String, Vec<&CodeFinding>> =
            std::collections::HashMap::new();
        for finding in &report.findings {
            findings_by_file
                .entry(finding.file.clone())
                .or_default()
                .push(finding);
        }

        for (file_path, findings) in &findings_by_file {
            let full_path = self.workspace.join(file_path);
            let original = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut modified = original.clone();

            // Apply fixes based on finding categories
            for finding in findings {
                modified = apply_fix(&modified, finding);
            }

            if modified == original {
                continue; // No changes produced
            }

            let diff = Self::compute_diff(&original, &modified, file_path);

            let candidate = PatchCandidate {
                id: Uuid::new_v4(),
                diff,
                strategy: MutationStrategy::CloneToRef,
                origin: PatchOrigin::StaticAnalysis,
                affected_files: vec![file_path.clone()],
            };

            candidates.push(candidate);
        }

        Ok(candidates)
    }
}

/// Apply a fix to the file content based on a code finding.
fn apply_fix(content: &str, finding: &CodeFinding) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = finding.line.saturating_sub(1); // Convert 1-based to 0-based

    if line_idx >= lines.len() {
        return content.to_string();
    }

    let mut result = String::new();

    match finding.category {
        FindingCategory::TodoMarker => {
            // Replace todo!() / unimplemented!() with proper error
            for (i, line) in lines.iter().enumerate() {
                if i == line_idx {
                    let trimmed = line.trim();
                    if trimmed.contains("todo!()") {
                        let indent = line.len() - trimmed.len();
                        let spaces = " ".repeat(indent);
                        result.push_str(&format!(
                            "{}Err(anyhow::anyhow!(\"TODO: {}\"))",
                            spaces, finding.description
                        ));
                    } else if trimmed.contains("unimplemented!()") {
                        let indent = line.len() - trimmed.len();
                        let spaces = " ".repeat(indent);
                        result.push_str(&format!(
                            "{}Err(anyhow::anyhow!(\"Not implemented: {}\"))",
                            spaces, finding.description
                        ));
                    } else {
                        result.push_str(line);
                    }
                } else {
                    result.push_str(line);
                }
                result.push('\n');
            }
        }
        FindingCategory::DeadCode => {
            // Remove #[allow(dead_code)] annotation
            for (i, line) in lines.iter().enumerate() {
                if i == line_idx {
                    let trimmed = line.trim();
                    if trimmed.contains("#[allow(dead_code)]") {
                        // Skip this line entirely
                        continue;
                    }
                    result.push_str(line);
                } else {
                    result.push_str(line);
                }
                result.push('\n');
            }
        }
        _ => {
            // For other categories, add a comment with the suggestion
            for (i, line) in lines.iter().enumerate() {
                result.push_str(line);
                if i == line_idx {
                    result.push_str(&format!(
                        " // 👆 Auto-fix candidate: {}",
                        finding.suggestion
                    ));
                }
                result.push('\n');
            }
        }
    }

    // Trim trailing newline
    while result.ends_with('\n') {
        result.pop();
    }
    result.push('\n');

    result
}
