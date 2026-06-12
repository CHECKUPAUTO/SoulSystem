//! Auto-PR: full pull request lifecycle — branch, commit, push, PR creation.

use std::path::Path;
use tracing::{info, warn};

pub struct AutoPr {
    github_token: String,
    owner: String,
    repo: String,
    base_branch: String,
}

impl Default for AutoPr {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoPr {
    pub fn new() -> Self {
        let github_token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .unwrap_or_default();
        let owner = "CHECKUPAUTO".to_string();
        let repo = "openevolve".to_string();
        let base_branch = "main".to_string();
        Self {
            github_token,
            owner,
            repo,
            base_branch,
        }
    }

    /// Apply a git diff to a local repository.
    pub fn apply_patch(diff: &str, repo: &Path) -> anyhow::Result<()> {
        let patch_path = repo.join(".openevolve_auto.patch");
        std::fs::write(&patch_path, diff)?;
        let output = std::process::Command::new("git")
            .args(["apply", patch_path.to_str().unwrap()])
            .current_dir(repo)
            .output()?;
        if output.status.success() {
            info!("Auto-PR patch applied successfully");
            let _ = std::fs::remove_file(&patch_path);
            Ok(())
        } else {
            anyhow::bail!(
                "git apply failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        }
    }

    /// Create a full PR: branch → commit → push → GitHub PR API.
    pub async fn create_pr(
        &self,
        program_code: &str,
        score: f64,
        target_path: &str,
    ) -> anyhow::Result<String> {
        if score < 0.8 {
            anyhow::bail!("Score too low for PR: {:.2} < 0.8", score);
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let branch = format!("evolve/improvement-{:016x}", timestamp);
        let title = format!("Auto: improve fitness to {:.3}", score);

        // Phase 1: write code locally
        let local_path = Path::new(target_path);
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(local_path, program_code)?;

        // Phase 2: git commands
        let status = std::process::Command::new("git")
            .args(["checkout", "-b", &branch])
            .output()?;
        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            if !stderr.contains("already exists") {
                anyhow::bail!("git checkout -b failed: {}", stderr);
            }
            // Branch exists, just switch
            std::process::Command::new("git")
                .args(["checkout", &branch])
                .output()?;
        }

        std::fs::write(local_path, program_code)?;

        let add = std::process::Command::new("git")
            .args(["add", target_path])
            .output()?;
        if !add.status.success() {
            anyhow::bail!("git add failed: {}", String::from_utf8_lossy(&add.stderr));
        }

        let commit = std::process::Command::new("git")
            .args(["commit", "-m", &title, "--allow-empty"])
            .output()?;
        if !commit.status.success() {
            anyhow::bail!(
                "git commit failed: {}",
                String::from_utf8_lossy(&commit.stderr)
            );
        }

        let push = std::process::Command::new("git")
            .args(["push", "origin", &branch])
            .output()?;
        if !push.status.success() {
            anyhow::bail!("git push failed: {}", String::from_utf8_lossy(&push.stderr));
        }

        // Phase 3: GitHub PR API
        if self.github_token.is_empty() {
            warn!("GITHUB_TOKEN not set, skipping PR creation");
            std::process::Command::new("git")
                .args(["checkout", &self.base_branch])
                .output()?;
            return Ok(format!("Branch pushed: {}", branch));
        }

        let pr_body = format!(
            "## Évolution automatique\n\n\
             Score atteint : **{score:.3}**\n\n\
             Cette PR a été générée par OpenEvolve v4.0 — optimisation \
             de code par évolution guidée par LLM.\n\n\
             ### Changements\n\
             - `{target_path}` : amélioration du score à {score:.3}\n\n\
             Vérifier manuellement avant merge."
        );

        let client = reqwest::Client::new();
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls",
            self.owner, self.repo
        );
        let body = serde_json::json!({
            "title": title,
            "body": pr_body,
            "head": branch,
            "base": self.base_branch,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.github_token))
            .header("User-Agent", "OpenEvolve/4.3")
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            let pr: serde_json::Value = resp.json().await?;
            let pr_url = pr["html_url"].as_str().unwrap_or(&url).to_string();
            info!("PR created: {}", pr_url);

            // Return to base branch
            std::process::Command::new("git")
                .args(["checkout", &self.base_branch])
                .output()?;

            Ok(pr_url)
        } else {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error: {}", text)
        }
    }

