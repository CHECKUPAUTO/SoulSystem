//! Verified aggregation — independent verifiers + a coverage gate.
//!
//! The plain [`AggregationStrategy`](crate::AggregationStrategy) picks a winner
//! by length or arrival order, which says nothing about whether the answer is
//! *correct* or even *on-topic*. This module implements the reliability moat from
//! `docs/RESEARCH_FRONTIER_2026.md` §5: multiple independent [`Verifier`]s judge
//! each candidate (Multi-Agent Verification, arXiv:2502.20379), the winner is the
//! best-approved candidate, and the orchestrator output is **gated** — if no
//! candidate actually addresses the query (arXiv:2603.11445, Plan-Execute
//! verification) the result is flagged `needs_replanning` instead of returning
//! the least-bad guess.
//!
//! Verifiers come in two kinds:
//! - **gates** (`is_gate() == true`) are mandatory: a candidate that fails any
//!   gate is ineligible no matter how it scores elsewhere. Coverage is a gate —
//!   an off-topic answer is never acceptable.
//! - **votes** are scored by `min_approval_ratio`: enough of them must approve.
//!
//! Every verifier here is deterministic (no LLM), so the decision is auditable
//! and reproducible — the "zero-variance multi-agent output" property
//! (arXiv:2511.15755) that production SLAs need.

use crate::senate::ExpertResponse;
use serde::{Deserialize, Serialize};

/// One verifier's judgement of a single candidate answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Verdict {
    /// Name of the verifier that produced this judgement.
    pub verifier: String,
    /// Whether the candidate cleared this verifier.
    pub approved: bool,
    /// Confidence in `[0, 1]`.
    pub score: f64,
    /// Human-readable justification.
    pub reason: String,
}

/// An independent judge of whether a candidate answer is acceptable for a query.
///
/// Verifiers are intentionally narrow and independent — the strength of
/// multi-verifier test-time compute comes from combining several cheap,
/// uncorrelated checks rather than one expensive judge.
pub trait Verifier: Send + Sync {
    fn name(&self) -> &str;
    fn verify(&self, query: &str, candidate: &ExpertResponse) -> Verdict;

    /// A *gate* verifier is mandatory: failing it makes the candidate
    /// ineligible regardless of other verdicts. Defaults to a vote.
    fn is_gate(&self) -> bool {
        false
    }
}

/// Rejects empty or trivially short answers. A vote, not a gate.
#[derive(Debug, Clone)]
pub struct NonEmptyVerifier {
    pub min_chars: usize,
}

impl Default for NonEmptyVerifier {
    fn default() -> Self {
        Self { min_chars: 1 }
    }
}

impl Verifier for NonEmptyVerifier {
    fn name(&self) -> &str {
        "non_empty"
    }

    fn verify(&self, _query: &str, candidate: &ExpertResponse) -> Verdict {
        let len = candidate.response.trim().chars().count();
        let approved = len >= self.min_chars;
        Verdict {
            verifier: self.name().to_string(),
            approved,
            score: if approved { 1.0 } else { 0.0 },
            reason: if approved {
                format!("{len} chars")
            } else {
                format!("only {len} chars (< {})", self.min_chars)
            },
        }
    }
}

/// Approves a candidate only if it covers enough of the query's significant
/// terms — the "does the answer address the query" check. This is a **gate**.
#[derive(Debug, Clone)]
pub struct CoverageVerifier {
    /// Minimum fraction of query keywords that must appear in the answer.
    pub min_coverage: f64,
}

impl Default for CoverageVerifier {
    fn default() -> Self {
        Self { min_coverage: 0.5 }
    }
}

impl Verifier for CoverageVerifier {
    fn name(&self) -> &str {
        "coverage"
    }

    fn is_gate(&self) -> bool {
        true
    }

    fn verify(&self, query: &str, candidate: &ExpertResponse) -> Verdict {
        let keywords = keywords(query);
        // A query with no significant terms is trivially covered.
        if keywords.is_empty() {
            return Verdict {
                verifier: self.name().to_string(),
                approved: true,
                score: 1.0,
                reason: "query has no significant terms".into(),
            };
        }
        let answer = candidate.response.to_lowercase();
        let hits = keywords.iter().filter(|k| answer.contains(*k)).count();
        let coverage = hits as f64 / keywords.len() as f64;
        let approved = coverage >= self.min_coverage;
        Verdict {
            verifier: self.name().to_string(),
            approved,
            score: coverage,
            reason: format!("{hits}/{} query terms covered", keywords.len()),
        }
    }
}

/// Per-candidate result of running every verifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateAssessment {
    pub response: ExpertResponse,
    pub verdicts: Vec<Verdict>,
    /// How many verifiers approved (gates and votes alike).
    pub approvals: usize,
    /// Sum of verifier scores (tie-breaker).
    pub total_score: f64,
    /// Whether every gate verifier approved.
    pub gates_passed: bool,
    /// Whether the vote verifiers met the approval ratio.
    pub votes_passed: bool,
}

