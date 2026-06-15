//! MeshContext — structured summary of per-brain HNN telemetry.
//!
//! Built from the raw `results` map returned by `call_all_parallel` on
//! `/api/query`. All brain responses are `serde_json::Value` with varying
//! shapes; this module produces a stable, flat summary that we can
//! (a) serialize back to the gateway, and (b) format into a prompt for
//! the LLM.
//!
//! # What each brain provides (as of Phase 6a-hybrid)
//!
//! The brains return:
//! * `top_concepts: [{concept, score, mastery}]` — merged separately
//! * `attractor: "StableOrbit" | "DeepBasin" | "Transient" | ...`
//! * `turbulence: f64` — 0.0 (calm) to 1.0 (chaotic)
//! * `momentum: f64` — rate of change of activation
//! * `active_neurons: u32` — count currently firing above threshold
//!
//! Any field that's missing or the wrong type is silently defaulted; we
//! never panic on a malformed brain response.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::routing::attractor_score;

#[derive(Debug, Clone, Serialize)]
pub struct BrainSummary {
    pub brain: String,
    pub attractor: String,
    pub turbulence: f64,
    pub momentum: f64,
    pub active_neurons: u32,
    pub concept_count: usize,
    /// true if the brain returned an error instead of a reading
    pub errored: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeshContext {
    pub query: String,
    pub brains: Vec<BrainSummary>,
    /// The attractor present in the largest fraction of brains.
    pub attractor_dominant: String,
    pub turbulence_avg: f64,
    pub turbulence_max: f64,
    pub active_neurons_total: u32,
    /// The best 5 concepts across the mesh (pre-merged).
    pub top_concepts: Vec<ConceptSummary>,
    pub brains_errored: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConceptSummary {
    pub concept: String,
    pub score: f64,
    pub source_brain: String,
}

/// A verified-orchestration confidence gate over the mesh panel
/// (`docs/RESEARCH_FRONTIER_2026.md` §5, adapted to the brains' telemetry):
/// how much the brains *agree* and whether the surfaced concepts actually
/// *cover* the query. Low confidence flags `needs_replanning` so the caller can
/// escalate or re-query rather than trust a split, off-topic result.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeshConfidence {
    /// Fraction of responding brains sharing the dominant attractor (consensus).
    pub agreement: f64,
    /// Fraction of the query's significant terms present in the top concepts.
    pub coverage: f64,
    /// Both agreement and coverage cleared the bar (and a brain responded).
    pub verified: bool,
    /// No candidate cleared the gate — replan / escalate rather than trust it.
    pub needs_replanning: bool,
    pub reason: String,
}

impl MeshContext {
    /// Build from the raw results map + the already-merged concepts list
    /// (produced by `routing::merge_concepts`).
    pub fn build(query: &str, results: &HashMap<String, Value>, merged: &[Value]) -> Self {
        let mut brains: Vec<BrainSummary> = results
            .iter()
            .map(|(name, v)| summarize_brain(name, v))
            .collect();
        // Deterministic order for logging + testing
        brains.sort_by(|a, b| a.brain.cmp(&b.brain));

        let brains_errored: Vec<String> = brains
            .iter()
            .filter(|b| b.errored)
            .map(|b| b.brain.clone())
            .collect();

        let ok_brains: Vec<&BrainSummary> = brains.iter().filter(|b| !b.errored).collect();

        let attractor_dominant = dominant_attractor(&ok_brains);
        let turbulence_avg = if ok_brains.is_empty() {
            0.0
        } else {
            ok_brains.iter().map(|b| b.turbulence).sum::<f64>() / ok_brains.len() as f64
        };
        let turbulence_max = ok_brains
            .iter()
            .map(|b| b.turbulence)
            .fold(0.0_f64, f64::max);
        let active_neurons_total: u32 = ok_brains.iter().map(|b| b.active_neurons).sum();

        let top_concepts: Vec<ConceptSummary> = merged
            .iter()
            .take(5)
            .filter_map(|v| {
                Some(ConceptSummary {
                    concept: v.get("concept")?.as_str()?.to_string(),
                    score: v.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    source_brain: v
                        .get("source_brain")
                        .and_then(|x| x.as_str())
                        .unwrap_or("?")
                        .to_string(),
                })
            })
            .collect();

        Self {
            query: query.to_string(),
            brains,
            attractor_dominant,
            turbulence_avg,
            turbulence_max,
            active_neurons_total,
            top_concepts,
            brains_errored,
        }
    }

