//! Analyse git : matrice de co-commits par dépôt.

use anyhow::Result;
use git2::{Repository, Sort};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

#[derive(Debug, Clone, Default)]
pub struct CoCommitMatrix {
    /// (file_a, file_b) → nb de commits qui les touchent ensemble (a <= b lexico).
    pub pairs: HashMap<(String, String), u32>,
    /// Nb de commits par fichier.
    pub freq: HashMap<String, u32>,
}

impl CoCommitMatrix {
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty() && self.freq.is_empty()
    }

    pub fn jaccard(&self, a: &str, b: &str) -> f32 {
        let (k1, k2) = if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
        let inter = *self.pairs.get(&(k1, k2)).unwrap_or(&0) as f32;
        let ua = *self.freq.get(a).unwrap_or(&0) as f32;
        let ub = *self.freq.get(b).unwrap_or(&0) as f32;
        let union = (ua + ub - inter).max(1.0);
        inter / union
    }

    pub fn top_pairs(&self, min_jaccard: f32, limit: usize) -> Vec<(String, String, f32)> {
        let mut v: Vec<(String, String, f32)> = self
            .pairs
            .iter()
            .filter_map(|((a, b), _)| {
                let j = self.jaccard(a, b);
                if j >= min_jaccard {
                    Some((a.clone(), b.clone(), j))
                } else {
                    None
                }
            })
            .collect();
        v.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        v.truncate(limit);
        v
    }

    pub fn merge(&mut self, other: CoCommitMatrix) {
        for ((a, b), c) in other.pairs {
            *self.pairs.entry((a, b)).or_insert(0) += c;
        }
        for (f, c) in other.freq {
            *self.freq.entry(f).or_insert(0) += c;
        }
    }
}

pub fn analyze_repo(repo_path: &Path, window: usize) -> Result<CoCommitMatrix> {
    let mut mtx = CoCommitMatrix::default();
    let repo = match Repository::open(repo_path) {
        Ok(r) => r,
        Err(e) => {
            debug!(path = %repo_path.display(), error = %e, "pas un repo git, skip");
            return Ok(mtx);
        }
    };

    let mut walker = repo.revwalk()?;
    walker.set_sorting(Sort::TIME)?;
    walker.push_head()?;

    let mut count = 0usize;
    for oid in walker {
        if count >= window {
            break;
        }
        let oid = match oid {
            Ok(o) => o,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(oid) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Collect changed files vs first parent.
        let tree = match commit.tree() { Ok(t) => t, Err(_) => continue };
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
        let diff = match repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let mut files: Vec<String> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(p) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                    if let Some(s) = p.to_str() {
                        files.push(format!("{}/{}", repo_path.display(), s));
                    }
                }
                true
            },
            None, None, None,
        ).ok();

        for f in &files {
            *mtx.freq.entry(f.clone()).or_insert(0) += 1;
        }
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let (a, b) = if files[i] <= files[j] {
                    (files[i].clone(), files[j].clone())
                } else {
                    (files[j].clone(), files[i].clone())
                };
                *mtx.pairs.entry((a, b)).or_insert(0) += 1;
            }
        }
        count += 1;
    }

    Ok(mtx)
}
