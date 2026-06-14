//! Learned, cost-aware routing.
//!
//! The capability router in [`crate::router`] is a good heuristic floor. This
//! module adds the layer the research frontier calls for
//! (`docs/RESEARCH_FRONTIER_2026.md` §2): a *learned* difficulty predictor that
//! decides how strong a model a query actually needs, then a *cost-aware*
//! selection that picks the **cheapest model that clears the predicted bar** —
//! the RouteLLM idea (strong/weak deferral, Zhuang et al. 2024) generalized to
//! an N-model cascade, with cost-aware contrastive selection and
//! uncertainty-based escalation.
//!
//! Design choices that matter here:
//! - **Out-of-the-box useful, then learned.** [`DifficultyModel::default`] ships
//!   sensible prior weights so routing is reasonable with zero training; calling
//!   [`DifficultyModel::train`] on logged outcomes sharpens it (RouteLLM's
//!   preference-data training).
//! - **Self-improvable.** All tunables live in a serializable [`RouterParams`],
//!   and [`CostAwareRouter::evaluate`] is a deterministic offline score, so a
//!   `soul-rsi` evaluator can treat a parameter set as a Variant and keep only
//!   measured improvements — no LLM or network in the loop.
//! - **No new dependencies.** Pure `std` + `serde`.

use crate::{Capability, ModelProfile, ModelRegistry};
use serde::{Deserialize, Serialize};

/// Number of query features the difficulty model consumes.
pub const N_FEATURES: usize = 8;

/// Cheap, deterministic features extracted from a query + its required
/// capabilities. No tokenizer or model needed — every feature is in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueryFeatures(pub [f64; N_FEATURES]);

impl QueryFeatures {
    /// Extract features from a prompt and the capabilities it requests.
    pub fn extract(query: &str, capabilities: &[Capability]) -> Self {
        let lower = query.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        let word_count = words.len().max(1);

        // 0. Length / token estimate (log-scaled, saturating ~2k tokens).
        let token_est = query.len() as f64 / 4.0;
        let f_len = (token_est.max(1.0).ln() / 8.0).clamp(0.0, 1.0);

        // 1. Code signal.
        let code_markers = [
            "```", "fn ", "def ", "class ", "impl ", "{", "};", "=>", "import ",
        ];
        let f_code = saturating_hits(&lower, &code_markers, 3);

        // 2. Reasoning signal.
        let reason_markers = [
            "why",
            "prove",
            "analyz",
            "explain",
            "design",
            "root cause",
            "derive",
            "reason",
        ];
        let f_reason = saturating_hits(&lower, &reason_markers, 2);

        // 3. Multi-step signal.
        let step_markers = [
            "step by step",
            "then ",
            "after that",
            "first ",
            "finally",
            "and then",
        ];
        let f_steps = saturating_hits(&lower, &step_markers, 2);

        // 4. Clause complexity (commas + conjunctions per word).
        let clauses = lower.matches(',').count()
            + count_words(&words, &["and", "but", "because", "however", "if", "while"]);
        let f_clauses = (clauses as f64 / word_count as f64 * 4.0).clamp(0.0, 1.0);

        // 5. Hardest requested capability.
        let f_cap_hard = capabilities
            .iter()
            .map(capability_difficulty)
            .fold(0.0_f64, f64::max);

        // 6. Number of distinct capabilities requested.
        let f_cap_count = (capabilities.len() as f64 / 4.0).clamp(0.0, 1.0);

        // 7. Average word length (rare/technical vocabulary proxy).
        let avg_word_len = words.iter().map(|w| w.len()).sum::<usize>() as f64 / word_count as f64;
        let f_vocab = ((avg_word_len - 3.0) / 6.0).clamp(0.0, 1.0);

        Self([
            f_len,
            f_code,
            f_reason,
            f_steps,
            f_clauses,
            f_cap_hard,
            f_cap_count,
            f_vocab,
        ])
    }
}

fn saturating_hits(haystack: &str, needles: &[&str], saturate_at: usize) -> f64 {
    let hits = needles.iter().filter(|n| haystack.contains(**n)).count();
    (hits as f64 / saturate_at as f64).clamp(0.0, 1.0)
}

fn count_words(words: &[&str], targets: &[&str]) -> usize {
    words.iter().filter(|w| targets.contains(w)).count()
}

