use crate::self_modify::patch_generator::PatchCandidate;

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
}

/// Trait for patch validators.
pub trait Validator {
    fn validate(&self, candidate: &PatchCandidate) -> anyhow::Result<ValidationReport>;
}

/// Cargo-based validator (check + test offline).
pub struct CargoValidator {
    workspace: std::path::PathBuf,
    config: ValidatorConfig,
}

impl CargoValidator {
    /// Create a new Cargo-based validator.
    pub fn new(workspace: impl Into<std::path::PathBuf>, config: ValidatorConfig) -> Self {
        Self { workspace: workspace.into(), config }
    }
}

impl Validator for CargoValidator {
    fn validate(&self, _candidate: &PatchCandidate) -> anyhow::Result<ValidationReport> {
        // TODO: apply patch to worktree, run cargo check/test
        Ok(ValidationReport {
            passed: true,
            build_ok: true,
            tests_ok: self.config.require_tests,
            lint_ok: true,
            warnings: Vec::new(),
        })
    }
}
