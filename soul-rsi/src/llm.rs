//! An LLM-backed proposer — the STOP "improver".
//!
//! [`LlmProposer`] turns any [`CodeModel`] into a [`Proposer`]: it shows the
//! model the goal, the parent's measured fitness, and the lessons from recent
//! rejections, then asks for a single, exact `find → replace` edit in a strict
//! machine-parseable envelope. Because it depends only on the `CodeModel` trait,
//! the loop stays decoupled from any specific LLM client and remains testable.

use crate::error::Result;
use crate::rng::SplitMix64;
use crate::traits::{CodeModel, ImprovementContext, Proposal, Proposer};
use crate::variant::Patch;

/// Wraps a [`CodeModel`] so it can drive the self-improvement loop.
pub struct LlmProposer<M: CodeModel> {
    model: M,
    /// Files the model is allowed to touch, relative to the workspace root.
    /// A change to anything else is dropped — the primary safety rail.
    allowed_paths: Vec<String>,
}

impl<M: CodeModel> LlmProposer<M> {
    pub fn new(model: M, allowed_paths: Vec<String>) -> Self {
        Self {
            model,
            allowed_paths,
        }
    }

    fn build_prompt(&self, ctx: &ImprovementContext<'_>) -> String {
        let fitness = ctx
            .parent_fitness
            .map(|f| {
                format!(
                    "compiles={} tests_passed={} tests_failed={} score={:.4}",
                    f.compiles, f.tests_passed, f.tests_failed, f.score
                )
            })
            .unwrap_or_else(|| "unknown".to_string());

        let mut lessons = String::new();
        if !ctx.recent_rejections.is_empty() {
            lessons.push_str("\nRecently rejected ideas (do not repeat these):\n");
            for r in ctx.recent_rejections {
                lessons.push_str(&format!("- {r}\n"));
            }
        }

        format!(
            "You are improving a Rust codebase. Goal: {goal}\n\
             Parent fitness: {fitness}\n\
             You may ONLY edit these files: {allowed}\n{lessons}\n\
             Propose ONE small, safe, compiling change. Respond EXACTLY in this format \
             and nothing else:\n\
             TARGET: <relative/path.rs>\n\
             FIND:\n<<<\n<exact text that currently exists, occurring once>\n>>>\n\
             REPLACE:\n<<<\n<the replacement text>\n>>>\n\
             RATIONALE: <one short line>\n",
            goal = ctx.goal,
            fitness = fitness,
            allowed = self.allowed_paths.join(", "),
            lessons = lessons,
        )
    }
}

impl<M: CodeModel> Proposer for LlmProposer<M> {
    fn propose(
        &self,
        ctx: &ImprovementContext<'_>,
        _rng: &mut SplitMix64,
    ) -> Result<Option<Proposal>> {
        let prompt = self.build_prompt(ctx);
        let raw = self.model.complete(&prompt)?;
        let proposal = match parse_proposal(&raw) {
            Some(p) => p,
            None => return Ok(None),
        };
        // Safety rail: never let the model escape the allow-list.
        if !self
            .allowed_paths
            .iter()
            .any(|a| a == &proposal.patch.target)
        {
            tracing::warn!(target = %proposal.patch.target, "rsi: proposal outside allow-list, dropped");
            return Ok(None);
        }
        Ok(Some(proposal))
    }
}

/// Parse the strict envelope. Returns `None` if the model went off-format —
/// the loop treats that as "no proposal", never as a crash.
fn parse_proposal(raw: &str) -> Option<Proposal> {
    let target = line_value(raw, "TARGET:")?;
    let find = block_after(raw, "FIND:")?;
    let replace = block_after(raw, "REPLACE:")?;
    let rationale = line_value(raw, "RATIONALE:").unwrap_or_else(|| "llm proposal".to_string());
    if find.is_empty() || find == replace {
        return None;
    }
    Some(Proposal {
        patch: Patch::new(target, find, replace),
        rationale,
    })
}

fn line_value(raw: &str, key: &str) -> Option<String> {
    raw.lines()
        .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Extract the text inside the `<<<` / `>>>` fence that follows `key`.
fn block_after(raw: &str, key: &str) -> Option<String> {
    let key_pos = raw.find(key)?;
    let after_key = &raw[key_pos + key.len()..];
    let open = after_key.find("<<<")?;
    let rest = &after_key[open + 3..];
    let close = rest.find(">>>")?;
    Some(rest[..close].trim_matches('\n').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedModel(String);
    impl CodeModel for FixedModel {
        fn complete(&self, _prompt: &str) -> Result<String> {
            Ok(self.0.clone())
        }
    }

    const WELL_FORMED: &str = "\
TARGET: src/lib.rs
FIND:
<<<
let x = 0;
>>>
REPLACE:
<<<
let x = 1;
>>>
RATIONALE: bump the constant
";

    #[test]
    fn parses_well_formed_proposal() {
        let p = parse_proposal(WELL_FORMED).unwrap();
        assert_eq!(p.patch.target, "src/lib.rs");
        assert_eq!(p.patch.find, "let x = 0;");
        assert_eq!(p.patch.replace, "let x = 1;");
        assert_eq!(p.rationale, "bump the constant");
    }

    #[test]
    fn off_format_yields_none() {
        assert!(parse_proposal("I think you should change something.").is_none());
    }

    #[test]
    fn proposer_enforces_allow_list() {
        let mut rng = SplitMix64::new(0);
        let root = std::path::Path::new("/tmp");
        let ctx = ImprovementContext {
            workspace_root: root,
            goal: "g",
            parent_fitness: None,
            recent_rejections: &[],
        };
        // Allowed → returns a proposal.
        let ok = LlmProposer::new(
            FixedModel(WELL_FORMED.to_string()),
            vec!["src/lib.rs".into()],
        );
        assert!(ok.propose(&ctx, &mut rng).unwrap().is_some());
        // Not allowed → dropped.
        let blocked =
            LlmProposer::new(FixedModel(WELL_FORMED.to_string()), vec!["other.rs".into()]);
        assert!(blocked.propose(&ctx, &mut rng).unwrap().is_none());
    }
}
