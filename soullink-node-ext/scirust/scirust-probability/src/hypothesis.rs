//! Hypothesis verification loop (LLM hybrid interface).

use crate::bayesian::BayesianNetwork;
use crate::consistency::{ConsistencyEngine, ConsistencyScore, Rule};
use crate::memory::InferenceMemory;
use scirust_symbolic::Expr;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Hypothesis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HypothesisSource {
    LLM,
    Deduced,
    Observed,
}

#[derive(Debug, Clone)]
pub struct Hypothesis {
    pub statement: Expr,
    pub confidence: f64,
    pub source: HypothesisSource,
    pub timestamp: u64,
}

impl Hypothesis {
    pub fn new(statement: Expr, confidence: f64, source: HypothesisSource) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self { statement, confidence, source, timestamp }
    }

    pub fn from_llm(statement: Expr) -> Self {
        Self::new(statement, 0.5, HypothesisSource::LLM)
    }

    pub fn from_observation(statement: Expr) -> Self {
        Self::new(statement, 0.95, HypothesisSource::Observed)
    }

    pub fn from_deduced(statement: Expr, confidence: f64) -> Self {
        Self::new(statement, confidence, HypothesisSource::Deduced)
    }

    pub fn to_summary(&self) -> String {
        let src = match self.source {
            HypothesisSource::LLM => "LLM",
            HypothesisSource::Deduced => "deduced",
            HypothesisSource::Observed => "observed",
        };
        format!("[{}] '{}' (conf: {:.2}, {})",
            self.timestamp, self.statement, self.confidence, src)
    }
}

// ---------------------------------------------------------------------------
// Verification Result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Consistent,
    Contradicted,
    Uncertain,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub verdict: Verdict,
    pub confidence: f64,
    pub explanation: String,
    pub iterations: usize,
}

impl VerificationResult {
    pub fn consistent(confidence: f64, explanation: String) -> Self {
        Self { verdict: Verdict::Consistent, confidence, explanation, iterations: 1 }
    }

    pub fn contradicted(explanation: String) -> Self {
        Self { verdict: Verdict::Contradicted, confidence: 0.0, explanation, iterations: 1 }
    }

    pub fn uncertain(explanation: String) -> Self {
        Self { verdict: Verdict::Uncertain, confidence: 0.0, explanation, iterations: 1 }
    }
}

// ---------------------------------------------------------------------------
// Correction Suggestion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CorrectionSuggestion {
    pub original: Hypothesis,
    pub corrected_statement: Expr,
    pub confidence: f64,
    pub rationale: String,
}

impl CorrectionSuggestion {
    pub fn new(original: Hypothesis, corrected: Expr, confidence: f64, rationale: &str) -> Self {
        Self { original, corrected_statement: corrected, confidence, rationale: rationale.to_string() }
    }
}

// ---------------------------------------------------------------------------
// Hypothesis Verifier
// ---------------------------------------------------------------------------

pub struct HypothesisVerifier {
    bayesian_network: Option<BayesianNetwork>,
    consistency_engine: ConsistencyEngine,
    max_iterations: usize,
}

impl HypothesisVerifier {
    pub fn new(bayesian_network: Option<BayesianNetwork>, consistency_engine: ConsistencyEngine) -> Self {
        Self { bayesian_network, consistency_engine, max_iterations: 5 }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    pub fn verify(&self, hypothesis: &Hypothesis) -> VerificationResult {
        let mut explanation = String::new();

        let consistency_score = self.consistency_engine.check(&[hypothesis.statement.clone()]);

        if consistency_score.score < 0.5 {
            explanation.push_str(&format!("Consistency check failed (score: {:.2}):\n", consistency_score.score));
            for issue in &consistency_score.issues {
                explanation.push_str(&format!("  - {}\n", issue));
            }
            return VerificationResult::contradicted(explanation);
        }

        if consistency_score.score < 1.0 {
            explanation.push_str(&format!("Partial consistency (score: {:.2}), some minor conflicts.\n", consistency_score.score));
        }

        if let Some(ref net) = self.bayesian_network {
            let evidence: HashMap<String, f64> = HashMap::new();
            let stmt_str = format!("{}", hypothesis.statement);
            for (node_id, node) in &net.nodes {
                if stmt_str.contains(node_id) {
                    let posterior = node.posterior();
                    let map_est = posterior.map_estimate();
                    explanation.push_str(&format!("Bayesian evidence for '{}': MAP estimate = {:.4}\n", node_id, map_est));
                    if map_est < 0.1 && hypothesis.confidence > 0.5 {
                        explanation.push_str(&format!("  → Bayesian evidence contradicts hypothesis (MAP={:.4})\n", map_est));
                        return VerificationResult::contradicted(explanation);
                    }
                    break;
                }
            }
        }

        let combined_conf = hypothesis.confidence * consistency_score.score;
        explanation.push_str(&format!("Combined confidence: {:.2} (hypothesis: {:.2} × consistency: {:.2})\n",
            combined_conf, hypothesis.confidence, consistency_score.score));

        if combined_conf > 0.6 {
            VerificationResult::consistent(combined_conf, explanation)
        } else if combined_conf > 0.3 {
            VerificationResult::uncertain(explanation)
        } else {
            VerificationResult::contradicted(explanation)
        }
    }

    pub fn suggest_correction(&self, hypothesis: &Hypothesis) -> Option<CorrectionSuggestion> {
        let mut best_suggestion: Option<CorrectionSuggestion> = None;
        let mut best_conf = 0.0;

        for rule in self.consistency_engine.rules() {
            if scirust_reasoning::prove_equal(&hypothesis.statement, &rule.consequent) {
                let corrected = rule.antecedent.clone();
                let conf = rule.confidence;
                if conf > best_conf {
                    best_conf = conf;
                    best_suggestion = Some(CorrectionSuggestion::new(
                        hypothesis.clone(), corrected, conf,
                        &format!("Adjusting to match rule '{}' (confidence: {:.2})", rule, conf),
                    ));
                }
            }
        }
        best_suggestion
    }
}

// ---------------------------------------------------------------------------
// Hypothesis Loop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HypothesisLoopResult {
    pub final_hypothesis: Hypothesis,
    pub verification: VerificationResult,
    pub corrections: Vec<CorrectionSuggestion>,
    pub memory: InferenceMemory,
}

