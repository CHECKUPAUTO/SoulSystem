//! Actions GitHub : commit local + push optionnel d'un repo de proposals.
//!
//! Le repo `CHECKUPAUTO/SYNERGIE-PROPOSALS` (path local configurable) reçoit
//! les scaffoldings générés par `codegen` (cf. agent.rs). On reste prudent :
//! pas de push si `dry_run`.

use crate::types::Synergy;
use anyhow::{Context, Result};
use git2::{IndexAddOption, Repository, Signature};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Génère un scaffolding (proposal.md + crate squelette si dedup) dans
/// `proposals_dir/<synergy_id>/` et renvoie les chemins écrits.
pub fn write_scaffolding(
    synergy: &Synergy,
    proposals_dir: &Path,
    dry_run: bool,
) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let dir = proposals_dir.join(&synergy.id);
    if !dry_run {
        std::fs::create_dir_all(&dir)?;
    }

    let proposal_path = dir.join("proposal.md");
    let body = format!(
        "# Proposal — {}\n\n- **ID** : `{id}`\n- **Type** : `{ty}`\n- **Score** : {score:.2}\n- **From → To** : `{a}` → `{b}`\n- **Effort** : {eff} | **Value** : {val}\n\n## Description\n\n{desc}\n\n## Action suggérée\n\n{act}\n\n## Evidence\n\n{ev}\n",
        synergy.description.lines().next().unwrap_or(""),
        id = synergy.id,
        ty = synergy.synergy_type,
        score = synergy.adjusted_score,
        a = synergy.source_project,
        b = synergy.target_project,
        eff = synergy.effort,
        val = synergy.value,
        desc = synergy.description,
        act = synergy.suggested_action.as_deref().unwrap_or("(non spécifié)"),
        ev = synergy.evidence.iter().take(10).map(|e| format!(
            "- `{}:{}` — {}", e.path.display(),
            e.line.map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
            e.fragment
        )).collect::<Vec<_>>().join("\n"),
    );
    if !dry_run {
        std::fs::write(&proposal_path, &body)?;
    }
    out.push(proposal_path);

    // Pour Deduplication : on crée aussi un squelette commons-<hash>/.
    if synergy.synergy_type == "Deduplication" {
        let crate_name = format!("commons-{}", &synergy.id[..synergy.id.len().min(6)]);
        let cdir = dir.join(&crate_name);
        let src = cdir.join("src");
        let cargo = format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = {{ version = \"1\", features = [\"derive\"] }}\n",
            crate_name.replace('-', "_")
        );
        let lib = format!(
            "//! {name}\n//!\n//! Crate factorisée par SYNERGIE (id={id}).\n//! NOTE: déplacer ici l'impl consolidée depuis les sources.\n",
            name = crate_name, id = synergy.id
        );
        if !dry_run {
            std::fs::create_dir_all(&src)?;
            std::fs::write(cdir.join("Cargo.toml"), cargo)?;
            std::fs::write(src.join("lib.rs"), lib)?;
        }
        out.push(cdir.join("Cargo.toml"));
        out.push(src.join("lib.rs"));
    }

    Ok(out)
}

/// Commit local des proposals dans `repo_path` sur une branche `synergie/auto/<id>`.
/// Renvoie le SHA. Ne pushe pas (cf. `push`).
pub fn commit_proposal(
    repo_path: &Path,
    branch_prefix: &str,
    synergy: &Synergy,
    staged_files: &[PathBuf],
) -> Result<String> {
    let repo = Repository::discover(repo_path).context("discover repo")?;
    let head = repo.head().context("head")?;
    let head_commit = head.peel_to_commit().context("head→commit")?;

    let branch_name = format!("{}{}", branch_prefix, synergy.id);
    let branch_ref = match repo.find_branch(&branch_name, git2::BranchType::Local) {
        Ok(b) => b.into_reference(),
        Err(_) => repo
            .branch(&branch_name, &head_commit, false)
            .context("branch create")?
            .into_reference(),
    };
    let ref_name = branch_ref
        .name()
        .map(|s| s.to_string())
        .context("branch name")?;
    repo.set_head(&ref_name).context("set_head")?;

    let mut index = repo.index().context("index")?;
    let workdir = repo.workdir().context("repo bare non supporté")?;
    for f in staged_files {
        if let Ok(rel) = f.strip_prefix(workdir) {
            let _ = index.add_path(rel);
        } else {
            warn!(target: "github", path = %f.display(), "fichier hors workdir — skip");
        }
    }
    index.add_all(["."], IndexAddOption::DEFAULT, None).ok();
    index.write().context("write index")?;
    let tree_id = index.write_tree().context("write_tree")?;
    let tree = repo.find_tree(tree_id).context("find_tree")?;

    let sig = Signature::now("synergie-agent", "synergie@localhost").context("sig")?;
    let msg = format!(
        "synergie({}): {}\n\nGénéré automatiquement par SYNERGIE v3.\nReview avant push.",
        synergy.id,
        synergy.description.lines().next().unwrap_or("")
    );

    let oid = repo
        .commit(Some(&ref_name), &sig, &sig, &msg, &tree, &[&head_commit])
        .context("commit")?;
    info!(target: "github", branch = %branch_name, sha = %oid, "commit OK");
    Ok(oid.to_string())
}

/// Push de la branche courante vers le remote `remote` (défaut "origin").
pub fn push(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    let repo = Repository::discover(repo_path)?;
    let mut remote = repo.find_remote(remote)?;
    let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);
    remote.push(&[refspec.as_str()], None).context("push")?;
    info!(target: "github", branch = %branch, "push OK");
    Ok(())
}