/// Intrinsic difficulty weight of a capability in `[0, 1]`.
fn capability_difficulty(cap: &Capability) -> f64 {
    match cap {
        Capability::FastChat => 0.1,
        Capability::Summarization => 0.3,
        Capability::Creative => 0.4,
        Capability::CodeGeneration => 0.6,
        Capability::CodeReview => 0.8,
        Capability::Planning => 0.85,
        Capability::Analysis => 0.9,
    }
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// Logistic difficulty predictor: `P(needs a strong model)` in `[0, 1]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DifficultyModel {
    pub weights: [f64; N_FEATURES],
    pub bias: f64,
}

impl Default for DifficultyModel {
    /// Hand-set prior weights so routing is sensible before any training:
    /// length, reasoning, multi-step, clause-complexity and capability hardness
    /// push toward "strong"; code and vocabulary contribute mildly.
    fn default() -> Self {
        Self {
            //    len  code reason step clause capHard capN vocab
            weights: [0.8, 0.6, 1.3, 1.1, 0.9, 1.6, 0.7, 0.5],
            bias: -1.6,
        }
    }
}

impl DifficultyModel {
    /// Predicted probability that the query needs a strong model.
    pub fn predict(&self, f: &QueryFeatures) -> f64 {
        let z: f64 = self
            .weights
            .iter()
            .zip(f.0.iter())
            .map(|(w, x)| w * x)
            .sum::<f64>()
            + self.bias;
        sigmoid(z)
    }

    /// Train on labelled samples by logistic-regression gradient descent.
    /// `label = 1.0` means "the strong model was needed/won", `0.0` means the
    /// weak model sufficed — exactly RouteLLM's preference signal. Returns the
    /// mean log-loss after the final epoch.
    pub fn train(&mut self, samples: &[(QueryFeatures, f64)], epochs: usize, lr: f64) -> f64 {
        if samples.is_empty() {
            return f64::NAN;
        }
        for _ in 0..epochs {
            let mut grad_w = [0.0_f64; N_FEATURES];
            let mut grad_b = 0.0_f64;
            for (f, y) in samples {
                let p = self.predict(f);
                let err = p - *y; // dL/dz for logistic loss
                for (g, x) in grad_w.iter_mut().zip(f.0.iter()) {
                    *g += err * x;
                }
                grad_b += err;
            }
            let n = samples.len() as f64;
            for (w, g) in self.weights.iter_mut().zip(grad_w.iter()) {
                *w -= lr * g / n;
            }
            self.bias -= lr * grad_b / n;
        }
        self.log_loss(samples)
    }

    /// Mean log-loss over samples (lower is better).
    #[allow(clippy::suboptimal_flops)] // readability over fused-multiply-add here
    pub fn log_loss(&self, samples: &[(QueryFeatures, f64)]) -> f64 {
        if samples.is_empty() {
            return f64::NAN;
        }
        let eps = 1e-9;
        let total: f64 = samples
            .iter()
            .map(|(f, y)| {
                let p = self.predict(f).clamp(eps, 1.0 - eps);
                -(y * p.ln() + (1.0 - y) * (1.0 - p).ln())
            })
            .sum();
        total / samples.len() as f64
    }
}

/// All tunable routing parameters — the unit a `soul-rsi` loop evolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouterParams {
    pub model: DifficultyModel,
    /// Difficulty at/above which a query is considered "hard" (for the
    /// strong-fraction calibration and the uncertainty band).
    pub threshold: f64,
    /// In `[0, 1]`: how much to trade quality away for cost. 0 = always meet the
    /// predicted quality bar; 1 = strongly prefer cheap models.
    pub cost_aversion: f64,
    /// Half-width of the band around `threshold` within which a decision is
    /// flagged `uncertain` (caller may escalate / sample — the uncertainty-based
    /// routing idea).
    pub uncertainty_margin: f64,
}

impl Default for RouterParams {
    fn default() -> Self {
        Self {
            model: DifficultyModel::default(),
            threshold: 0.5,
            cost_aversion: 0.25,
            uncertainty_margin: 0.1,
        }
    }
}

/// A routing decision with the reasoning needed to audit it.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub profile: ModelProfile,
    /// Predicted difficulty in `[0, 1]`.
    pub difficulty: f64,
    /// The quality bar the difficulty implied (in the candidates' quality range).
    pub target_quality: f64,
    /// True when difficulty sits inside the uncertainty band around the
    /// threshold — the caller may want to escalate or sample to be safe.
    pub uncertain: bool,
    pub reason: String,
}

/// Offline routing quality, à la LLMRouterBench: how well the router trades cost
/// for correctness on a labelled set.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingMetrics {
    /// Fraction of queries the router sent to a strong (above-median quality)
    /// model.
    pub strong_fraction: f64,
    /// Mean effective cost per query of the routed choices.
    pub avg_cost: f64,
    /// Fraction of queries where routing strong⇔hard matched the label.
    pub accuracy: f64,
    pub samples: usize,
}

