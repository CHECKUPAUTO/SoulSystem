//! Skill induction — synthesise a reusable [`Skill`] from a successful
//! execution trace.
//!
//! When the agent completes a task, the sequence of tool calls and the goal
//! that produced it are a reusable procedure. [`SkillInducer`] turns that
//! [`Episode`] into a [`Skill`]: the goal becomes the description, significant
//! goal words become triggers, the tools used become `tools_required`, and the
//! action descriptions become ordered steps.
//!
//! Induction is heuristic and deterministic (no LLM call) so it is cheap to run
//! after every task and trivial to test. When the new procedure overlaps an
//! existing skill, induction *reinforces* that skill instead of producing a
//! near-duplicate.

use serde::{Deserialize, Serialize};

use crate::Skill;

/// One tool invocation within an [`Episode`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeAction {
    /// Tool that was invoked (e.g. `read_file`, `shell`, `grep`).
    pub tool: String,
    /// Human-readable description of what the step accomplished.
    pub description: String,
}

impl EpisodeAction {
    pub fn new(tool: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            description: description.into(),
        }
    }
}

/// A record of one completed task — the raw material for induction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// The goal/request the agent was working on.
    pub goal: String,
    /// The ordered tool calls the agent made.
    pub actions: Vec<EpisodeAction>,
    /// Whether the task completed successfully.
    pub success: bool,
    /// Optional one-line summary of the outcome (used as an example).
    #[serde(default)]
    pub result_summary: Option<String>,
}

impl Episode {
    pub fn new(goal: impl Into<String>, actions: Vec<EpisodeAction>, success: bool) -> Self {
        Self {
            goal: goal.into(),
            actions,
            success,
            result_summary: None,
        }
    }
}

/// The result of attempting to induce a skill from an episode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Induction {
    /// A brand-new skill was synthesised.
    New(Skill),
    /// An existing skill was reinforced (extra example, unioned tools, bumped
    /// priority). Carries the updated skill.
    Reinforced(Skill),
    /// Nothing was induced; the string explains why.
    Skipped(&'static str),
}

/// Configuration for [`SkillInducer`].
#[derive(Debug, Clone)]
pub struct InducerConfig {
    /// Minimum number of actions an episode must have to be worth a skill.
    /// One-step tasks aren't procedures worth saving.
    pub min_actions: usize,
    /// Default priority assigned to newly induced skills (kept below the
    /// hand-authored builtins, which sit at 7–9).
    pub default_priority: u8,
    /// Maximum number of significant words used to build the skill name.
    pub max_name_words: usize,
    /// Maximum number of triggers extracted from the goal.
    pub max_triggers: usize,
}

impl Default for InducerConfig {
    fn default() -> Self {
        Self {
            min_actions: 2,
            default_priority: 4,
            max_name_words: 4,
            max_triggers: 6,
        }
    }
}

/// Synthesises [`Skill`]s from [`Episode`]s.
#[derive(Debug, Clone, Default)]
pub struct SkillInducer {
    config: InducerConfig,
}

