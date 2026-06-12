use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path;

use std::sync::Arc;
use std::time::SystemTime;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use crate::metadata::{parse_skill_file, Capability, Skill};

/// A registry of loaded skills, with name-based and capability-based lookup.
///
/// Skills are stored in a `HashMap` for O(1) name lookup, with a secondary
/// index mapping each [`Capability`] to the set of skill names that provide it.
///
/// # Examples
///
/// ```rust,no_run
/// # use avid_skills::SkillRegistry;
/// # use std::path::Path;
/// #
/// let mut registry = SkillRegistry::new();
/// registry.load_from_dir(Path::new("skills")).unwrap();
///
/// let code_reviewers = registry.find_by_capability(&avid_skills::Capability::CodeReview);
/// println!("Available reviewers: {}", code_reviewers.len());
/// ```
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    by_capability: HashMap<Capability, Vec<String>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Create an empty skill registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            by_capability: HashMap::new(),
        }
    }

    /// Load all `SKILL.md` files recursively from a directory.
    ///
    /// Walks the directory tree, finds all `SKILL.md` files, parses them,
    /// and registers them in the registry. Duplicate skill names are skipped
    /// with a warning.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The directory does not exist or cannot be read
    /// - Any `SKILL.md` file fails to parse
    ///
    /// # Returns
    ///
    /// The number of skills successfully loaded.
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<usize, SkillRegistryError> {
        let mut count = 0;

        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.file_name() != "SKILL.md" {
                continue;
            }

            let skill = parse_skill_file(entry.path())
                .map_err(|e| SkillRegistryError::Parse(e.to_string()))?;

            let name = skill.metadata.name.clone();
            debug!("Loaded skill: {} from {}", name, entry.path().display());

            if self.skills.contains_key(&name) {
                warn!(
                    "Duplicate skill '{name}' at {}, skipping",
                    entry.path().display()
                );
                continue;
            }

            self.register(skill)?;
            count += 1;
        }

        Ok(count)
    }

    /// Register a skill in the registry.
    ///
    /// Indexes the skill by both its name and its capabilities. Returns an
    /// error if a skill with the same name is already registered (use
    /// [`reload`](Self::reload) to replace).
    ///
    /// # Errors
    ///
    /// Returns `SkillRegistryError::Duplicate` if the name already exists.
    pub fn register(&mut self, skill: Skill) -> Result<(), SkillRegistryError> {
        let name = skill.metadata.name.clone();

        if self.skills.contains_key(&name) {
            return Err(SkillRegistryError::Duplicate { name });
        }

        // Index by capability
        for cap in &skill.metadata.capabilities {
            self.by_capability
                .entry(cap.clone())
                .or_default()
                .push(name.clone());
        }

        self.skills.insert(name, skill);
        Ok(())
    }

    /// Look up a skill by its name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Find all skills tagged with a given capability.
    #[must_use]
    pub fn find_by_capability(&self, cap: &Capability) -> Vec<&Skill> {
        self.by_capability
            .get(cap)
            .map(|names| names.iter().filter_map(|n| self.skills.get(n)).collect())
            .unwrap_or_default()
    }

    /// List all registered skills.
    pub fn list_all(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Clear the registry and reload all skills from a directory.
    ///
    /// This is a full reset — the old index is discarded entirely.
    ///
    /// # Errors
    ///
    /// Errors propagate from [`load_from_dir`](Self::load_from_dir).
    ///
    /// # Returns
    ///
    /// The number of skills (re)loaded.
    pub fn reload(&mut self, dir: &Path) -> Result<usize, SkillRegistryError> {
        self.skills.clear();
        self.by_capability.clear();
        self.load_from_dir(dir)
    }

    /// Return the number of registered skills.
    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Return `true` if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum SkillRegistryError {
    #[error("Duplicate skill name: {name}")]
    Duplicate { name: String },

    #[error("Skill parse error: {0}")]
    Parse(String),
}

// ── Hot-reload (polling) ────────────────────────────────────

/// Snapshot of a single SKILL.md file on disk.
#[derive(Debug, Clone)]
pub struct SkillSnapshot {
    pub path: std::path::PathBuf,
    pub mtime: SystemTime,
}

impl SkillRegistry {
    /// Scan the directory tree and return current metadata for every
    /// `SKILL.md` without actually loading them.
    pub fn scan_snapshot(
        &self,
        dir: &std::path::Path,
    ) -> Result<Vec<SkillSnapshot>, SkillRegistryError> {
        let mut snaps = Vec::new();
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
                continue;
            }
            let meta = entry
                .metadata()
                .map_err(|e| SkillRegistryError::Parse(e.to_string()))?;
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            snaps.push(SkillSnapshot {
                path: entry.path().to_path_buf(),
                mtime,
            });
        }
        Ok(snaps)
    }
}

