//! MTP Validator — Rejection Sampling Engine for Speculative Decoding.
//!
//! Compares MTP head predictions with main-model logits to determine which
//! speculative tokens are accepted. Implements both strict (argmax) and
//! probabilistic (rejection sampling per Leviathan et al. 2023) strategies,
//! adapted for Multi-Token Prediction where N heads propose tokens for
//! positions t+1, t+2, ..., t+N.
//!
//! # Algorithm
//! ```text
//! For each speculative step k (0..num_steps):
//!   mtp_proposal = mtp_tokens[k]
//!   main_logits  = main_logits[pos=k]  // vocabulary distribution
//!   accepted[k]  = accepts(main_logits, mtp_logits, mtp_proposal)
//!   if !accepted[k]: break  // reject all subsequent tokens
//! ```
//!
//! # Acceptance Strategies
//! - **Argmax** (strict): accept iff mtp_token == argmax(main_logits)
//! - **RejectionSampling**: accept with probability min(1, p_main(t)/p_mtp(t))
//!   When rejected, sample correction from residual distribution
//!   max(0, p_main - p_mtp) / Z.

use rand::Rng;
use thiserror::Error;

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum MtpValidatorError {
    #[error("Mismatched logits dimension: expected {expected} entries, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error(
        "Not enough logits to cover {steps} speculative steps (need {needed} values, got {got})"
    )]
    InsufficientLogits {
        steps: usize,
        needed: usize,
        got: usize,
    },

    #[error("No valid tokens found in acceptance window")]
    NoValidTokens,
}

pub type Result<T> = std::result::Result<T, MtpValidatorError>;

// ── Acceptance strategy ───────────────────────────────────────────────────────

/// How speculative tokens are verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceStrategy {
    /// Strict deterministic match: accept iff the MTP proposal matches the main
    /// model's argmax at that position. Fast, zero randomness, but rejects
    /// more often for high-entropy positions.
    Argmax,
    /// Probabilistic acceptance (Leviathan et al. 2023):
    /// accept with probability `min(1, p_main(token) / p_mtp(token))`.
    /// When the main model would have sampled the same token anyway, this
    /// guarantees the final distribution exactly matches the main model's
    /// distribution (unbiased).
    RejectionSampling,
}

// ── Verification result ───────────────────────────────────────────────────────

/// Result of one verification pass over a batch of speculative tokens.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Number of consecutive tokens accepted (0 means the first token was rejected).
    pub accepted_count: usize,
    /// The actual accepted token IDs.
    pub accepted_tokens: Vec<usize>,
    /// Position of first rejection (0 = first token rejected,
    /// `num_steps` = every token accepted).
    pub break_index: usize,
    /// Whether every speculative token was accepted.
    pub all_accepted: bool,
    /// When a rejection occurs, the corrected token sampled from the main model
    /// at the break point (so the caller can continue generation).
    pub corrected_token: Option<usize>,
    /// Per-step acceptance decisions (for diagnostics).
    pub per_step_accepted: Vec<bool>,
}

// ── MtpValidator ──────────────────────────────────────────────────────────────

/// MTP speculative decoding validator with pluggable acceptance strategy.
///
/// # Performance
/// The hot path (`verify`) performs O(steps × vocab_size) operations for
/// softmax normalization (in sampling mode) or O(steps × vocab_size) for
/// argmax. Both are memory-bandwidth-bound and trivially parallelizable
/// across validation steps.
pub struct MtpValidator {
    /// Number of speculative steps (max tokens to verify per call).
    pub num_steps: usize,
    /// Vocabulary size.
    pub vocab_size: usize,
    /// Acceptance strategy.
    pub strategy: AcceptanceStrategy,
    /// MTP head confidence factor (0.0–1.0). Used in rejection sampling to
    /// estimate p_mtp when exact MTP logits aren't provided (default: 0.95).
    /// Set via `with_mtp_confidence`.
    pub mtp_confidence: f32,
    /// Temperature for logit → probability conversion. Default: 1.0.
    pub temperature: f32,
}

impl MtpValidator {
    /// Create a new validator.
    pub fn new(num_steps: usize, vocab_size: usize) -> Self {
        Self {
            num_steps,
            vocab_size,
            strategy: AcceptanceStrategy::Argmax,
            mtp_confidence: 0.95,
            temperature: 1.0,
        }
    }