impl SkillInducer {
    pub fn new(config: InducerConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &InducerConfig {
        &self.config
    }

    /// Induce a fresh skill from an episode, ignoring any existing catalogue.
    /// Returns `None` when the episode isn't worth a skill (failed, or too
    /// few actions).
    pub fn induce(&self, episode: &Episode) -> Option<Skill> {
        if !episode.success || episode.actions.len() < self.config.min_actions {
            return None;
        }

        let keywords = keywords(&episode.goal);
        let name = self.derive_name(&keywords, &episode.goal);
        let triggers: Vec<String> = keywords
            .iter()
            .take(self.config.max_triggers)
            .cloned()
            .collect();

        // Unique tools in first-seen order.
        let mut tools_required = Vec::new();
        for a in &episode.actions {
            if !a.tool.is_empty() && !tools_required.contains(&a.tool) {
                tools_required.push(a.tool.clone());
            }
        }

        let steps: Vec<String> = episode
            .actions
            .iter()
            .map(|a| a.description.clone())
            .filter(|d| !d.is_empty())
            .collect();

        let examples = episode
            .result_summary
            .clone()
            .into_iter()
            .collect::<Vec<_>>();

        Some(Skill {
            name,
            description: episode.goal.trim().to_string(),
            triggers,
            tools_required,
            model_hint: None,
            system_prompt: None,
            steps,
            examples,
            tags: vec!["induced".into()],
            priority: self.config.default_priority,
            enabled: true,
        })
    }

    /// Induce from an episode, reinforcing an existing skill when the goal
    /// already matches one rather than creating a near-duplicate.
    pub fn induce_or_merge(&self, episode: &Episode, existing: &[Skill]) -> Induction {
        let Some(candidate) = self.induce(episode) else {
            return Induction::Skipped(if !episode.success {
                "episode did not succeed"
            } else {
                "too few actions to be worth a skill"
            });
        };

        // Reinforce the first enabled existing skill whose triggers fire on the
        // goal (or which shares the induced name).
        if let Some(matched) = existing
            .iter()
            .find(|s| s.enabled && (s.name == candidate.name || s.matches_trigger(&episode.goal)))
        {
            let mut updated = matched.clone();

            // Union the tools.
            for t in &candidate.tools_required {
                if !updated.tools_required.contains(t) {
                    updated.tools_required.push(t.clone());
                }
            }
            // Add the goal as a reinforcing example (deduped).
            let example = episode.goal.trim().to_string();
            if !example.is_empty() && !updated.examples.contains(&example) {
                updated.examples.push(example);
            }
            // Bump priority (saturating, capped at 10) to reflect repeated use.
            updated.priority = updated.priority.saturating_add(1).min(10);

            return Induction::Reinforced(updated);
        }

        Induction::New(candidate)
    }

    /// Derive a kebab-case skill name from the goal's significant words,
    /// falling back to a slug of the whole goal.
    fn derive_name(&self, keywords: &[String], goal: &str) -> String {
        let from_keywords: Vec<&str> = keywords
            .iter()
            .take(self.config.max_name_words)
            .map(|s| s.as_str())
            .collect();

        let slug = if from_keywords.is_empty() {
            slugify(goal)
        } else {
            from_keywords.join("-")
        };

        if slug.is_empty() {
            "induced-skill".to_string()
        } else {
            slug
        }
    }
}

/// Words ignored when extracting triggers/keywords from a goal.
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "for", "with", "at", "by",
    "from", "up", "as", "is", "are", "was", "were", "be", "been", "it", "this", "that", "these",
    "those", "i", "we", "you", "my", "our", "your", "please", "can", "could", "would", "should",
    "do", "does", "did", "me", "him", "her", "them", "then", "so", "if", "into", "out", "all",
    "some", "any", "no", "not", "make", "get", "want", "need", "help",
];

/// Extract significant lowercase keywords from a goal, in first-seen order,
/// dropping stopwords and very short tokens.
fn keywords(goal: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for raw in goal.split(|c: char| !c.is_alphanumeric()) {
        let w = raw.to_lowercase();
        if w.len() < 3 || STOPWORDS.contains(&w.as_str()) {
            continue;
        }
        if !seen.contains(&w) {
            seen.push(w);
        }
    }
    seen
}