    // ── v4.3: PR on an external target repository ──────────────────────

    /// Create a PR on a *target* repository (not OpenEvolve itself).
    ///
    /// `repo_dir` must point to a local clone of the target repo. The function:
    ///   1. parses the `origin` remote URL to discover `owner/repo`,
    ///   2. creates a feature branch,
    ///   3. writes `improved_code` to the relative `target_path` inside the repo,
    ///   4. commits, pushes, opens a PR through the GitHub API,
    ///   5. attaches a rich Markdown body with before/after metrics.
    ///
    /// Requires `GITHUB_TOKEN` (or `GH_TOKEN`) in the environment for the API
    /// call. If absent, returns `Ok(branch_name)` after pushing the branch so
    /// the user can open the PR manually.
    pub async fn create_pr_on_target(
        &self,
        repo_dir: &Path,
        target_path: &str,
        improved_code: &str,
        report: &TargetPrReport,
    ) -> anyhow::Result<String> {
        let (owner, repo) = parse_origin_owner_repo(repo_dir)?;
        let branch = format!(
            "openevolve/{}-{:08x}",
            sanitize_branch(target_path),
            random_suffix()
        );
        let title = format!(
            "OpenEvolve: {target_path} score {:.4} → {:.4}",
            report.score_before, report.score_after
        );

        // Stage 1: filesystem
        let abs_target = repo_dir.join(target_path);
        if let Some(parent) = abs_target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs_target, improved_code)?;

        // Stage 2: git
        run_git(repo_dir, &["checkout", "-b", &branch])
            .or_else(|_| run_git(repo_dir, &["checkout", &branch]))?;
        run_git(repo_dir, &["add", target_path])?;
        run_git(
            repo_dir,
            &[
                "-c",
                "user.email=openevolve@cortexsynapse.local",
                "-c",
                "user.name=OpenEvolve",
                "commit",
                "-m",
                &title,
                "--allow-empty",
            ],
        )?;
        run_git(repo_dir, &["push", "-u", "origin", &branch])?;

        if self.github_token.is_empty() {
            warn!("GITHUB_TOKEN absent — branch poussée, PR à ouvrir manuellement");
            return Ok(format!("branch:{branch}"));
        }

        // Stage 3: GitHub API
        let body = render_target_pr_body(report);
        let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
        let payload = serde_json::json!({
            "title": title,
            "body": body,
            "head": branch,
            "base": self.base_branch,
        });
        let resp = reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.github_token))
            .header("User-Agent", "OpenEvolve/4.3")
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error: {text}");
        }
        let pr: serde_json::Value = resp.json().await?;
        let pr_url = pr["html_url"].as_str().unwrap_or(&url).to_string();
        info!(%pr_url, "PR opened on target repository");
        Ok(pr_url)
    }
}

// ── Helpers for target-repo PR ──────────────────────────────────────────

/// Structured payload that the engine fills in before opening the PR.
#[derive(Debug, Clone, Default)]
pub struct TargetPrReport {
    pub score_before: f64,
    pub score_after: f64,
    pub iterations: u32,
    pub language: String,
    pub elapsed_secs: f64,
    pub loc_before: usize,
    pub loc_after: usize,
    pub complexity_before: String,
    pub complexity_after: String,
    pub provider_costs: Vec<(String, f64)>,
}

