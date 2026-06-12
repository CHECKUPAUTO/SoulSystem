use std::path::Path;
use anyhow::Context;
use serde::Deserialize;

/// Constitutional policy loaded from `SELF_MODIFY_POLICY.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    pub max_patch_lines: usize,
    pub forbidden_strategies: Vec<String>,
    pub require_tests: bool,
    pub allow_cargo_toml: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_patch_lines: 500,
            forbidden_strategies: vec!["GeneticMutate".into(), "ModuleReorg".into()],
            require_tests: true,
            allow_cargo_toml: false,
        }
    }
}

/// Load policy from workspace root.
pub fn load_policy(workspace: &Path) -> anyhow::Result<Policy> {
    let path = workspace.join("SELF_MODIFY_POLICY.toml");
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading policy at {}", path.display()))?;
        let policy: Policy = toml::from_str(&content)
            .with_context(|| format!("parsing policy at {}", path.display()))?;
        Ok(policy)
    } else {
        Ok(Policy::default())
    }
}

/// Check a candidate against the policy.
pub fn check(candidate: &crate::self_modify::patch_generator::PatchCandidate, policy: &Policy) -> anyhow::Result<()> {
    let diff_lines = candidate.diff.lines().count();
    if diff_lines > policy.max_patch_lines {
        anyhow::bail!("patch exceeds max_patch_lines: {} > {}", diff_lines, policy.max_patch_lines);
    }
    let strategy_name = format!("{:?}", candidate.strategy);
    if policy.forbidden_strategies.contains(&strategy_name) {
        anyhow::bail!("strategy {} is forbidden", strategy_name);
    }
    if candidate.diff.contains("Cargo.toml") && !policy.allow_cargo_toml {
        anyhow::bail!("modifying Cargo.toml is forbidden");
    }
    Ok(())
}