impl CandidateAssessment {
    /// Eligible to be selected: cleared every gate and the vote threshold.
    pub fn eligible(&self) -> bool {
        self.gates_passed && self.votes_passed
    }
}

/// The outcome of verified aggregation over a set of candidate answers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifiedResult {
    /// The selected answer text (empty when nothing cleared the gate).
    pub response: String,
    /// Index of the selected candidate, if any.
    pub selected: Option<usize>,
    /// Per-candidate verifier assessments, in input order.
    pub candidates: Vec<CandidateAssessment>,
    /// Whether the selected answer cleared verification.
    pub verified: bool,
    /// True when no candidate cleared the gate — the orchestrator should replan
    /// rather than trust any answer.
    pub needs_replanning: bool,
    /// Human-readable explanation of the decision.
    pub reason: String,
}

/// Aggregates candidate answers by independent verification.
pub struct VerifiedAggregator {
    verifiers: Vec<Box<dyn Verifier>>,
    /// Fraction of *vote* verifiers that must approve for eligibility.
    min_approval_ratio: f64,
}

impl Default for VerifiedAggregator {
    fn default() -> Self {
        Self::standard()
    }
}

impl VerifiedAggregator {
    /// An aggregator with no verifiers yet; requires a vote majority by default.
    pub fn new() -> Self {
        Self {
            verifiers: Vec::new(),
            min_approval_ratio: 0.5,
        }
    }

    /// The standard panel: coverage (gate) + non-empty (vote), vote majority.
    pub fn standard() -> Self {
        Self::new()
            .with_verifier(Box::new(CoverageVerifier::default()))
            .with_verifier(Box::new(NonEmptyVerifier::default()))
    }

    pub fn with_verifier(mut self, verifier: Box<dyn Verifier>) -> Self {
        self.verifiers.push(verifier);
        self
    }