    /// Compute the confidence gate over the mesh panel: agreement (attractor
    /// consensus among the brains that responded) and coverage (how much the
    /// surfaced concepts address the query). Verified only when both clear 0.5.
    pub fn confidence(&self) -> MeshConfidence {
        let ok: Vec<&BrainSummary> = self.brains.iter().filter(|b| !b.errored).collect();
        if ok.is_empty() {
            return MeshConfidence {
                agreement: 0.0,
                coverage: 0.0,
                verified: false,
                needs_replanning: true,
                reason: "no brains responded".into(),
            };
        }

        let agreeing = ok
            .iter()
            .filter(|b| b.attractor == self.attractor_dominant)
            .count();
        let agreement = agreeing as f64 / ok.len() as f64;

        let kws = query_keywords(&self.query);
        let coverage = if kws.is_empty() {
            1.0
        } else {
            let blob = self
                .top_concepts
                .iter()
                .map(|c| c.concept.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            let hits = kws.iter().filter(|k| blob.contains(*k)).count();
            hits as f64 / kws.len() as f64
        };

        let verified = agreement >= 0.5 && coverage >= 0.5;
        let reason = if verified {
            format!("agreement={agreement:.2}, coverage={coverage:.2}")
        } else {
            format!(
                "low confidence (agreement={agreement:.2}, coverage={coverage:.2}); \
                 consider replanning or escalating"
            )
        };
        MeshConfidence {
            agreement,
            coverage,
            verified,
            needs_replanning: !verified,
            reason,
        }
    }

    /// Format as a natural-language block for inclusion in an LLM prompt.
    /// Intentionally compact — the model doesn't need verbose prose.
    pub fn format_for_prompt(&self) -> String {
        let concepts_line = if self.top_concepts.is_empty() {
            "  (no concepts surfaced)".to_string()
        } else {
            self.top_concepts
                .iter()
                .take(5)
                .map(|c| {
                    format!(
                        "  - {} (score {:.2}, from {})",
                        c.concept, c.score, c.source_brain
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let brains_line = self
            .brains
            .iter()
            .filter(|b| !b.errored)
            .map(|b| {
                format!(
                    "  - {}: attractor={}, turbulence={:.2}, active_neurons={}",
                    b.brain, b.attractor, b.turbulence, b.active_neurons
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let errored_note = if self.brains_errored.is_empty() {
            String::new()
        } else {
            format!(
                "\n(brains that errored: {})",
                self.brains_errored.join(", ")
            )
        };

        let conf = self.confidence();
        let confidence_line = format!(
            "\n\nMesh confidence: {} (agreement={:.2}, coverage={:.2}).{}",
            if conf.verified { "verified" } else { "LOW" },
            conf.agreement,
            conf.coverage,
            if conf.needs_replanning {
                " Treat the answer as tentative; say so and suggest what to clarify."
            } else {
                ""
            },
        );

        format!(
            "Mesh telemetry for the query:\n\
             Consulted {} brain(s). Dominant attractor: {}. \
             Turbulence avg={:.2}, max={:.2}. Total active neurons: {}.\n\n\
             Per-brain readings:\n{}\n\n\
             Top concepts:\n{}{}{}",
            self.brains.len(),
            self.attractor_dominant,
            self.turbulence_avg,
            self.turbulence_max,
            self.active_neurons_total,
            brains_line,
            concepts_line,
            errored_note,
            confidence_line,
        )
    }
}

/// Significant lowercase terms of the query (drops stopwords and short tokens),
/// for the coverage check.
fn query_keywords(query: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "for", "with", "at", "by",
        "is", "are", "was", "were", "be", "how", "what", "why", "when", "do", "does", "did", "can",
        "should", "would", "i", "you", "we", "it", "this", "that", "explain", "tell", "about",
    ];
    let mut out = Vec::new();
    for raw in query.split(|c: char| !c.is_alphanumeric()) {
        let w = raw.to_lowercase();
        if w.len() < 3 || STOPWORDS.contains(&w.as_str()) {
            continue;
        }
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

fn summarize_brain(name: &str, v: &Value) -> BrainSummary {
    if v.get("error").is_some() {
        return BrainSummary {
            brain: name.to_string(),
            attractor: "unknown".into(),
            turbulence: 0.0,
            momentum: 0.0,
            active_neurons: 0,
            concept_count: 0,
            errored: true,
        };
    }
    BrainSummary {
        brain: name.to_string(),
        attractor: v
            .get("attractor")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string(),
        turbulence: v.get("turbulence").and_then(|x| x.as_f64()).unwrap_or(0.0),
        momentum: v.get("momentum").and_then(|x| x.as_f64()).unwrap_or(0.0),
        active_neurons: v
            .get("active_neurons")
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as u32,
        concept_count: v
            .get("top_concepts")
            .and_then(|x| x.as_array())
            .map(|a| a.len())
            .unwrap_or(0),
        errored: false,
    }
}

fn dominant_attractor(brains: &[&BrainSummary]) -> String {
    if brains.is_empty() {
        return "unknown".into();
    }
    // Count occurrences, break ties by attractor_score (stability preferred).
    let mut counts: HashMap<&str, (u32, f64)> = HashMap::new();
    for b in brains {
        let entry = counts
            .entry(b.attractor.as_str())
            .or_insert((0, attractor_score(&b.attractor)));
        entry.0 += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| {
            a.1 .0.cmp(&b.1 .0).then(
                a.1 .1
                    .partial_cmp(&b.1 .1)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        })
        .map(|(k, _)| k.to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_with_empty_results() {
        let ctx = MeshContext::build("hello", &HashMap::new(), &[]);
        assert_eq!(ctx.query, "hello");
        assert_eq!(ctx.attractor_dominant, "unknown");
        assert_eq!(ctx.turbulence_avg, 0.0);
        assert!(ctx.brains.is_empty());
        assert!(ctx.top_concepts.is_empty());
    }

    #[test]
    fn build_aggregates_telemetry() {
        let mut results = HashMap::new();
        results.insert(
            "science".into(),
            json!({
                "attractor": "StableOrbit", "turbulence": 0.1,
                "momentum": 0.05, "active_neurons": 42,
                "top_concepts": [{"concept": "x", "score": 0.8}]
            }),
        );
        results.insert(
            "mind".into(),
            json!({
                "attractor": "StableOrbit", "turbulence": 0.3,
                "momentum": 0.1, "active_neurons": 15,
            }),
        );
        results.insert(
            "broken".into(),
            json!({
                "error": "connection refused"
            }),
        );

        let merged = vec![json!({"concept": "x", "score": 0.8, "source_brain": "science"})];
        let ctx = MeshContext::build("test", &results, &merged);

        assert_eq!(ctx.brains.len(), 3);
        assert_eq!(ctx.brains_errored, vec!["broken"]);
        assert_eq!(ctx.attractor_dominant, "StableOrbit");
        assert!((ctx.turbulence_avg - 0.2).abs() < 1e-9);
        assert!((ctx.turbulence_max - 0.3).abs() < 1e-9);
        assert_eq!(ctx.active_neurons_total, 42 + 15);
        assert_eq!(ctx.top_concepts.len(), 1);
        assert_eq!(ctx.top_concepts[0].concept, "x");
    }

    #[test]
    fn format_for_prompt_includes_all_sections() {
        let results = HashMap::from([(
            "science".to_string(),
            json!({
                "attractor": "DeepBasin", "turbulence": 0.2, "active_neurons": 30,
            }),
        )]);
        let merged = vec![json!({"concept": "entropy", "score": 0.9, "source_brain": "science"})];
        let ctx = MeshContext::build("query", &results, &merged);
        let text = ctx.format_for_prompt();
        assert!(text.contains("Consulted 1 brain"));
        assert!(text.contains("DeepBasin"));
        assert!(text.contains("entropy"));
        assert!(text.contains("science"));
    }

    #[test]
    fn format_handles_no_concepts() {
        let results = HashMap::from([(
            "meta".to_string(),
            json!({"attractor": "Transient", "turbulence": 0.5}),
        )]);
        let ctx = MeshContext::build("hi", &results, &[]);
        let text = ctx.format_for_prompt();
        assert!(text.contains("(no concepts surfaced)"));
        assert!(text.contains("Transient"));
    }

    #[test]
    fn confidence_verified_when_brains_agree_and_concepts_cover() {
        let results = HashMap::from([
            (
                "science".to_string(),
                json!({"attractor": "StableOrbit", "turbulence": 0.1}),
            ),
            (
                "mind".to_string(),
                json!({"attractor": "StableOrbit", "turbulence": 0.2}),
            ),
        ]);
        let merged = vec![
            json!({"concept": "entropy", "score": 0.9, "source_brain": "science"}),
            json!({"concept": "thermodynamics", "score": 0.7, "source_brain": "science"}),
        ];
        let ctx = MeshContext::build("explain entropy and thermodynamics", &results, &merged);
        let c = ctx.confidence();
        assert!((c.agreement - 1.0).abs() < 1e-9); // both StableOrbit
        assert!((c.coverage - 1.0).abs() < 1e-9); // both query terms surfaced
        assert!(c.verified);
        assert!(!c.needs_replanning);
    }

    #[test]
    fn confidence_flags_replanning_on_disagreement_and_no_coverage() {
        let results = HashMap::from([
            (
                "science".to_string(),
                json!({"attractor": "StableOrbit", "turbulence": 0.1}),
            ),
            (
                "mind".to_string(),
                json!({"attractor": "Transient", "turbulence": 0.9}),
            ),
        ]);
        // Concepts unrelated to the query → zero coverage.
        let merged = vec![json!({"concept": "cooking", "score": 0.5, "source_brain": "mind"})];
        let ctx = MeshContext::build("explain quantum entanglement", &results, &merged);
        let c = ctx.confidence();
        assert!(c.agreement < 1.0); // split attractors
        assert_eq!(c.coverage, 0.0);
        assert!(c.needs_replanning);
        assert!(ctx.format_for_prompt().contains("LOW"));
    }

    #[test]
    fn confidence_needs_replanning_when_no_brains() {
        let ctx = MeshContext::build("anything", &HashMap::new(), &[]);
        let c = ctx.confidence();
        assert!(c.needs_replanning);
        assert_eq!(c.reason, "no brains responded");
    }

    #[test]
    fn dominant_attractor_ties_broken_by_stability() {
        // science → StableOrbit (score 1.0); mind → StrangeAttractor (0.1).
        // Both have count=1, tie-break via attractor_score → StableOrbit wins.
        let results = HashMap::from([
            (
                "science".to_string(),
                json!({"attractor": "StableOrbit",      "turbulence": 0.1}),
            ),
            (
                "mind".to_string(),
                json!({"attractor": "StrangeAttractor", "turbulence": 0.9}),
            ),
        ]);
        let ctx = MeshContext::build("q", &results, &[]);
        assert_eq!(ctx.attractor_dominant, "StableOrbit");
    }
}