pub struct HypothesisLoop {
    verifier: HypothesisVerifier,
    memory: InferenceMemory,
}

impl HypothesisLoop {
    pub fn new(bayesian_network: Option<BayesianNetwork>, consistency_engine: ConsistencyEngine) -> Self {
        Self {
            verifier: HypothesisVerifier::new(bayesian_network, consistency_engine),
            memory: InferenceMemory::new(),
        }
    }

    pub fn run(&mut self, mut hypothesis: Hypothesis) -> HypothesisLoopResult {
        let mut corrections = Vec::new();

        for iteration in 0..self.verifier.max_iterations {
            self.memory.push_step(
                format!("step_{}", iteration + 1),
                hypothesis.statement.clone(),
                hypothesis.confidence,
                iteration < self.verifier.max_iterations - 1,
            );

            let result = self.verifier.verify(&hypothesis);

            if result.verdict != Verdict::Contradicted {
                return HypothesisLoopResult {
                    final_hypothesis: hypothesis,
                    verification: result,
                    corrections,
                    memory: self.memory.clone(),
                };
            }

            if let Some(suggestion) = self.verifier.suggest_correction(&hypothesis) {
                let corrected_hyp = Hypothesis::new(
                    suggestion.corrected_statement.clone(),
                    suggestion.confidence,
                    HypothesisSource::Deduced,
                );
                corrections.push(suggestion);
                hypothesis = corrected_hyp;
            } else {
                return HypothesisLoopResult {
                    final_hypothesis: hypothesis,
                    verification: result,
                    corrections,
                    memory: self.memory.clone(),
                };
            }
        }

        let result = self.verifier.verify(&hypothesis);
        HypothesisLoopResult {
            final_hypothesis: hypothesis,
            verification: result,
            corrections,
            memory: self.memory.clone(),
        }
    }

    pub fn memory(&self) -> &InferenceMemory { &self.memory }
    pub fn memory_mut(&mut self) -> &mut InferenceMemory { &mut self.memory }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_symbolic::parse;

    #[test]
    fn test_hypothesis_creation() {
        let expr = parse("x - 5").unwrap();
        let h = Hypothesis::from_llm(expr);
        assert_eq!(h.source, HypothesisSource::LLM);
        assert!((h.confidence - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_verification_consistent() {
        let engine = ConsistencyEngine::new();
        let verifier = HypothesisVerifier::new(None, engine);
        let h = Hypothesis::from_observation(parse("x").unwrap());
        let result = verifier.verify(&h);
        assert_eq!(result.verdict, Verdict::Consistent);
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn test_verification_contradicted() {
        let mut engine = ConsistencyEngine::new();
        engine.add_fact(parse("x").unwrap());
        let verifier = HypothesisVerifier::new(None, engine);
        let h = Hypothesis::from_llm(parse("-x").unwrap());
        let result = verifier.verify(&h);
        assert!(result.verdict == Verdict::Contradicted || result.verdict == Verdict::Uncertain);
    }

    #[test]
    fn test_correction_suggestion() {
        let mut engine = ConsistencyEngine::new();
        engine.add_rule(Rule::implies(parse("x").unwrap(), parse("y").unwrap(), 0.9));
        let verifier = HypothesisVerifier::new(None, engine);
        let h = Hypothesis::from_llm(parse("y").unwrap());
        let suggestion = verifier.suggest_correction(&h);
        assert!(suggestion.is_some());
    }

    #[test]
    fn test_hypothesis_loop() {
        let mut engine = ConsistencyEngine::new();
        engine.add_fact(parse("temperature - 30").unwrap());
        engine.add_rule(Rule::implies(parse("temperature - 30").unwrap(), parse("temperature - 0.5").unwrap(), 0.9));

        let mut loop_ = HypothesisLoop::new(None, engine);
        let h = Hypothesis::from_llm(parse("-(temperature - 30)").unwrap());
        let result = loop_.run(h);

        assert!(!result.corrections.is_empty() || result.verification.verdict != Verdict::Contradicted);
    }

    #[test]
    fn test_hypothesis_from_sources() {
        let expr = parse("x - 5").unwrap();
        let _llm = Hypothesis::from_llm(expr.clone());
        let _obs = Hypothesis::from_observation(expr.clone());
        let _ded = Hypothesis::from_deduced(expr, 0.8);
        assert!(true);
    }
}
