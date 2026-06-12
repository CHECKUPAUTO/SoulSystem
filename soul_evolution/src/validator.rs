use crate::frontmatter;
use crate::types::*;
use std::path::Path;

pub struct Validator;

impl Validator {
    pub fn validate_agent_file(content: &str) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if content.trim().is_empty() {
            errors.push(ValidationError {
                severity: Severity::Critical,
                message: "Agent file is empty".to_string(),
                field: None,
            });
            return ValidationResult {
                valid: false,
                errors,
                warnings,
            };
        }

        let fm = match frontmatter::parse_frontmatter(content) {
            Some((fm, _)) => fm,
            None => {
                errors.push(ValidationError {
                    severity: Severity::Critical,
                    message: "Missing or invalid YAML frontmatter (must start and end with ---)"
                        .to_string(),
                    field: None,
                });
                return ValidationResult {
                    valid: false,
                    errors,
                    warnings,
                };
            }
        };

        if !fm.has_key("name") {
            errors.push(ValidationError {
                severity: Severity::Critical,
                message: "Missing required field: name".to_string(),
                field: Some("name".to_string()),
            });
        } else if let Some(name) = fm.get_string("name") {
            if name.is_empty() {
                errors.push(ValidationError {
                    severity: Severity::High,
                    message: "Field 'name' is empty".to_string(),
                    field: Some("name".to_string()),
                });
            }
            if name.contains(' ') {
                warnings.push(ValidationError {
                    severity: Severity::Low,
                    message: "Agent name contains spaces; kebab-case preferred".to_string(),
                    field: Some("name".to_string()),
                });
            }
        }

        if !fm.has_key("description") {
            errors.push(ValidationError {
                severity: Severity::High,
                message: "Missing required field: description".to_string(),
                field: Some("description".to_string()),
            });
        } else if let Some(desc) = fm.get_string("description") {
            if desc.len() < 20 {
                warnings.push(ValidationError {
                    severity: Severity::Medium,
                    message: format!(
                        "Description is too short ({} chars, minimum 20 recommended)",
                        desc.len()
                    ),
                    field: Some("description".to_string()),
                });
            }
        }

        if !fm.has_key("tools") {
            warnings.push(ValidationError {
                severity: Severity::Medium,
                message: "No 'tools' field - agent cannot use any tools".to_string(),
                field: Some("tools".to_string()),
            });
        } else if let Some(tools) = fm.get_string_array("tools") {
            if tools.is_empty() {
                warnings.push(ValidationError {
                    severity: Severity::Medium,
                    message: "'tools' array is empty".to_string(),
                    field: Some("tools".to_string()),
                });
            }
        }

        if !fm.has_key("model") {
            warnings.push(ValidationError {
                severity: Severity::Low,
                message: "No 'model' field specified; will use default model".to_string(),
                field: Some("model".to_string()),
            });
        }

        let body = frontmatter::extract_body(content);
        if body.is_empty() {
            warnings.push(ValidationError {
                severity: Severity::Medium,
                message: "Agent has no body content".to_string(),
                field: Some("body".to_string()),
            });
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    pub fn validate_file(path: &Path) -> ValidationResult {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return ValidationResult {
                    valid: false,
                    errors: vec![ValidationError {
                        severity: Severity::Critical,
                        message: format!("Cannot read file: {}", e),
                        field: None,
                    }],
                    warnings: vec![],
                };
            }
        };
        Self::validate_agent_file(&content)
    }

    pub fn validate_proposal(proposal: &ImprovementProposal) -> ValidationResult {
        Self::validate_agent_file(&proposal.template_content)
    }

    pub fn validate_batch(
        proposals: &[ImprovementProposal],
    ) -> Vec<(String, ValidationResult)> {
        proposals
            .iter()
            .map(|p| {
                let result = Self::validate_proposal(p);
                (p.id.clone(), result)
            })
            .collect()
    }

    pub fn format_validation(results: &[(String, ValidationResult)]) -> String {
        let mut out = String::new();
        use std::fmt::Write;

        let _ = writeln!(
            out,
            "╔══════════════════════════════════════════════╗"
        );
        let _ = writeln!(
            out,
            "║           VALIDATION RESULTS                  ║"
        );
        let _ = writeln!(
            out,
            "╚══════════════════════════════════════════════╝"
        );
        let _ = writeln!(out);

        let valid_count = results.iter().filter(|r| r.1.valid).count();
        let total = results.len();
        let _ = writeln!(out, "  {}/{} proposals valid", valid_count, total);

        for (id, result) in results {
            let status = if result.valid { "✓ VALID" } else { "✗ INVALID" };
            let _ = writeln!(out, "  [{}] {}", id, status);

            for err in &result.errors {
                let field_str = err
                    .field
                    .as_ref()
                    .map(|f| format!(" (field: {})", f))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "    ERR  [{}]{} {}",
                    match err.severity {
                        Severity::Critical => "CRIT",
                        Severity::High => "HIGH",
                        Severity::Medium => "MED",
                        Severity::Low => "LOW",
                        Severity::Info => "INFO",
                    },
                    field_str,
                    err.message
                );
            }

            for warn in &result.warnings {
                let field_str = warn
                    .field
                    .as_ref()
                    .map(|f| format!(" (field: {})", f))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "    WARN [{}]{} {}",
                    match warn.severity {
                        Severity::Critical => "CRIT",
                        Severity::High => "HIGH",
                        Severity::Medium => "MED",
                        Severity::Low => "LOW",
                        Severity::Info => "INFO",
                    },
                    field_str,
                    warn.message
                );
            }
        }

        let _ = writeln!(
            out,
            "────────────────────────────────────────────"
        );
        out
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub severity: Severity,
    pub message: String,
    pub field: Option<String>,
}