    /// Require this fraction of *vote* verifiers to approve (1.0 = unanimous).
    pub fn with_min_approval_ratio(mut self, ratio: f64) -> Self {
        self.min_approval_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Verify every candidate, select the best-approved eligible one, and gate
    /// the result on whether any candidate cleared verification.
    pub fn assess(&self, query: &str, responses: &[ExpertResponse]) -> VerifiedResult {
        let total_verifiers = self.verifiers.len();
        let vote_total = self.verifiers.iter().filter(|v| !v.is_gate()).count();

        let candidates: Vec<CandidateAssessment> = responses
            .iter()
            .map(|r| {
                let verdicts: Vec<Verdict> =
                    self.verifiers.iter().map(|v| v.verify(query, r)).collect();
                let approvals = verdicts.iter().filter(|v| v.approved).count();
                let total_score = verdicts.iter().map(|v| v.score).sum();

                let gates_passed = self
                    .verifiers
                    .iter()
                    .zip(&verdicts)
                    .filter(|(v, _)| v.is_gate())
                    .all(|(_, verdict)| verdict.approved);

                let vote_approvals = self
                    .verifiers
                    .iter()
                    .zip(&verdicts)
                    .filter(|(v, verdict)| !v.is_gate() && verdict.approved)
                    .count();
                let votes_passed = vote_total == 0
                    || (vote_approvals as f64 / vote_total as f64) >= self.min_approval_ratio;

                CandidateAssessment {
                    response: r.clone(),
                    verdicts,
                    approvals,
                    total_score,
                    gates_passed,
                    votes_passed,
                }
            })
            .collect();

        if responses.is_empty() {
            return VerifiedResult {
                response: String::new(),
                selected: None,
                candidates,
                verified: false,
                needs_replanning: true,
                reason: "no expert responses to verify".into(),
            };
        }

        // Pick the eligible candidate with the most approvals, then the highest
        // summed score. Stable on input order for reproducibility.
        let best = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.eligible())
            .max_by(|(_, a), (_, b)| {
                a.approvals.cmp(&b.approvals).then(
                    a.total_score
                        .partial_cmp(&b.total_score)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            });

        match best {
            Some((idx, c)) => VerifiedResult {
                response: c.response.response.clone(),
                selected: Some(idx),
                reason: format!(
                    "candidate {idx} cleared all gates and {}/{total_verifiers} verifiers",
                    c.approvals
                ),
                candidates,
                verified: true,
                needs_replanning: false,
            },
            None => VerifiedResult {
                response: String::new(),
                selected: None,
                candidates,
                verified: false,
                needs_replanning: true,
                reason: "no candidate cleared verification; replanning advised".into(),
            },
        }
    }
}

/// Significant lowercase terms of a query (drops stopwords and short tokens).
fn keywords(query: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "for", "with", "at", "by",
        "is", "are", "was", "were", "be", "how", "what", "why", "when", "do", "does", "did", "can",
        "should", "would", "i", "you", "we", "it", "this", "that",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(text: &str) -> ExpertResponse {
        ExpertResponse {
            model: "m".into(),
            role: "expert".into(),
            response: text.into(),
            duration_ms: 1,
        }
    }

    /// A test-only vote verifier that always denies — to exercise vote ratios.
    struct AlwaysDeny;
    impl Verifier for AlwaysDeny {
        fn name(&self) -> &str {
            "always_deny"
        }
        fn verify(&self, _q: &str, _c: &ExpertResponse) -> Verdict {
            Verdict {
                verifier: "always_deny".into(),
                approved: false,
                score: 0.0,
                reason: "denied".into(),
            }
        }
    }

    #[test]
    fn non_empty_verifier_rejects_blank() {
        let v = NonEmptyVerifier { min_chars: 3 };
        assert!(!v.verify("q", &resp("  ")).approved);
        assert!(v.verify("q", &resp("hello")).approved);
    }

    #[test]
    fn coverage_verifier_scores_term_overlap() {
        let v = CoverageVerifier { min_coverage: 0.5 };
        // Query keywords: fix, deadlock, rust, mutex (4).
        let good = v.verify(
            "how to fix a deadlock in rust with a mutex",
            &resp("to fix a deadlock, two threads each hold a mutex; in rust use try_lock"),
        );
        assert!(good.approved);
        assert!((good.score - 1.0).abs() < 1e-9, "score was {}", good.score);

        let bad = v.verify(
            "how to fix a deadlock in rust with a mutex",
            &resp("the weather is nice today"),
        );
        assert!(!bad.approved);
        assert_eq!(bad.score, 0.0);
    }

    #[test]
    fn coverage_is_a_gate() {
        assert!(CoverageVerifier::default().is_gate());
        assert!(!NonEmptyVerifier::default().is_gate());
    }

    #[test]
    fn coverage_trivial_when_no_keywords() {
        let v = CoverageVerifier::default();
        assert!(v.verify("a an the", &resp("anything")).approved);
    }

    #[test]
    fn assess_selects_most_approved_candidate() {
        let agg = VerifiedAggregator::standard();
        let query = "explain rust ownership and borrowing";
        let responses = vec![
            resp("nope"),
            resp("rust ownership means each value has one owner; borrowing lends references"),
        ];
        let result = agg.assess(query, &responses);
        assert!(result.verified);
        assert_eq!(result.selected, Some(1));
        assert!(result.response.contains("ownership"));
        assert!(!result.needs_replanning);
    }

    #[test]
    fn coverage_gate_rejects_off_topic_even_if_nonempty() {
        // A long, non-empty, but entirely off-topic answer fails the coverage
        // gate and must not be returned as verified.
        let agg = VerifiedAggregator::standard();
        let query = "explain rust ownership and borrowing";
        let responses = vec![resp("the weather is nice and the sky is blue today")];
        let result = agg.assess(query, &responses);
        assert!(!result.verified);
        assert!(result.needs_replanning);
        assert_eq!(result.selected, None);
        assert!(result.response.is_empty());
    }

    #[test]
    fn assess_flags_replanning_when_nothing_clears() {
        let agg = VerifiedAggregator::standard();
        let query = "explain rust ownership and borrowing";
        let responses = vec![resp(""), resp("the weather is nice")];
        let result = agg.assess(query, &responses);
        assert!(!result.verified);
        assert!(result.needs_replanning);
        assert_eq!(result.selected, None);
    }

    #[test]
    fn assess_empty_responses_needs_replanning() {
        let agg = VerifiedAggregator::standard();
        let result = agg.assess("anything", &[]);
        assert!(result.needs_replanning);
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn vote_ratio_gates_secondary_checks() {
        // Coverage (gate) + non_empty (vote) + always_deny (vote) → 2 votes.
        // A good, on-topic answer clears the gate; votes are 1/2 approved.
        let query = "explain rust ownership and borrowing";
        let answer = "rust ownership means each value has one owner; borrowing lends references";

        // Unanimous vote ratio (1.0) needs both votes → rejected (always_deny).
        let strict = VerifiedAggregator::new()
            .with_verifier(Box::new(CoverageVerifier::default()))
            .with_verifier(Box::new(NonEmptyVerifier::default()))
            .with_verifier(Box::new(AlwaysDeny))
            .with_min_approval_ratio(1.0);
        assert!(strict.assess(query, &[resp(answer)]).needs_replanning);

        // Majority vote ratio (0.5) accepts 1/2 votes → verified.
        let lenient = VerifiedAggregator::new()
            .with_verifier(Box::new(CoverageVerifier::default()))
            .with_verifier(Box::new(NonEmptyVerifier::default()))
            .with_verifier(Box::new(AlwaysDeny))
            .with_min_approval_ratio(0.5);
        let r = lenient.assess(query, &[resp(answer)]);
        assert!(r.verified);
        assert_eq!(r.selected, Some(0));
    }
}
