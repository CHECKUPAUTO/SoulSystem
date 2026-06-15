//! Inter-rollout agreement — a cheap adaptive-compute signal.
//!
//! When several experts answer the same prompt, how much they *agree* is a free
//! signal for whether the answer can be trusted or whether more compute should
//! be spent (arXiv:2604.08369, `docs/RESEARCH_FRONTIER_2026.md` §5). High
//! agreement → proceed; low agreement → escalate (more experts, a stronger
//! model, or human review) rather than committing to a coin-flip.
//!
//! The measure is deterministic — mean pairwise Jaccard similarity over the
//! answers' token sets — so the escalate/proceed decision is reproducible.

use crate::senate::ExpertResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The consensus analysis over a set of candidate answers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgreementReport {
    /// Number of answers analysed.
    pub n: usize,
    /// Mean pairwise similarity in `[0, 1]` (1.0 = identical, 0.0 = disjoint).
    pub consensus: f64,
    /// Index of the most *central* answer (highest summed similarity to the
    /// others) — the one to prefer when proceeding. `None` for an empty set.
    pub central: Option<usize>,
    /// True when consensus is below the escalation threshold: the answers
    /// disagree enough that more compute is warranted before trusting one.
    pub escalate: bool,
}

/// Computes [`AgreementReport`]s and the escalate/proceed decision.
#[derive(Debug, Clone)]
pub struct AgreementAnalyzer {
    /// Escalate when consensus is strictly below this value.
    pub escalate_below: f64,
}

impl Default for AgreementAnalyzer {
    fn default() -> Self {
        // Below ~half-overlap on average, the panel is too split to trust.
        Self {
            escalate_below: 0.5,
        }
    }
}

impl AgreementAnalyzer {
    pub fn new(escalate_below: f64) -> Self {
        Self {
            escalate_below: escalate_below.clamp(0.0, 1.0),
        }
    }

    /// Analyse how much the candidate answers agree.
    pub fn analyze(&self, responses: &[ExpertResponse]) -> AgreementReport {
        let n = responses.len();
        if n == 0 {
            return AgreementReport {
                n: 0,
                consensus: 0.0,
                central: None,
                escalate: true,
            };
        }
        if n == 1 {
            // A lone answer trivially "agrees" with itself, but there is no
            // corroboration — leave the escalate decision to the caller's
            // threshold (consensus 1.0 ⇒ proceed by default).
            return AgreementReport {
                n: 1,
                consensus: 1.0,
                central: Some(0),
                escalate: 1.0 < self.escalate_below,
            };
        }

        let token_sets: Vec<HashSet<String>> =
            responses.iter().map(|r| tokens(&r.response)).collect();

        // Pairwise similarities; track each answer's summed similarity to find
        // the most central one.
        let mut total = 0.0;
        let mut pairs = 0usize;
        let mut summed = vec![0.0f64; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let s = jaccard(&token_sets[i], &token_sets[j]);
                total += s;
                pairs += 1;
                summed[i] += s;
                summed[j] += s;
            }
        }
        let consensus = if pairs == 0 {
            1.0
        } else {
            total / pairs as f64
        };

        let central = (0..n).max_by(|&a, &b| {
            summed[a]
                .partial_cmp(&summed[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        AgreementReport {
            n,
            consensus,
            central,
            escalate: consensus < self.escalate_below,
        }
    }
}

/// Lowercase token set of a text (alphanumeric runs).
fn tokens(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// Jaccard similarity of two token sets: `|A∩B| / |A∪B|`. Two empty sets are
/// defined as fully similar (1.0).
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        1.0
    } else {
        inter as f64 / union as f64
    }
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

    #[test]
    fn identical_answers_full_consensus() {
        let a = AgreementAnalyzer::default();
        let report = a.analyze(&[
            resp("rust ownership and borrowing"),
            resp("rust ownership and borrowing"),
        ]);
        assert!((report.consensus - 1.0).abs() < 1e-9);
        assert!(!report.escalate);
    }

    #[test]
    fn disjoint_answers_escalate() {
        let a = AgreementAnalyzer::default();
        let report = a.analyze(&[
            resp("apples oranges bananas"),
            resp("quantum relativity entropy"),
        ]);
        assert_eq!(report.consensus, 0.0);
        assert!(report.escalate);
    }

    #[test]
    fn partial_overlap_between_zero_and_one() {
        let a = AgreementAnalyzer::new(0.9);
        // {a,b,c} vs {b,c,d}: inter=2, union=4 → 0.5.
        let report = a.analyze(&[resp("a b c"), resp("b c d")]);
        assert!((report.consensus - 0.5).abs() < 1e-9);
        assert!(report.escalate); // 0.5 < 0.9
    }

    #[test]
    fn central_answer_is_most_representative() {
        let a = AgreementAnalyzer::default();
        // Two answers share "rust ownership"; the third is an outlier. The
        // central index should be one of the two agreeing answers, not the
        // outlier.
        let report = a.analyze(&[
            resp("rust ownership model"),
            resp("rust ownership rules"),
            resp("completely unrelated text"),
        ]);
        assert_ne!(report.central, Some(2));
    }

    #[test]
    fn empty_set_escalates() {
        let a = AgreementAnalyzer::default();
        let report = a.analyze(&[]);
        assert_eq!(report.n, 0);
        assert!(report.escalate);
        assert_eq!(report.central, None);
    }

    #[test]
    fn single_answer_proceeds_by_default() {
        let a = AgreementAnalyzer::default();
        let report = a.analyze(&[resp("the only answer")]);
        assert_eq!(report.consensus, 1.0);
        assert!(!report.escalate);
        assert_eq!(report.central, Some(0));
    }
}