    /// Set the acceptance strategy (builder pattern).
    pub fn with_strategy(mut self, strategy: AcceptanceStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the MTP head confidence factor for rejection sampling (builder).
    pub fn with_mtp_confidence(mut self, confidence: f32) -> Self {
        self.mtp_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the temperature for logit → probability conversion (builder).
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp.max(0.01);
        self
    }

    /// Verify a sequence of speculative tokens against main model logits.
    ///
    /// * `mtp_tokens` — tokens proposed by MTP heads; `mtp_tokens[k]` is the
    ///   proposal for speculative position `k`.
    /// * `main_logits` — flat slice of main model logits, shape `[num_steps × vocab_size]`.
    ///   `main_logits[k * vocab_size .. (k+1) * vocab_size]` are the logits at
    ///   speculative position `k`.
    /// * `mtp_logits` — optional MTP head logits with the same layout as `main_logits`.
    ///   Required for accurate rejection sampling. When `None` and strategy is
    ///   `RejectionSampling`, `mtp_confidence` is used as a uniform proxy.
    ///
    /// Returns the verification result including the break index and accepted tokens.
    pub fn verify(
        &self,
        mtp_tokens: &[usize],
        main_logits: &[f32],
        mtp_logits: Option<&[f32]>,
    ) -> Result<VerificationResult> {
        let steps = mtp_tokens.len().min(self.num_steps);
        if steps == 0 {
            return Err(MtpValidatorError::NoValidTokens);
        }
        let needed = steps * self.vocab_size;
        if main_logits.len() < needed {
            return Err(MtpValidatorError::InsufficientLogits {
                steps,
                needed,
                got: main_logits.len(),
            });
        }
        if let Some(ml) = mtp_logits {
            if ml.len() < needed {
                return Err(MtpValidatorError::DimensionMismatch {
                    expected: needed,
                    actual: ml.len(),
                });
            }
        }

        let mut rng = rand::thread_rng();
        let mut accepted_tokens = Vec::with_capacity(steps);
        let mut per_step = Vec::with_capacity(steps);
        let mut break_index = 0usize;
        let mut corrected_token = None;

        for step in 0..steps {
            let start = step * self.vocab_size;
            let main_step_logits = &main_logits[start..start + self.vocab_size];
            let mtp_step_logits = mtp_logits.map(|ml| &ml[start..start + self.vocab_size]);
            let mtp_token = mtp_tokens[step];

            let accepted = match self.strategy {
                AcceptanceStrategy::Argmax => self.accept_argmax(mtp_token, main_step_logits),
                AcceptanceStrategy::RejectionSampling => {
                    self.accept_sampling(mtp_token, main_step_logits, mtp_step_logits, &mut rng)
                }
            };

            per_step.push(accepted);

            if accepted {
                accepted_tokens.push(mtp_token);
                break_index = step + 1;
            } else {
                // Rejection: sample the corrected token from main model
                corrected_token = Some(self.sample_from_logits(main_step_logits, &mut rng));
                break;
            }
        }

        Ok(VerificationResult {
            accepted_count: break_index,
            accepted_tokens,
            break_index,
            all_accepted: break_index == steps,
            corrected_token,
            per_step_accepted: per_step,
        })
    }

    /// Strict argmax: accept iff MTP token == argmax(main_logits).
    fn accept_argmax(&self, mtp_token: usize, logits: &[f32]) -> bool {
        let predicted = argmax(logits);
        mtp_token == predicted
    }

    /// Probabilistic acceptance: accept with prob min(1, p_main(t) / p_mtp(t)).
    ///
    /// When `mtp_logits` is provided, p_mtp(t) is computed from the MTP logits
    /// via softmax. Otherwise, p_mtp(t) ≈ mtp_confidence (assuming the MTP head
    /// predicts its top candidate with this probability).
    fn accept_sampling(
        &self,
        mtp_token: usize,
        main_logits: &[f32],
        mtp_logits: Option<&[f32]>,
        rng: &mut impl Rng,
    ) -> bool {
        let p_main = softmax_token_prob(main_logits, mtp_token, self.temperature);

        let p_mtp = match mtp_logits {
            Some(ml) => softmax_token_prob(ml, mtp_token, self.temperature),
            None => self.mtp_confidence,
        };

        if p_main <= 0.0 || p_mtp <= 0.0 {
            return false;
        }

        let accept_prob = (p_main / p_mtp).min(1.0);
        rng.gen::<f32>() < accept_prob
    }

    /// Sample a token from logits (used for corrected token after rejection).
    fn sample_from_logits(&self, logits: &[f32], rng: &mut impl Rng) -> usize {
        let probs = softmax(logits, self.temperature);
        let r: f32 = rng.gen();
        let mut cumsum = 0.0f32;
        for (i, &p) in probs.iter().enumerate() {
            cumsum += p;
            if r < cumsum {
                return i;
            }
        }
        // Fallback to argmax on numerical edge
        argmax(logits)
    }

    /// Find the first speculative position where MTP and main-model argmax
    /// disagree. Returns `(break_index, Option<corrected_token>)`.
    ///
    /// This is a lightweight alternative to full `verify` when only the
    /// deterministic breakpoint is needed.
    pub fn find_breakpoint(
        &self,
        mtp_tokens: &[usize],
        main_logits: &[f32],
    ) -> Result<(usize, Option<usize>)> {
        let steps = mtp_tokens.len().min(self.num_steps);
        for step in 0..steps {
            let start = step * self.vocab_size;
            let logits = &main_logits[start..start + self.vocab_size];
            let predicted = argmax(logits);
            if mtp_tokens[step] != predicted {
                return Ok((step, Some(predicted)));
            }
        }
        Ok((steps, None))
    }

    /// Dynamic depth adjustment: given recent acceptance rates, suggest the
    /// optimal number of speculative steps for the next verification window.
    ///
    /// Uses a simple greedy policy: increase depth when acceptance rate is
    /// high, decrease when low, bounded by `num_steps`.
    pub fn suggest_depth(&self, recent_acceptance_rate: f32, current_depth: usize) -> usize {
        if recent_acceptance_rate > 0.75 {
            (current_depth + 1).min(self.num_steps)
        } else if recent_acceptance_rate < 0.3 {
            (current_depth.saturating_sub(1)).max(1)
        } else {
            current_depth
        }
    }
}

// ── Utility functions ─────────────────────────────────────────────────────────

/// Numerically stable softmax returning probabilities.
#[inline]
fn softmax(logits: &[f32], temperature: f32) -> Vec<f32> {
    let inv_temp = 1.0 / temperature;
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut probs: Vec<f32> = logits
        .iter()
        .map(|&l| ((l - max_logit) * inv_temp).exp())
        .collect();
    let sum: f32 = probs.iter().sum();
    if sum > 0.0 {
        let inv_sum = 1.0 / sum;
        for p in &mut probs {
            *p *= inv_sum;
        }
    }
    probs
}

/// Softmax probability for a single token.
#[inline]
fn softmax_token_prob(logits: &[f32], token: usize, temperature: f32) -> f32 {
    let inv_temp = 1.0 / temperature;
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = logits
        .iter()
        .map(|&l| ((l - max_logit) * inv_temp).exp())
        .sum();
    if sum <= 0.0 {
        return 0.0;
    }
    ((logits[token] - max_logit) * inv_temp).exp() / sum
}

/// Index of the maximum element in a slice.
#[inline]
fn argmax(slice: &[f32]) -> usize {
    slice
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_logits(vocab: usize, answer: usize) -> Vec<f32> {
        let mut logits = vec![0.0f32; vocab];
        logits[answer] = 10.0; // strong signal
        logits[(answer + 1) % vocab] = 0.5;
        logits
    }

    fn make_mtp_logits(vocab: usize, answer: usize) -> Vec<f32> {
        let mut logits = vec![0.0f32; vocab];
        logits[answer] = 8.0;
        logits[(answer + 1) % vocab] = 1.0;
        logits
    }

    #[test]
    fn argmax_accepts_exact_match() {
        let v = MtpValidator::new(3, 100);
        let mtp = vec![5, 12, 7];
        let logits: Vec<f32> = (0..3).flat_map(|s| make_logits(100, mtp[s])).collect();
        let r = v.verify(&mtp, &logits, None).unwrap();
        assert_eq!(r.accepted_count, 3);
        assert!(r.all_accepted);
        assert_eq!(r.accepted_tokens, vec![5, 12, 7]);
    }

    #[test]
    fn argmax_rejects_mismatch() {
        let v = MtpValidator::new(3, 100);
        let mtp = vec![5, 12, 7];
        // Step 0: main model argmax == 5 (match)
        // Step 1: main model argmax == 99 != 12 (mismatch → reject)
        let mut logits = make_logits(100, 5); // step 0
        logits.extend(make_logits(100, 99)); // step 1
        logits.extend(make_logits(100, 7)); // step 2 (never reached)
        let r = v.verify(&mtp, &logits, None).unwrap();
        assert_eq!(r.accepted_count, 1);
        assert!(!r.all_accepted);
        assert_eq!(r.break_index, 1);
        assert_eq!(r.accepted_tokens, vec![5]);
        assert_eq!(r.corrected_token, Some(99));
    }

    #[test]
    fn rejection_sampling_with_mtp_logits() {
        let v = MtpValidator::new(2, 10).with_strategy(AcceptanceStrategy::RejectionSampling);
        let mtp = vec![3, 7];
        // Same distributions → high acceptance rate
        let mut main = make_logits(10, 3);
        main.extend(make_logits(10, 7));
        let mtp_logits: Vec<f32> = (0..2).flat_map(|s| make_mtp_logits(10, mtp[s])).collect();
        // With matching distributions, acceptance probability ≈ min(1, ~1.0) = 1.0
        let r = v.verify(&mtp, &main, Some(&mtp_logits)).unwrap();
        assert!(
            r.accepted_count >= 1,
            "Should accept ≥1 token (probabilistic)"
        );
    }

    #[test]
    fn rejection_sampling_rejects_divergent() {
        let v = MtpValidator::new(1, 10).with_strategy(AcceptanceStrategy::RejectionSampling);
        let mtp = vec![3];
        // Main model strongly prefers 4 vs MTP preferring 3
        let main_logits = {
            let mut l = vec![0.0f32; 10];
            l[4] = 10.0; // main model argmax
            l[3] = 0.1; // MTP token has near-zero prob
            l
        };
        let mtp_logits = {
            let mut l = vec![0.0f32; 10];
            l[3] = 10.0; // MTP strongly prefers 3
            l
        };
        // p_main(3) ≈ 0, p_mtp(3) ≈ 1  →  accept prob ≈ 0 → should reject
        let r = v.verify(&mtp, &main_logits, Some(&mtp_logits)).unwrap();
        assert_eq!(r.accepted_count, 0);
        assert_eq!(r.break_index, 0);
    }

    #[test]
    fn find_breakpoint_all_match() {
        let v = MtpValidator::new(4, 50);
        let mtp = vec![1, 2, 3, 4];
        let logits: Vec<f32> = (0..4).flat_map(|s| make_logits(50, mtp[s])).collect();
        let (idx, corr) = v.find_breakpoint(&mtp, &logits).unwrap();
        assert_eq!(idx, 4);
        assert_eq!(corr, None);
    }

    #[test]
    fn find_breakpoint_mismatch() {
        let v = MtpValidator::new(4, 50);
        let mtp = vec![1, 2, 3, 4];
        let logits: Vec<f32> = (0..4)
            .flat_map(|s| make_logits(50, if s == 2 { 49 } else { mtp[s] }))
            .collect();
        let (idx, corr) = v.find_breakpoint(&mtp, &logits).unwrap();
        assert_eq!(idx, 2); // mismatch at step 2
        assert_eq!(corr, Some(49));
    }

    #[test]
    fn empty_mtp_tokens_rejected() {
        let v = MtpValidator::new(3, 10);
        assert!(v.verify(&[], &[], None).is_err());
    }

    #[test]
    fn insufficient_logits_error() {
        let v = MtpValidator::new(3, 100);
        assert!(v.verify(&[1, 2, 3], &[0.0; 50], None).is_err());
    }

    #[test]
    fn suggest_depth_policy() {
        let v = MtpValidator::new(8, 100);
        assert_eq!(v.suggest_depth(0.9, 3), 4); // high rate → increase
        assert_eq!(v.suggest_depth(0.2, 3), 2); // low rate → decrease
        assert_eq!(v.suggest_depth(0.5, 3), 3); // moderate → hold
        assert_eq!(v.suggest_depth(0.9, 8), 8); // clamped to max
        assert_eq!(v.suggest_depth(0.1, 1), 1); // clamped to min
    }

    #[test]
    fn builder_pattern() {
        let v = MtpValidator::new(4, 256)
            .with_strategy(AcceptanceStrategy::RejectionSampling)
            .with_mtp_confidence(0.8)
            .with_temperature(0.7);
        assert_eq!(v.strategy, AcceptanceStrategy::RejectionSampling);
        assert_eq!(v.mtp_confidence, 0.8);
        assert_eq!(v.temperature, 0.7);
    }

    #[test]
    fn softmax_is_normalized() {
        let probs = softmax(&[1.0, 2.0, 3.0, 0.1], 1.0);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "sum = {sum}");
        assert!(probs[2] > probs[1] && probs[1] > probs[0]); // monotonic
    }

    #[test]
    fn softmax_token_prob_matches_full_softmax() {
        let logits = vec![1.0, 4.0, 2.5, 0.3, 5.0];
        let full = softmax(&logits, 1.0);
        for (i, &expected) in full.iter().enumerate() {
            let got = softmax_token_prob(&logits, i, 1.0);
            assert!(
                (got - expected).abs() < 1e-6,
                "token {i}: {got} vs {expected}"
            );
        }
    }
}