/// Turn arbitrary text into a kebab-case slug of its alphanumeric words.
fn slugify(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode() -> Episode {
        let mut e = Episode::new(
            "Review the auth module for security bugs",
            vec![
                EpisodeAction::new("read_file", "Read the auth module source"),
                EpisodeAction::new("grep", "Search for credential handling"),
                EpisodeAction::new("read_file", "Read the token validator"),
            ],
            true,
        );
        e.result_summary = Some("Found one missing constant-time compare".into());
        e
    }

    #[test]
    fn induce_builds_skill_from_successful_episode() {
        let inducer = SkillInducer::default();
        let skill = inducer.induce(&episode()).expect("should induce");

        assert_eq!(skill.name, "review-auth-module-security");
        assert_eq!(
            skill.description,
            "Review the auth module for security bugs"
        );
        assert!(skill.triggers.contains(&"auth".to_string()));
        assert!(skill.triggers.contains(&"security".to_string()));
        // Tools deduped, first-seen order.
        assert_eq!(skill.tools_required, vec!["read_file", "grep"]);
        assert_eq!(skill.steps.len(), 3);
        assert_eq!(
            skill.examples,
            vec!["Found one missing constant-time compare"]
        );
        assert_eq!(skill.tags, vec!["induced"]);
        assert_eq!(skill.priority, 4);
        assert!(skill.enabled);
    }

    #[test]
    fn induce_skips_failed_episode() {
        let inducer = SkillInducer::default();
        let mut e = episode();
        e.success = false;
        assert!(inducer.induce(&e).is_none());
    }

    #[test]
    fn induce_skips_trivial_episode() {
        let inducer = SkillInducer::default();
        let e = Episode::new(
            "say hello",
            vec![EpisodeAction::new("echo", "print hello")],
            true,
        );
        assert!(inducer.induce(&e).is_none());
    }

    #[test]
    fn triggers_capped_and_stopwords_removed() {
        let inducer = SkillInducer::new(InducerConfig {
            max_triggers: 2,
            ..InducerConfig::default()
        });
        let skill = inducer.induce(&episode()).unwrap();
        assert_eq!(skill.triggers.len(), 2);
        assert!(!skill.triggers.iter().any(|t| t == "the" || t == "for"));
    }

    #[test]
    fn induce_or_merge_creates_new_when_no_match() {
        let inducer = SkillInducer::default();
        let outcome = inducer.induce_or_merge(&episode(), &[]);
        match outcome {
            Induction::New(s) => assert_eq!(s.name, "review-auth-module-security"),
            other => panic!("expected New, got {other:?}"),
        }
    }

    #[test]
    fn induce_or_merge_reinforces_matching_skill() {
        let inducer = SkillInducer::default();
        let existing = vec![Skill {
            triggers: vec!["auth".into(), "security".into()],
            tools_required: vec!["read_file".into()],
            priority: 6,
            ..Skill::new("security-audit", "Audit code for security issues")
        }];

        match inducer.induce_or_merge(&episode(), &existing) {
            Induction::Reinforced(s) => {
                assert_eq!(s.name, "security-audit");
                // grep unioned in, read_file not duplicated.
                assert!(s.tools_required.contains(&"grep".to_string()));
                assert_eq!(
                    s.tools_required
                        .iter()
                        .filter(|t| *t == "read_file")
                        .count(),
                    1
                );
                // Goal added as an example, priority bumped.
                assert!(s.examples.iter().any(|e| e.contains("auth module")));
                assert_eq!(s.priority, 7);
            }
            other => panic!("expected Reinforced, got {other:?}"),
        }
    }

    #[test]
    fn priority_bump_saturates_at_ten() {
        let inducer = SkillInducer::default();
        let existing = vec![Skill {
            triggers: vec!["auth".into()],
            priority: 10,
            ..Skill::new("maxed", "already at the cap")
        }];
        match inducer.induce_or_merge(&episode(), &existing) {
            Induction::Reinforced(s) => assert_eq!(s.priority, 10),
            other => panic!("expected Reinforced, got {other:?}"),
        }
    }

    #[test]
    fn induce_or_merge_skips_with_reason() {
        let inducer = SkillInducer::default();
        let mut failed = episode();
        failed.success = false;
        assert!(matches!(
            inducer.induce_or_merge(&failed, &[]),
            Induction::Skipped("episode did not succeed")
        ));
    }

    #[test]
    fn name_falls_back_to_slug_when_all_stopwords() {
        let inducer = SkillInducer::default();
        let e = Episode::new(
            "do it for me",
            vec![
                EpisodeAction::new("shell", "run"),
                EpisodeAction::new("shell", "run again"),
            ],
            true,
        );
        let skill = inducer.induce(&e).unwrap();
        // All words are stopwords/short → slug of the whole goal.
        assert_eq!(skill.name, "do-it-for-me");
    }
}