/// Spawn a background Tokio task that polls the skill directory and
/// reloads the registry whenever a `SKILL.md` file changes.
///
/// The caller must hold `registry` inside an `Arc<tokio::sync::RwLock<SkillRegistry>>`
/// so that the background task can acquire a write lock safely.
///
/// # Example
/// ```no_run
/// # use avid_skills::SkillRegistry;
/// # use std::path::Path;
/// # #[tokio::main]
/// # async fn main() {
/// let mut registry = SkillRegistry::new();
/// registry.load_from_dir(Path::new("skills")).unwrap();
/// let handle = avid_skills::spawn_hot_reload(
///     std::sync::Arc::new(tokio::sync::RwLock::new(registry)),
///     Path::new("skills").to_path_buf(),
///     5,
/// );
/// // handle.abort() when done
/// # }
/// ```
pub fn spawn_hot_reload(
    registry: Arc<tokio::sync::RwLock<SkillRegistry>>,
    dir: std::path::PathBuf,
    period_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_snapshot: std::collections::HashMap<std::path::PathBuf, SystemTime> =
            std::collections::HashMap::new();
        let mut ticker = interval(Duration::from_secs(period_secs));
        loop {
            ticker.tick().await;
            let current = {
                let reg = registry.read().await;
                match reg.scan_snapshot(&dir) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("hot-reload scan failed: {}", e);
                        continue;
                    }
                }
            };

            let mut changed = false;
            for snap in &current {
                match last_snapshot.get(&snap.path) {
                    Some(&prev) if prev == snap.mtime => {}
                    _ => {
                        changed = true;
                        break;
                    }
                }
            }
            if !changed && current.len() != last_snapshot.len() {
                changed = true;
            }

            if changed {
                info!("SKILL.md change detected — hot-reloading registry");
                let reload_result = {
                    let mut reg = registry.write().await;
                    reg.reload(&dir)
                };

                match reload_result {
                    Ok(count) => info!("hot-reload complete — {count} skills active"),
                    Err(e) => warn!("hot-reload failed: {e}"),
                }
                last_snapshot = current.into_iter().map(|s| (s.path, s.mtime)).collect();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::SkillMetadata;

    fn make_skill(name: &str, caps: Vec<Capability>) -> Skill {
        Skill {
            metadata: SkillMetadata {
                name: name.to_string(),
                version: "1.0.0".into(),
                description: "Test".into(),
                author: "Test".into(),
                capabilities: caps,
                tools_allowed: vec![],
                model_preference: None,
                max_tokens: None,
                timeout_seconds: None,
            },
            system_prompt: "Test prompt".into(),
            source_path: Path::new("test").join(name).join("SKILL.md"),
        }
    }

    #[test]
    fn test_register_and_lookup() {
        let mut reg = SkillRegistry::new();
        let skill = make_skill("reviewer", vec![Capability::CodeReview]);

        reg.register(skill).unwrap();
        assert_eq!(reg.len(), 1);
        assert!(reg.get("reviewer").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_find_by_capability() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("r1", vec![Capability::CodeReview]))
            .unwrap();
        reg.register(make_skill(
            "r2",
            vec![Capability::CodeReview, Capability::Analysis],
        ))
        .unwrap();
        reg.register(make_skill("p1", vec![Capability::Planning]))
            .unwrap();

        let reviewers = reg.find_by_capability(&Capability::CodeReview);
        assert_eq!(reviewers.len(), 2);

        let planners = reg.find_by_capability(&Capability::Planning);
        assert_eq!(planners.len(), 1);

        let docs = reg.find_by_capability(&Capability::Documentation);
        assert!(docs.is_empty());
    }

    #[test]
    fn test_list_all() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("a", vec![])).unwrap();
        reg.register(make_skill("b", vec![])).unwrap();
        assert_eq!(reg.list_all().len(), 2);
    }

    #[test]
    fn test_duplicate_rejected() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("dup", vec![])).unwrap();
        let err = reg.register(make_skill("dup", vec![])).unwrap_err();
        assert!(matches!(err, SkillRegistryError::Duplicate { name } if name == "dup"));
    }

    #[test]
    fn test_load_from_dir() {
        let tmp = tempfile::TempDir::with_prefix("avid-skills-test").unwrap();
        let review_dir = tmp.path().join("code-review");
        std::fs::create_dir_all(&review_dir).unwrap();
        std::fs::write(
            review_dir.join("SKILL.md"),
            r#"---
name: code-review
version: "1.0.0"
description: "Review code"
author: "AVID"
capabilities:
  - CodeReview
---

# Review
Do it.
"#,
        )
        .unwrap();

        let mut reg = SkillRegistry::new();
        let count = reg.load_from_dir(tmp.path()).unwrap();
        assert_eq!(count, 1);
        assert!(reg.get("code-review").is_some());
    }

    #[test]
    fn test_reload_clears() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("old", vec![Capability::CodeReview]))
            .unwrap();

        let tmp = tempfile::TempDir::with_prefix("avid-skills-test").unwrap();
        let dir = tmp.path().join("planning");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            r#"---
name: planning
version: "1.0.0"
description: "Plan"
author: "AVID"
capabilities:
  - Planning
---

Plan it.
"#,
        )
        .unwrap();

        let count = reg.reload(tmp.path()).unwrap();
        assert_eq!(count, 1);
        assert!(reg.get("old").is_none());
        assert!(reg.get("planning").is_some());
    }
}