fn render_target_pr_body(r: &TargetPrReport) -> String {
    let mut s = String::new();
    s.push_str("## 🧬 OpenEvolve — résultats\n\n");
    s.push_str(&format!(
        "| Métrique | Avant | Après |\n|---|---|---|\n\
         | Score   | {:.4} | **{:.4}** |\n\
         | LOC     | {} | {} |\n\
         | Classe  | {} | {} |\n\n",
        r.score_before,
        r.score_after,
        r.loc_before,
        r.loc_after,
        r.complexity_before,
        r.complexity_after,
    ));
    s.push_str(&format!(
        "**Itérations** : {}  |  **Durée** : {:.1}s  |  **Langage** : {}\n\n",
        r.iterations, r.elapsed_secs, r.language,
    ));
    if !r.provider_costs.is_empty() {
        s.push_str("### Coût LLM\n\n| Provider | USD |\n|---|---|\n");
        for (p, c) in &r.provider_costs {
            s.push_str(&format!("| {p} | ${c:.4} |\n"));
        }
        s.push('\n');
    }
    s.push_str(
        "_PR générée par OpenEvolve v4.3. Vérifie les tests avant merge — \
         OpenEvolve sait optimiser le score de l'évaluateur, pas \
         garantir la correction sémantique sur des cas non testés._\n",
    );
    s
}

fn parse_origin_owner_repo(repo_dir: &Path) -> anyhow::Result<(String, String)> {
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_dir)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git remote get-url origin failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Supports: git@github.com:owner/repo.git  and  https://github.com/owner/repo(.git)
    let cleaned = url
        .trim_end_matches(".git")
        .replace("git@github.com:", "https://github.com/");
    let after = cleaned
        .split_once("github.com/")
        .map(|x| x.1)
        .ok_or_else(|| anyhow::anyhow!("not a github remote: {url}"))?;
    let mut it = after.splitn(2, '/');
    let owner = it.next().unwrap_or("").to_string();
    let repo = it.next().unwrap_or("").to_string();
    if owner.is_empty() || repo.is_empty() {
        anyhow::bail!("could not parse owner/repo from {url}");
    }
    Ok((owner, repo))
}

fn sanitize_branch(p: &str) -> String {
    p.replace(['/', '\\', ' '], "-")
        .replace("..", "")
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(50)
        .collect()
}

fn random_suffix() -> u32 {
    use rand::Rng;
    rand::thread_rng().gen()
}

fn run_git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

#[cfg(test)]
mod target_pr_tests {
    use super::*;

    #[test]
    fn parses_https_origin() {
        // We can't run git in a tempdir trivially, so test the URL splitter directly.
        let url = "https://github.com/CHECKUPAUTO/openevolve.git";
        let cleaned = url
            .trim_end_matches(".git")
            .replace("git@github.com:", "https://github.com/");
        let after = cleaned.split_once("github.com/").unwrap().1;
        let mut it = after.splitn(2, '/');
        assert_eq!(it.next(), Some("CHECKUPAUTO"));
        assert_eq!(it.next(), Some("openevolve"));
    }

    #[test]
    fn parses_ssh_origin() {
        let url = "git@github.com:CHECKUPAUTO/avid.git";
        let cleaned = url
            .trim_end_matches(".git")
            .replace("git@github.com:", "https://github.com/");
        let after = cleaned.split_once("github.com/").unwrap().1;
        let mut it = after.splitn(2, '/');
        assert_eq!(it.next(), Some("CHECKUPAUTO"));
        assert_eq!(it.next(), Some("avid"));
    }

    #[test]
    fn branch_sanitization() {
        assert_eq!(sanitize_branch("src/foo/bar.rs"), "src-foo-bar.rs");
        assert_eq!(sanitize_branch("a..b"), "ab");
    }

    #[test]
    fn renders_markdown_body() {
        let r = TargetPrReport {
            score_before: 0.42,
            score_after: 0.87,
            iterations: 150,
            language: "rust".into(),
            elapsed_secs: 304.5,
            loc_before: 30,
            loc_after: 18,
            complexity_before: "O(n²)".into(),
            complexity_after: "O(n log n)".into(),
            provider_costs: vec![("ollama".into(), 0.0), ("anthropic".into(), 0.27)],
        };
        let md = render_target_pr_body(&r);
        assert!(md.contains("0.4200"));
        assert!(md.contains("0.8700"));
        assert!(md.contains("O(n log n)"));
        assert!(md.contains("$0.2700"));
    }
}
