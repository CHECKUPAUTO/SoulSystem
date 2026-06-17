use crate::self_modify::patch_generator::PatchCandidate;
use std::process::Command;

/// Result of validating a candidate patch.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub passed: bool,
    pub build_ok: bool,
    pub tests_ok: bool,
    pub lint_ok: bool,
    pub warnings: Vec<String>,
}

/// Validator configuration.
#[derive(Debug, Clone, Default)]
pub struct ValidatorConfig {
    pub require_tests: bool,
    pub allow_warnings: bool,
    /// Timeout for cargo operations in seconds.
    pub cargo_timeout_secs: u64,
    /// Target crate to check (if None, checks the whole workspace).
    pub target_crate: Option<String>,
}

/// Trait for patch validators.
pub trait Validator {
    fn validate(&self, candidate: &PatchCandidate) -> anyhow::Result<ValidationReport>;
}

/// Cargo-based validator (check + test offline).
/// Applies the diff as a patch, runs cargo check on the affected files,
/// then restores the original state.
pub struct CargoValidator {
    workspace: std::path::PathBuf,
    config: ValidatorConfig,
}

impl CargoValidator {
    /// Create a new Cargo-based validator.
    pub fn new(
        workspace: impl Into<std::path::PathBuf>,
        config: ValidatorConfig,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            config,
        }
    }
}