/// Effective per-query cost: dollars per 1k tokens plus a latency surcharge, so
/// the router stays cost-aware even across a fleet of free local models (where
/// `cost_per_1k_tokens == 0` and latency is the real cost).
#[allow(clippy::suboptimal_flops)] // readability over fused-multiply-add here
fn effective_cost(p: &ModelProfile) -> f64 {
    p.cost_per_1k_tokens * 1000.0 + p.avg_latency_ms as f64 / 1000.0
}

/// Quality proxy: capability breadth plus a mild context-window bonus.
fn model_quality(p: &ModelProfile) -> f64 {
    p.capabilities.len() as f64 + (p.max_tokens as f64 / 100_000.0)
}

/// The learned, cost-aware router.
#[derive(Debug, Clone)]
pub struct CostAwareRouter {
    registry: ModelRegistry,
    params: RouterParams,
}

impl CostAwareRouter {
    pub fn new(registry: ModelRegistry, params: RouterParams) -> Self {
        Self { registry, params }
    }

    /// Build over the embedded default model fleet with default parameters.
    pub fn with_defaults() -> Self {
        Self {
            registry: ModelRegistry::with_defaults(),
            params: RouterParams::default(),
        }
    }

    pub fn params(&self) -> &RouterParams {
        &self.params
    }

    pub fn set_params(&mut self, params: RouterParams) {
        self.params = params;
    }

    pub fn registry_mut(&mut self) -> &mut ModelRegistry {
        &mut self.registry
    }

    fn candidates(&self, capabilities: &[Capability]) -> Vec<ModelProfile> {
        let all = self.registry.list_all();
        let filtered: Vec<ModelProfile> = if capabilities.is_empty() {
            all.into_iter().cloned().collect()
        } else {
            all.into_iter()
                .filter(|p| capabilities.iter().all(|c| p.capabilities.contains(c)))
                .cloned()
                .collect()
        };
        filtered
    }

