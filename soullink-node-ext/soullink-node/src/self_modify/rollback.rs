use std::path::Path;
use anyhow::{Context, Result};
use tracing::{info, warn, error};

use crate::self_modify::{
    patch_generator::PatchCandidate,
    telemetry::{EvolutionEvent, Telemetry},
};

/// Apply a candidate patch, run validation, and rollback on failure.
pub async fn apply_or_rollback(
    candidate: &PatchCandidate,
    workspace: &Path,
    telemetry: &Telemetry,
) -> Result<()> {
    let target = workspace.join("src").join("lib.rs");
    let backup = target.with_extension("rs.bak");

    // Stash current state
    if target.exists() {
        std::fs::copy(&target, &backup)
            .with_context(|| format!("backup {} -> {}", target.display(), backup.display()))?;
    }

    // Atomic write
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, &candidate.diff)
        .with_context(|| format!("write tmp {}", tmp.display()))?;
    std::fs::rename(&tmp, &target)
        .with_context(|| format!("rename {} -> {}", tmp.display(), target.display()))?;

    info!("Patch applied: {}", candidate.id);

    // Validation
    if let Err(e) = run_cargo_check(workspace).await {
        warn!("Validation failed for {}: {}", candidate.id, e);
        restore_backup(&backup, &target)?;
        telemetry.emit(EvolutionEvent::RolledBack {
            id: candidate.id,
            reason: format!("cargo check: {}", e),
        });
        anyhow::bail!("rollback after validation failure: {}", e);
    }

    Ok(())
}

async fn run_cargo_check(workspace: &Path) -> Result<()> {
    let out = tokio::process::Command::new("cargo")
        .args(["check", "--offline"])
        .current_dir(workspace)
        .output()
        .await
        .context("cargo check failed to start")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        error!("cargo check --offline failed:\n{}", stderr);
        anyhow::bail!("cargo check failed:\n{}", stderr);
    }
    Ok(())
}

fn restore_backup(backup: &std::path::Path, target: &std::path::Path) -> Result<()> {
    if backup.exists() {
        std::fs::copy(backup, target).context("restore backup")?;
    }
    Ok(())
}