impl Validator for CargoValidator {
    fn validate(&self, candidate: &PatchCandidate) -> anyhow::Result<ValidationReport> {
        let mut report = ValidationReport::default();

        // Step 1: Apply the patch to affected files (backup originals)
        let backup_dir = tempfile::tempdir()?;
        let mut backups: Vec<(String, Vec<u8>)> = Vec::new();

        for file_path in &candidate.affected_files {
            let full_path = self.workspace.join(file_path);
            if full_path.exists() {
                let original = std::fs::read(&full_path)?;
                let backup_path = backup_dir.path().join(
                    file_path.replace('/', "_").replace('\\', "_"),
                );
                std::fs::write(&backup_path, &original)?;
                backups.push((file_path.clone(), original));
            }
        }

        // Apply the diff to the files
        if let Err(e) = apply_diff(&self.workspace, &candidate.diff, &candidate.affected_files) {
            report.warnings.push(format!("Failed to apply diff: {}", e));
            restore_backups(&backups);
            return Ok(report);
        }

        // Step 2: Run cargo check
        let check_result = run_cargo(&self.workspace, "check", self.config.cargo_timeout_secs);
        match check_result {
            Ok(output) => {
                report.build_ok = true;
                if !output.stderr.is_empty() {
                    for line in output.stderr.lines() {
                        if line.contains("warning:") {
                            report.warnings.push(line.to_string());
                        }
                    }
                }
            }
            Err(e) => {
                report.build_ok = false;
                report.warnings.push(format!("cargo check failed: {}", e));
            }
        }

        // Step 3: Run cargo clippy if build passed
        if report.build_ok {
            let lint_result = run_cargo(&self.workspace, "clippy", self.config.cargo_timeout_secs);
            match lint_result {
                Ok(output) => {
                    report.lint_ok = true;
                    if !output.stderr.is_empty() {
                        for line in output.stderr.lines() {
                            if line.contains("warning:") {
                                report.warnings.push(line.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    report.lint_ok = false;
                    report.warnings.push(format!("cargo clippy failed: {}", e));
                }
            }
        }

        // Step 4: Run cargo test if required
        if self.config.require_tests && report.build_ok {
            let test_result = run_cargo(&self.workspace, "test", self.config.cargo_timeout_secs * 3);
            match test_result {
                Ok(output) => {
                    report.tests_ok = true;
                }
                Err(e) => {
                    report.tests_ok = false;
                    report.warnings.push(format!("cargo test failed: {}", e));
                }
            }
        } else if self.config.require_tests {
            report.warnings
                .push("Skipping tests: build check failed".into());
        }

        // Step 5: Restore original files
        restore_backups(&backups);

        // Step 6: Determine pass/fail
        report.passed = report.build_ok
            && report.lint_ok
            && (!self.config.require_tests || report.tests_ok)
            && (self.config.allow_warnings || report.warnings.is_empty());

        Ok(report)
    }
}

/// Apply a unified diff to files in the workspace.
fn apply_diff(
    workspace: &std::path::Path,
    diff: &str,
    affected_files: &[String],
) -> anyhow::Result<()> {
    // Parse the unified diff
    let mut current_file: Option<String> = None;
    let mut hunks: Vec<UnifiedHunk> = Vec::new();
    let mut current_hunk_lines: Vec<DiffLine> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            // File header — flush current hunk
            if !current_hunk_lines.is_empty() {
                if let Some(ref f) = current_file {
                    hunks.push(UnifiedHunk {
                        file: f.clone(),
                        lines: std::mem::take(&mut current_hunk_lines),
                    });
                }
            }
            if line.starts_with("--- a/") {
                current_file = Some(line[6..].to_string());
            }
            continue;
        }

        if line.starts_with(' ') || line.starts_with('-') || line.starts_with('+') {
            current_hunk_lines.push(DiffLine {
                kind: match line.chars().next() {
                    Some(' ') => DiffKind::Context,
                    Some('-') => DiffKind::Removed,
                    Some('+') => DiffKind::Added,
                    _ => DiffKind::Context,
                },
                content: line[1..].to_string(),
            });
        }
    }

    // Flush last hunk
    if !current_hunk_lines.is_empty() {
        if let Some(ref f) = current_file {
            hunks.push(UnifiedHunk {
                file: f.clone(),
                lines: current_hunk_lines,
            });
        }
    }

    // Apply hunks to files
    for hunk in &hunks {
        let full_path = workspace.join(&hunk.file);
        if !full_path.exists() {
            continue;
        }

        let original = std::fs::read_to_string(&full_path)?;
        let modified = apply_hunk(&original, hunk);
        std::fs::write(&full_path, modified)?;
    }

    Ok(())
}

struct UnifiedHunk {
    file: String,
    lines: Vec<DiffLine>,
}

struct DiffLine {
    kind: DiffKind,
    content: String,
}

#[derive(PartialEq)]
enum DiffKind {
    Context,
    Removed,
    Added,
}

/// Apply a hunk to file content.
fn apply_hunk(original: &str, hunk: &UnifiedHunk) -> String {
    let mut result = String::new();
    let orig_lines: Vec<&str> = original.lines().collect();

    let mut hunk_idx = 0;
    let mut file_idx = 0;

    while hunk_idx < hunk.lines.len() && file_idx < orig_lines.len() {
        match hunk.lines[hunk_idx].kind {
            DiffKind::Context => {
                if file_idx < orig_lines.len() {
                    result.push_str(orig_lines[file_idx]);
                    result.push('\n');
                    file_idx += 1;
                }
                hunk_idx += 1;
            }
            DiffKind::Removed => {
                // Skip the removed line in the original
                file_idx += 1;
                hunk_idx += 1;
            }
            DiffKind::Added => {
                // Insert the new line
                result.push_str(&hunk.lines[hunk_idx].content);
                result.push('\n');
                hunk_idx += 1;
                // Don't advance file_idx
            }
        }
    }

    // Append any remaining original lines
    for i in file_idx..orig_lines.len() {
        result.push_str(orig_lines[i]);
        result.push('\n');
    }

    // Append any remaining added lines
    for i in hunk_idx..hunk.lines.len() {
        if hunk.lines[i].kind == DiffKind::Added {
            result.push_str(&hunk.lines[i].content);
            result.push('\n');
        }
    }

    result
}

fn restore_backups(backups: &[(String, Vec<u8>)]) {
    for (path, content) in backups {
        let _ = std::fs::write(path, content);
    }
}

struct CargoOutput {
    stderr: String,
}

fn run_cargo(
    workspace: &std::path::Path,
    subcommand: &str,
    timeout_secs: u64,
) -> anyhow::Result<CargoOutput> {
    use std::time::Duration;

    let output = Command::new("cargo")
        .arg(subcommand)
        .arg("--workspace")
        .current_dir(workspace)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run cargo {}: {}", subcommand, e))?;

    if output.status.success() {
        Ok(CargoOutput {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(anyhow::anyhow!(
            "cargo {} failed:\nstdout:\n{}\nstderr:\n{}",
            subcommand,
            stdout,
            stderr
        ))
    }
}