    /// Route a query: predict difficulty, derive a quality bar (lowered by
    /// `cost_aversion`), then pick the cheapest candidate that clears the bar.
    pub fn route(&self, query: &str, capabilities: &[Capability]) -> Option<RoutingDecision> {
        let candidates = self.candidates(capabilities);
        if candidates.is_empty() {
            return None;
        }

        let features = QueryFeatures::extract(query, capabilities);
        let difficulty = self.params.model.predict(&features);

        let q_min = candidates
            .iter()
            .map(model_quality)
            .fold(f64::INFINITY, f64::min);
        let q_max = candidates
            .iter()
            .map(model_quality)
            .fold(f64::NEG_INFINITY, f64::max);

        // Cost aversion lowers the effective difficulty, accepting a cheaper
        // (lower-quality) model to save cost.
        let effective_d = (difficulty * (1.0 - self.params.cost_aversion)).clamp(0.0, 1.0);
        let target_quality = q_min + effective_d * (q_max - q_min);

        // Cheapest candidate clearing the bar; if none clears it, take the
        // highest-quality candidate available.
        let chosen = candidates
            .iter()
            .filter(|p| model_quality(p) >= target_quality - 1e-9)
            .min_by(|a, b| {
                cost_key(a)
                    .partial_cmp(&cost_key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| {
                candidates.iter().max_by(|a, b| {
                    model_quality(a)
                        .partial_cmp(&model_quality(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })?
            .clone();

        let uncertain = (difficulty - self.params.threshold).abs() < self.params.uncertainty_margin;

        let reason = format!(
            "difficulty={difficulty:.2} → target_quality={target_quality:.2} \
             (cost_aversion={:.2}); chose '{}' (quality={:.2}, cost={:.3}){}",
            self.params.cost_aversion,
            chosen.name,
            model_quality(&chosen),
            effective_cost(&chosen),
            if uncertain {
                " [uncertain — consider escalation]"
            } else {
                ""
            },
        );

        Some(RoutingDecision {
            profile: chosen,
            difficulty,
            target_quality,
            uncertain,
            reason,
        })
    }

    /// Calibrate `threshold` so that roughly `target_fraction` of the given
    /// queries are classified "hard" (difficulty ≥ threshold). This is the knob
    /// that trades total cost against quality (the deferral-curve operating
    /// point).
    pub fn calibrate_threshold(
        &mut self,
        queries: &[(&str, Vec<Capability>)],
        target_fraction: f64,
    ) {
        if queries.is_empty() {
            return;
        }
        let mut diffs: Vec<f64> = queries
            .iter()
            .map(|(q, caps)| self.params.model.predict(&QueryFeatures::extract(q, caps)))
            .collect();
        diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let frac = target_fraction.clamp(0.0, 1.0);
        // The threshold at the (1 - frac) quantile yields ~frac queries above it.
        let idx = (((1.0 - frac) * diffs.len() as f64).floor() as usize).min(diffs.len() - 1);
        self.params.threshold = diffs[idx];
    }

    /// Score the router offline on labelled samples (`label = 1.0` ⇒ the query
    /// truly needed a strong model). Deterministic — suitable as a `soul-rsi`
    /// evaluation gate.
    pub fn evaluate(&self, samples: &[(&str, Vec<Capability>, f64)]) -> RoutingMetrics {
        if samples.is_empty() {
            return RoutingMetrics {
                strong_fraction: 0.0,
                avg_cost: 0.0,
                accuracy: 0.0,
                samples: 0,
            };
        }
        let median_q = self.median_quality();
        let mut strong = 0usize;
        let mut correct = 0usize;
        let mut total_cost = 0.0;

        for (q, caps, label) in samples {
            let Some(decision) = self.route(q, caps) else {
                continue;
            };
            let routed_strong = model_quality(&decision.profile) >= median_q;
            if routed_strong {
                strong += 1;
            }
            total_cost += effective_cost(&decision.profile);
            let label_strong = *label >= 0.5;
            if routed_strong == label_strong {
                correct += 1;
            }
        }
        let n = samples.len() as f64;
        RoutingMetrics {
            strong_fraction: strong as f64 / n,
            avg_cost: total_cost / n,
            accuracy: correct as f64 / n,
            samples: samples.len(),
        }
    }

    fn median_quality(&self) -> f64 {
        let mut qs: Vec<f64> = self
            .registry
            .list_all()
            .iter()
            .map(|p| model_quality(p))
            .collect();
        if qs.is_empty() {
            return 0.0;
        }
        qs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        qs[qs.len() / 2]
    }
}

/// Cost-first ordering key, tie-broken by priority then latency.
fn cost_key(p: &ModelProfile) -> (f64, u8, u64) {
    (effective_cost(p), p.priority, p.avg_latency_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet() -> ModelRegistry {
        let mut reg = ModelRegistry::new();
        reg.register(ModelProfile {
            name: "tiny:local".into(),
            provider: "ollama".into(),
            base_url: "http://localhost:11434".into(),
            capabilities: vec![
                Capability::FastChat,
                Capability::Summarization,
                Capability::CodeGeneration,
            ],
            max_tokens: 8192,
            cost_per_1k_tokens: 0.0,
            priority: 5,
            avg_latency_ms: 200,
        })
        .unwrap();
        reg.register(ModelProfile {
            name: "mid:cloud".into(),
            provider: "openai".into(),
            base_url: "https://api".into(),
            capabilities: vec![
                Capability::CodeGeneration,
                Capability::Planning,
                Capability::Summarization,
            ],
            max_tokens: 64_000,
            cost_per_1k_tokens: 0.5,
            priority: 10,
            avg_latency_ms: 1500,
        })
        .unwrap();
        reg.register(ModelProfile {
            name: "frontier:cloud".into(),
            provider: "anthropic".into(),
            base_url: "https://api".into(),
            capabilities: vec![
                Capability::CodeGeneration,
                Capability::CodeReview,
                Capability::Planning,
                Capability::Analysis,
            ],
            max_tokens: 200_000,
            cost_per_1k_tokens: 3.0,
            priority: 20,
            avg_latency_ms: 4000,
        })
        .unwrap();
        reg
    }

    #[test]
    fn features_are_bounded_and_responsive() {
        let easy = QueryFeatures::extract("hi", &[Capability::FastChat]);
        let hard = QueryFeatures::extract(
            "Analyze why this distributed system deadlocks, prove the root cause step by step, \
             and design a fix considering consistency and partition tolerance.",
            &[Capability::Analysis, Capability::Planning],
        );
        for x in easy.0.iter().chain(hard.0.iter()) {
            assert!((0.0..=1.0).contains(x), "feature out of range: {x}");
        }
        // The hard query should score higher on reasoning + capability hardness.
        assert!(hard.0[2] > easy.0[2]);
        assert!(hard.0[5] > easy.0[5]);
    }

    #[test]
    fn default_model_separates_easy_from_hard() {
        let m = DifficultyModel::default();
        let easy = m.predict(&QueryFeatures::extract("hi there", &[Capability::FastChat]));
        let hard = m.predict(&QueryFeatures::extract(
            "Analyze and prove the root cause, then design a step by step fix",
            &[Capability::Analysis],
        ));
        assert!(easy < 0.5, "easy={easy}");
        assert!(hard > 0.5, "hard={hard}");
    }

    #[test]
    fn training_reduces_loss_on_separable_data() {
        let samples = vec![
            (QueryFeatures::extract("hi", &[Capability::FastChat]), 0.0),
            (
                QueryFeatures::extract("thanks", &[Capability::FastChat]),
                0.0,
            ),
            (
                QueryFeatures::extract(
                    "analyze and prove the root cause step by step",
                    &[Capability::Analysis],
                ),
                1.0,
            ),
            (
                QueryFeatures::extract(
                    "design a distributed plan, then derive the tradeoffs",
                    &[Capability::Planning, Capability::Analysis],
                ),
                1.0,
            ),
        ];
        let mut m = DifficultyModel::default();
        let before = m.log_loss(&samples);
        let after = m.train(&samples, 400, 0.3);
        assert!(after < before, "loss did not improve: {before} -> {after}");
    }

    #[test]
    fn cost_aware_picks_cheap_for_easy_query() {
        let router = CostAwareRouter::new(fleet(), RouterParams::default());
        let d = router
            .route("hello, thanks!", &[Capability::FastChat])
            .unwrap();
        assert_eq!(d.profile.name, "tiny:local");
    }

    #[test]
    fn cost_aware_escalates_for_hard_query() {
        let router = CostAwareRouter::new(fleet(), RouterParams::default());
        let d = router
            .route(
                "Analyze why the consensus protocol stalls, prove the root cause step by step, \
                 then design a partition-tolerant fix.",
                &[Capability::Analysis],
            )
            .unwrap();
        // Only frontier has Analysis; it must be chosen.
        assert_eq!(d.profile.name, "frontier:cloud");
        assert!(d.difficulty > 0.5);
    }

    #[test]
    fn higher_cost_aversion_prefers_cheaper_models() {
        let q = "Plan and generate a moderately involved code change";
        let caps = [Capability::CodeGeneration, Capability::Planning];

        let cheap_params = RouterParams {
            cost_aversion: 0.95,
            ..Default::default()
        };
        let frugal = CostAwareRouter::new(fleet(), cheap_params);

        let lavish_params = RouterParams {
            cost_aversion: 0.0,
            ..Default::default()
        };
        let lavish = CostAwareRouter::new(fleet(), lavish_params);

        let frugal_cost = effective_cost(&frugal.route(q, &caps).unwrap().profile);
        let lavish_cost = effective_cost(&lavish.route(q, &caps).unwrap().profile);
        assert!(
            frugal_cost <= lavish_cost,
            "frugal={frugal_cost} lavish={lavish_cost}"
        );
    }

    #[test]
    fn calibration_hits_target_strong_fraction() {
        let mut router = CostAwareRouter::new(fleet(), RouterParams::default());
        let queries = vec![
            ("hi", vec![Capability::FastChat]),
            ("thanks", vec![Capability::FastChat]),
            ("summarize this", vec![Capability::Summarization]),
            (
                "analyze and prove the root cause step by step",
                vec![Capability::Analysis],
            ),
        ];
        router.calibrate_threshold(&queries, 0.5);
        // With the threshold at the median difficulty, ~half score above it.
        let above = queries
            .iter()
            .filter(|(q, caps)| {
                router
                    .params
                    .model
                    .predict(&QueryFeatures::extract(q, caps))
                    >= router.params.threshold
            })
            .count();
        assert!((1..=3).contains(&above), "above={above}");
    }

    #[test]
    fn evaluate_reports_accuracy_and_cost() {
        let router = CostAwareRouter::new(fleet(), RouterParams::default());
        let samples = vec![
            ("hi", vec![Capability::FastChat], 0.0),
            ("thanks a lot", vec![Capability::FastChat], 0.0),
            (
                "analyze and prove the root cause step by step then design a fix",
                vec![Capability::Analysis],
                1.0,
            ),
        ];
        let m = router.evaluate(&samples);
        assert_eq!(m.samples, 3);
        assert!(m.accuracy >= 0.66, "accuracy={}", m.accuracy);
        assert!(m.avg_cost >= 0.0);
    }

    #[test]
    fn params_round_trip_through_json() {
        let p = RouterParams::default();
        let s = serde_json::to_string(&p).unwrap();
        let back: RouterParams = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}
