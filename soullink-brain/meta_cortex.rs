//! Meta-Cortex — recursive self-observation, self-modeling, prediction,
//! counterfactual reasoning, and closed-loop feedback control.
//!
//! ## Capabilities
//! 1. **Self-Model** — Online-learned MLP that predicts next brain state
//!    from current state. Learns via SGD on prediction error.
//! 2. **Self-Prediction** — Multi-step rollout using the self-model to
//!    forecast future brain states.
//! 3. **Counterfactual Reasoning** — "What if X changed?" simulation:
//!    virtually applies a modification, runs the self-model forward,
//!    and compares predicted trajectories.
//! 4. **Loop Detection** — Identifies repetitive/stuck patterns in cortex output.
//! 5. **Meta-Guard** — Guardrails preventing harmful self-modification cascades.
//! 6. **Closed-Loop Recursive Control** — Full feedback cycle:
//!    observe → analyze → act → wait → measure outcome → learn → repeat.
//!    Adaptive guardrails tighten/loosen based on intervention success rate.
//!    Recursive depth limiter prevents infinite meta-cascades.

use std::collections::VecDeque;

/// Maximum observations retained for loop detection.
const MAX_OBSERVATIONS: usize = 256;
/// Consecutive pattern repeats before flagging a loop.
const LOOP_DETECTION_THRESHOLD: usize = 5;
/// Cooldown ticks between meta-actions.
const DEFAULT_COOLDOWN: u64 = 10;
/// Max meta-actions per observation window.
const MAX_ACTIONS_PER_WINDOW: usize = 5;
/// Window size for computing rolling success rate.
const SUCCESS_RATE_WINDOW: usize = 20;

// ── Recursive loop constants ──
/// Maximum recursive depth (prevents infinite meta-cascades).
const MAX_RECURSIVE_DEPTH: u32 = 3;
/// Ticks to wait before measuring short-term outcome.
const SHORT_TERM_DELAY: u64 = 10;
/// Ticks to wait before measuring long-term outcome.
const LONG_TERM_DELAY: u64 = 50;
/// Minimum improvement in fitness to count intervention as success.
const MIN_FITNESS_IMPROVEMENT: f64 = 0.01;
/// Maximum pending interventions tracked simultaneously.
const MAX_PENDING_INTERVENTIONS: usize = 8;

/// Minimum total interventions before circuit breaker can trigger.
const INTERVENTION_FREEZE_THRESHOLD: u64 = 20;

/// Harmful ratio threshold (harmful/total) for permanent freeze.
const HARMFUL_RATIO_FREEZE_THRESHOLD: f64 = 0.5;

// ── Self-model constants ──
/// Dimension of the brain state summary used by the self-model.
const SELF_MODEL_INPUT_DIM: usize = 16;
/// Hidden layer size for the self-model MLP.
const SELF_MODEL_HIDDEN_DIM: usize = 32;
/// Default learning rate for online SGD.
const SELF_MODEL_DEFAULT_LR: f64 = 0.01;
/// How often (in ticks) the self-model runs a learning update when in calm mode.
const SELF_MODEL_LEARN_INTERVAL: u64 = 5;
/// Maximum prediction horizon for counterfactuals.
const MAX_PREDICTION_HORIZON: usize = 50;

/// Hash for identifying output patterns.
fn pattern_hash(values: &[f64]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &v in values.iter().take(32) {
        let bits = v.to_bits() as u64;
        h = h.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(bits);
    }
    h
}

/// Shannon entropy of a distribution (normalized to [0,1]).
pub(crate) fn normalized_entropy(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().map(|v| v.abs()).sum();
    if sum < 1e-10 {
        return 0.0;
    }
    let max_entropy = (values.len() as f64).ln();
    if max_entropy < 1e-10 {
        return 0.0;
    }
    let entropy: f64 = values
        .iter()
        .map(|&v| {
            let p = (v.abs() / sum).clamp(1e-10, 1.0);
            -p * p.ln()
        })
        .sum();
    (entropy / max_entropy).clamp(0.0, 1.0)
}

// ═══════════════════════════════════════════════════════════════
// BRAIN STATE SUMMARY — compact feature vector for self-model
// ═══════════════════════════════════════════════════════════════

/// Compact summary of brain state used as input/output for the self-model.
#[derive(Debug, Clone)]
pub struct BrainStateSummary {
    /// Per-module activation levels (11 modules)
    pub module_activations: [f64; 11],
    /// Average energy reserve across all neurons
    pub energy_avg: f64,
    /// Arousal level (0-1)
    pub arousal: f64,
    /// Metabolic pressure (0-1)
    pub metabolic_pressure: f64,
    /// Composite pain signal (0-1)
    pub pain_composite: f64,
    /// Global firing rate (fraction of neurons that fired)
    pub firing_rate: f64,
    /// Intrinsic curiosity novelty score (0-1)
    pub novelty: f64,
}

impl Default for BrainStateSummary {
    fn default() -> Self {
        Self {
            module_activations: [0.0; 11],
            energy_avg: 0.5,
            arousal: 0.5,
            metabolic_pressure: 0.0,
            pain_composite: 0.0,
            firing_rate: 0.0,
            novelty: 0.0,
        }
    }
}

impl BrainStateSummary {
    /// Convert to a flat f64 vector for the self-model.
    pub fn to_vec(&self) -> Vec<f64> {
        let mut v = Vec::with_capacity(SELF_MODEL_INPUT_DIM);
        v.extend_from_slice(&self.module_activations);
        v.push(self.energy_avg);
        v.push(self.arousal);
        v.push(self.metabolic_pressure);
        v.push(self.pain_composite);
        v.push(self.firing_rate);
        v
    }

    /// Build from a flat f64 slice.
    pub fn from_vec(data: &[f64]) -> Self {
        let mut s = Self::default();
        let n = data.len().min(SELF_MODEL_INPUT_DIM);
        if n >= 11 {
            s.module_activations.copy_from_slice(&data[0..11]);
        }
        if n > 11 { s.energy_avg = data[11].clamp(0.0, 1.0); }
        if n > 12 { s.arousal = data[12].clamp(0.0, 1.0); }
        if n > 13 { s.metabolic_pressure = data[13].clamp(0.0, 1.0); }
        if n > 14 { s.pain_composite = data[14].clamp(0.0, 1.0); }
        if n > 15 { s.firing_rate = data[15].clamp(0.0, 1.0); }
        s
    }

    /// Composite health score including curiosity bonus.
    pub fn health_score(&self) -> f64 {
        let energy_w = 0.30;
        let pain_w = 0.30;
        let firing_w = 0.15;
        let arousal_w = 0.15;
        let novelty_w = 0.10;
        (energy_w * self.energy_avg
            + pain_w * (1.0 - self.pain_composite)
            + firing_w * self.firing_rate.min(0.3) / 0.3
            + arousal_w * (1.0 - (self.arousal - 0.5).abs() * 2.0)
            + novelty_w * self.novelty
        ).clamp(0.0, 1.0)
    }

    /// Compute Euclidean distance between two summaries.
    pub fn distance(&self, other: &Self) -> f64 {
        let a = self.to_vec();
        let b = other.to_vec();
        a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt()
    }
}

// ═══════════════════════════════════════════════════════════════
// SELF-MODEL — learned dynamics model (online MLP)
// ═══════════════════════════════════════════════════════════════

/// A lightweight 2-layer MLP that learns to predict next brain state.
/// Trained online via SGD on prediction error.
struct SelfModel {
    /// W1: [HIDDEN × INPUT]
    w1: Vec<Vec<f64>>,
    /// b1: [HIDDEN]
    b1: Vec<f64>,
    /// W2: [INPUT × HIDDEN]
    w2: Vec<Vec<f64>>,
    /// b2: [INPUT]
    b2: Vec<f64>,
    /// Learning rate
    lr: f64,
    /// Running average prediction error
    pub avg_error: f64,
    /// Number of training steps completed
    pub train_steps: u64,
}

impl SelfModel {
    fn new(lr: f64) -> Self {
        let mut rng = rand::rng();
        let scale1 = (1.0 / SELF_MODEL_INPUT_DIM as f64).sqrt();
        let scale2 = (1.0 / SELF_MODEL_HIDDEN_DIM as f64).sqrt();

        let w1: Vec<Vec<f64>> = (0..SELF_MODEL_HIDDEN_DIM)
            .map(|_| {
                (0..SELF_MODEL_INPUT_DIM)
                    .map(|_| rand::Rng::random_range(&mut rng, -scale1..scale1))
                    .collect()
            })
            .collect();
        let b1: Vec<f64> = vec![0.0; SELF_MODEL_HIDDEN_DIM];

        let w2: Vec<Vec<f64>> = (0..SELF_MODEL_INPUT_DIM)
            .map(|_| {
                (0..SELF_MODEL_HIDDEN_DIM)
                    .map(|_| rand::Rng::random_range(&mut rng, -scale2..scale2))
                    .collect()
            })
            .collect();
        let b2: Vec<f64> = vec![0.0; SELF_MODEL_INPUT_DIM];

        Self {
            w1, b1, w2, b2, lr,
            avg_error: 0.0,
            train_steps: 0,
        }
    }

    /// Forward pass: state → predicted next state.
    fn forward(&self, input: &[f64]) -> Vec<f64> {
        // Hidden layer with ReLU
        let mut hidden = vec![0.0; SELF_MODEL_HIDDEN_DIM];
        for i in 0..SELF_MODEL_HIDDEN_DIM {
            let mut sum = self.b1[i];
            for j in 0..SELF_MODEL_INPUT_DIM.min(input.len()) {
                sum += self.w1[i][j] * input[j];
            }
            hidden[i] = sum.max(0.0); // ReLU
        }

        // Output layer (linear)
        let mut output = vec![0.0; SELF_MODEL_INPUT_DIM];
        for i in 0..SELF_MODEL_INPUT_DIM {
            let mut sum = self.b2[i];
            for j in 0..SELF_MODEL_HIDDEN_DIM {
                sum += self.w2[i][j] * hidden[j];
            }
            output[i] = sum;
        }

        output
    }

    /// One step of online SGD: predict next state and update weights.
    /// Returns the prediction error (MSE).
    fn train_step(&mut self, current: &[f64], next: &[f64]) -> f64 {
        let n_in = SELF_MODEL_INPUT_DIM.min(current.len()).min(next.len());

        // ── Forward pass with activation caching ──
        let mut hidden = vec![0.0; SELF_MODEL_HIDDEN_DIM];
        let mut hidden_pre_act = vec![0.0; SELF_MODEL_HIDDEN_DIM];
        for i in 0..SELF_MODEL_HIDDEN_DIM {
            let mut sum = self.b1[i];
            for j in 0..n_in {
                sum += self.w1[i][j] * current[j];
            }
            hidden_pre_act[i] = sum;
            hidden[i] = sum.max(0.0);
        }

        let mut output = [0.0; SELF_MODEL_INPUT_DIM];
        for i in 0..SELF_MODEL_INPUT_DIM {
            let mut sum = self.b2[i];
            for j in 0..SELF_MODEL_HIDDEN_DIM {
                sum += self.w2[i][j] * hidden[j];
            }
            output[i] = sum;
        }

        // ── Compute errors ──
        let mut output_errors = [0.0; SELF_MODEL_INPUT_DIM];
        let mut mse = 0.0;
        for i in 0..n_in {
            output_errors[i] = output[i] - next[i];
            mse += output_errors[i].powi(2);
        }
        mse /= n_in.max(1) as f64;

        // ── Backprop to output layer ──
        for i in 0..SELF_MODEL_INPUT_DIM {
            for j in 0..SELF_MODEL_HIDDEN_DIM {
                self.w2[i][j] -= self.lr * output_errors[i] * hidden[j];
            }
            self.b2[i] -= self.lr * output_errors[i];
        }

        // ── Backprop through ReLU to hidden layer ──
        let mut hidden_errors = vec![0.0; SELF_MODEL_HIDDEN_DIM];
        for j in 0..SELF_MODEL_HIDDEN_DIM {
            let mut grad = 0.0;
            for i in 0..SELF_MODEL_INPUT_DIM {
                grad += output_errors[i] * self.w2[i][j];
            }
            hidden_errors[j] = grad * if hidden_pre_act[j] > 0.0 { 1.0 } else { 0.0 };
        }

        // ── Backprop to input layer ──
        for i in 0..SELF_MODEL_HIDDEN_DIM {
            for j in 0..n_in {
                self.w1[i][j] -= self.lr * hidden_errors[i] * current[j];
            }
            self.b1[i] -= self.lr * hidden_errors[i];
        }

        // ── Update running error estimate ──
        let alpha = 0.95;
        self.avg_error = self.avg_error * alpha + mse * (1.0 - alpha);
        self.train_steps += 1;

        mse
    }

    /// Decay learning rate over time for convergence.
    fn decay_lr(&mut self) {
        let min_lr = 0.0001;
        let decay = 0.9999;
        self.lr = (self.lr * decay).max(min_lr);
    }
}

// ═══════════════════════════════════════════════════════════════
// COUNTERFACTUAL — what-if reasoning
// ═══════════════════════════════════════════════════════════════

/// Describes a hypothetical modification to the brain.
#[derive(Debug, Clone)]
pub struct CounterfactualScenario {
    /// Human-readable description of the change.
    pub description: String,
    /// Parameter changes: (param_name, new_value).
    pub param_changes: Vec<(String, f64)>,
    /// Module-level activation tweaks: (module_name, delta).
    pub module_tweaks: Vec<(String, f64)>,
    /// Global state tweaks: (field_name, delta).
    pub state_tweaks: Vec<(String, f64)>,
}

/// Result of running a counterfactual simulation.
#[derive(Debug, Clone)]
pub struct CounterfactualResult {
    /// The scenario that was tested.
    pub scenario: CounterfactualScenario,
    /// Predicted states under the counterfactual (one per step).
    pub trajectory: Vec<BrainStateSummary>,
    /// Baseline predicted states (without the change).
    pub baseline: Vec<BrainStateSummary>,
    /// Divergence at each step (distance between counterfactual and baseline).
    pub divergence: Vec<f64>,
    /// Final overall impact score (0 = no effect, 1 = major divergence).
    pub impact_score: f64,
    /// Whether the impact is likely beneficial (heuristic).
    pub likely_beneficial: Option<bool>,
    /// Confidence in the result (based on self-model error).
    pub confidence: f64,
}

// ═══════════════════════════════════════════════════════════════
// RECURSIVE LOOP — closed-loop feedback control
// ═══════════════════════════════════════════════════════════════

/// Classified outcome of a meta-intervention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterventionOutcome {
    /// Fitness improved measurably after the intervention.
    Improved,
    /// Fitness degraded after the intervention.
    Worsened,
    /// No significant change detected.
    NoEffect,
    /// Mixed signals (e.g. energy improved but pain increased).
    Mixed,
    /// Too early to tell / still pending measurement.
    Pending,
    /// Intervention was blocked by guardrails.
    Blocked,
}

impl InterventionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Improved => "improved",
            Self::Worsened => "worsened",
            Self::NoEffect => "no_effect",
            Self::Mixed => "mixed",
            Self::Pending => "pending",
            Self::Blocked => "blocked",
        }
    }
}

/// Snapshot of brain vitals at a point in time, used for before/after comparison.
#[derive(Debug, Clone)]
pub struct VitalSnapshot {
    pub tick: u64,
    pub fitness: f64,
    pub pain: f64,
    pub energy_avg: f64,
    pub arousal: f64,
    pub firing_rate: f64,
    pub entropy: f64,
    pub novelty: f64,
}

impl VitalSnapshot {
    fn from_meta(mc: &MetaCortex, fitness: f64, entropy: f64) -> Self {
        Self {
            tick: 0,
            fitness,
            pain: mc.last_state.pain_composite,
            energy_avg: mc.last_state.energy_avg,
            arousal: mc.last_state.arousal,
            firing_rate: mc.last_state.firing_rate,
            entropy,
            novelty: mc.last_state.novelty,
        }
    }

    /// Composite health score from vitals (0-1, higher = healthier).
    fn health_score(&self) -> f64 {
        let fitness_w = 0.30;
        let pain_w = 0.25;
        let energy_w = 0.20;
        let firing_w = 0.10;
        let entropy_w = 0.05;
        let novelty_w = 0.10;
        (fitness_w * self.fitness
            + pain_w * (1.0 - self.pain)
            + energy_w * self.energy_avg
            + firing_w * self.firing_rate.min(0.3) / 0.3
            + entropy_w * self.entropy.min(0.5) / 0.5
            + novelty_w * self.novelty
        ).clamp(0.0, 1.0)
    }
}

/// Full record of a single intervention in the recursive loop.
#[derive(Debug, Clone)]
pub struct InterventionRecord {
    /// Unique intervention id.
    pub id: u64,
    /// Tick when the action was applied.
    pub applied_tick: u64,
    /// The action that was taken.
    pub action: MetaAction,
    /// Vital snapshot just before the action.
    pub state_before: VitalSnapshot,
    /// Short-term vitals (SHORT_TERM_DELAY ticks later). None until measured.
    pub state_after_short: Option<VitalSnapshot>,
    /// Long-term vitals (LONG_TERM_DELAY ticks later). None until measured.
    pub state_after_long: Option<VitalSnapshot>,
    /// Predicted effect from self-model (divergence at horizon).
    pub predicted_divergence: f64,
    /// Classified outcome (updated as measurements come in).
    pub outcome: InterventionOutcome,
    /// How many ticks have elapsed since the action.
    pub elapsed_ticks: u64,
    /// Confidence in the outcome classification.
    pub outcome_confidence: f64,
}

/// A pending intervention waiting for outcome measurement.
#[derive(Debug, Clone)]
struct PendingIntervention {
    record: InterventionRecord,
    /// Snapshot of state before the action (for comparison).
    state_before_health: f64,
    /// When to take short-term measurement.
    measure_at_short: u64,
    /// When to take long-term measurement.
    measure_at_long: u64,
}

/// Phase of the closed-loop recursive cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedLoopPhase {
    /// Normal observation, no action needed.
    Observing,
    /// Analyzing a detected issue (loop, stuck, low fitness).
    Analyzing,
    /// Executing a meta-action.
    Acting,
    /// Waiting for outcome to materialize before acting again.
    WaitingForOutcome,
    /// Evaluating past intervention outcomes to learn.
    Learning,
    /// Loop is paused (too many failures, conservative mode).
    Paused,
}

impl ClosedLoopPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observing => "observing",
            Self::Analyzing => "analyzing",
            Self::Acting => "acting",
            Self::WaitingForOutcome => "waiting_for_outcome",
            Self::Learning => "learning",
            Self::Paused => "paused",
        }
    }
}

/// Statistics for the recursive loop operation.
#[derive(Debug, Clone)]
pub struct RecursiveLoopStats {
    /// Current phase of the loop.
    pub phase: ClosedLoopPhase,
    /// Total interventions attempted.
    pub total_interventions: u64,
    /// Interventions that resulted in Improved outcome.
    pub successful_interventions: u64,
    /// Interventions that worsened the state.
    pub harmful_interventions: u64,
    /// Rolling success rate (0-1).
    pub success_rate: f64,
    /// Current recursive depth (0 = base level).
    pub current_depth: u32,
    /// Max depth ever reached.
    pub max_depth_reached: u32,
    /// Count of depth limit rejections.
    pub depth_rejections: u64,
    /// Current cooldown (adaptive, ticks).
    pub effective_cooldown: u64,
    /// Current max actions per window (adaptive).
    pub effective_max_actions: usize,
    /// Conservative mode flag (triggered by consecutive failures).
    pub conservative_mode: bool,
    /// Consecutive failures counter.
    pub consecutive_failures: u32,
    /// Number of completed feedback cycles.
    pub completed_cycles: u64,
    /// Permanent freeze: set when harm ratio exceeds threshold with enough samples.
    pub frozen: bool,
    pub frozen_at_tick: u64,
    pub freeze_reason: String,
}

impl Default for RecursiveLoopStats {
    fn default() -> Self {
        Self {
            phase: ClosedLoopPhase::Observing,
            total_interventions: 0,
            successful_interventions: 0,
            harmful_interventions: 0,
            success_rate: 0.5,
            current_depth: 0,
            max_depth_reached: 0,
            depth_rejections: 0,
            effective_cooldown: DEFAULT_COOLDOWN,
            effective_max_actions: MAX_ACTIONS_PER_WINDOW,
            conservative_mode: false,
            consecutive_failures: 0,
            completed_cycles: 0,
            frozen: false,
            frozen_at_tick: 0,
            freeze_reason: String::new(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// META TYPES (existing, preserved)
// ═══════════════════════════════════════════════════════════════

/// Target cortex for reconfiguration actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CortexTarget {
    Ssm,
    DeepSeek,
    Meta,
}

/// Action proposed or applied by the meta-cortex.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaAction {
    // ── Existing ──
    None,
    IncreaseExploration,
    ResetCortexState,
    ModifyLearningRate(f64),
    InjectNoise,
    SimplifyArchitecture,
    // ── Correction 1: Extended actions ──
    /// Ajuster un paramètre global du brain
    AdjustParam { param_name: String, delta: f64, reason: String },
    /// Modifier la configuration d'un cortex
    ReconfigureCortex { cortex: CortexTarget, param: String, value: f64 },
    /// Activer/désactiver un module
    ToggleModule { module_name: String, enabled: bool },
    /// Spawner un enfant
    SpawnChild { reason: String },
    /// Augmenter/diminuer le nombre de neurones d'un module
    ResizeModule { module_name: String, delta_neurons: i32 },
    /// Réinitialiser un module stagnant
    ResetModule { module_name: String },
    /// Injecter un script Rhai
    InjectScript { script_name: String, script_body: String },
    /// Déclencher une auto-modification du génome
    MutateGenome { target: String, strategy: String },
    /// Ajuster les poids de l'attention hybride
    RebalanceAttention { csa_weight: f64, hca_weight: f64 },
    /// Aucune action (observer seulement)
    NoOp,
}

impl MetaAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetaAction::None => "none",
            MetaAction::IncreaseExploration => "increase_exploration",
            MetaAction::ResetCortexState => "reset_cortex",
            MetaAction::ModifyLearningRate(_) => "modify_lr",
            MetaAction::InjectNoise => "inject_noise",
            MetaAction::SimplifyArchitecture => "simplify",
            MetaAction::AdjustParam { .. } => "adjust_param",
            MetaAction::ReconfigureCortex { .. } => "reconfigure_cortex",
            MetaAction::ToggleModule { .. } => "toggle_module",
            MetaAction::SpawnChild { .. } => "spawn_child",
            MetaAction::ResizeModule { .. } => "resize_module",
            MetaAction::ResetModule { .. } => "reset_module",
            MetaAction::InjectScript { .. } => "inject_script",
            MetaAction::MutateGenome { .. } => "mutate_genome",
            MetaAction::RebalanceAttention { .. } => "rebalance_attention",
            MetaAction::NoOp => "noop",
        }
    }
}

/// A single observation of the cortex state.
#[derive(Debug, Clone)]
struct CortexObservation {
    tick: u64,
    prediction_error: f64,
    modulation_entropy: f64,
    pattern_hash: u64,
    fitness: f64,
}

/// Detected loop in output patterns.
#[derive(Debug, Clone)]
pub struct LoopInfo {
    pub pattern_hash: u64,
    pub first_seen_tick: u64,
    pub repeat_count: usize,
    pub duration_ticks: u64,
}

/// Analysis result from meta-cortex observation.
#[derive(Debug, Clone)]
pub struct MetaAnalysis {
    pub is_stuck: bool,
    pub stuck_duration: u64,
    pub suggested_action: MetaAction,
    pub confidence: f64,
    pub active_loops: Vec<LoopInfo>,
    // ── Correction 1: Extended analysis fields ──
    pub stagnation_ticks: u64,
    pub recommended_action: Option<MetaAction>,
    pub action_confidence: f64,
    pub last_fitness: f64,
    pub last_fitness_tick: u64,
}

/// Guardrails to prevent harmful self-modification cascades.
struct MetaGuard {
    cooldown_ticks: u64,
    last_action_tick: Option<u64>,
    actions_this_window: usize,
    window_start_tick: u64,
    blacklisted_patterns: Vec<u64>,
    max_actions_per_window: usize,
}

impl MetaGuard {
    fn new() -> Self {
        Self::new_with_config(DEFAULT_COOLDOWN, MAX_ACTIONS_PER_WINDOW)
    }

    fn new_with_config(cooldown_ticks: u64, max_actions_per_window: usize) -> Self {
        Self {
            cooldown_ticks,
            last_action_tick: None,
            actions_this_window: 0,
            window_start_tick: 0,
            blacklisted_patterns: Vec::new(),
            max_actions_per_window,
        }
    }

    fn can_act(&self, current_tick: u64) -> bool {
        if let Some(last) = self.last_action_tick {
            if current_tick < last.saturating_add(self.cooldown_ticks) {
                return false;
            }
        }
        if current_tick - self.window_start_tick > 500 {
            return true;
        }
        self.actions_this_window < self.max_actions_per_window
    }

    fn record_action(&mut self, tick: u64) {
        if tick - self.window_start_tick > 500 {
            self.window_start_tick = tick;
            self.actions_this_window = 0;
        }
        self.actions_this_window += 1;
        self.last_action_tick = Some(tick);
    }

    fn is_blacklisted(&self, hash: u64) -> bool {
        self.blacklisted_patterns.contains(&hash)
    }

    fn blacklist(&mut self, hash: u64) {
        if !self.blacklisted_patterns.contains(&hash) {
            self.blacklisted_patterns.push(hash);
            if self.blacklisted_patterns.len() > 64 {
                self.blacklisted_patterns.remove(0);
            }
        }
    }
}

/// Detects loops in output patterns.
struct LoopDetector {
    pattern_history: VecDeque<(u64, u64)>, // (hash, tick)
}

impl LoopDetector {
    fn new() -> Self {
        Self::with_capacity(MAX_OBSERVATIONS)
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            pattern_history: VecDeque::with_capacity(cap),
        }
    }

    fn feed(&mut self, hash: u64, tick: u64, max_obs: usize) -> Vec<LoopInfo> {
        self.pattern_history.push_back((hash, tick));
        if self.pattern_history.len() > max_obs {
            self.pattern_history.pop_front();
        }

        let mut loops = Vec::new();
        let recent: Vec<_> = self.pattern_history.iter().rev().take(20).copied().collect();

        if recent.len() < LOOP_DETECTION_THRESHOLD {
            return loops;
        }

        // ── Contiguous repetition detection (existing) ──
        let target = recent[0].0;
        let mut count = 0;
        let mut first_tick = recent[0].1;
        for &(h, t) in &recent {
            if h == target {
                count += 1;
                first_tick = t;
            } else {
                break;
            }
        }

        if count >= LOOP_DETECTION_THRESHOLD {
            loops.push(LoopInfo {
                pattern_hash: target,
                first_seen_tick: first_tick,
                repeat_count: count,
                duration_ticks: tick.saturating_sub(first_tick),
            });
        }

        // ── Floyd non-contiguous cycle detection (tortoise/hare) ──
        if let Some(nc) = self.detect_floyd_cycle(tick) {
            // Dedup: only push if not already detected as contiguous
            if !loops.iter().any(|l| l.pattern_hash == nc.pattern_hash) {
                loops.push(nc);
            }
        }

        loops
    }

    /// Floyd's tortoise/hare cycle detection on pattern_history.
    /// Detects A→B→A→B→… cycles invisible to contiguous detection.
    fn detect_floyd_cycle(&self, current_tick: u64) -> Option<LoopInfo> {
        let n = self.pattern_history.len();
        if n < 8 {
            return None; // need enough history
        }

        // ── Phase 1: find meeting point ──
        let mut t: usize = 0;
        let mut h: usize = 0;
        let mut found = false;
        for _ in 0..n {
            if t + 1 >= n || h + 2 >= n {
                break;
            }
            t += 1;
            h += 2;
            if self.pattern_history[t].0 == self.pattern_history[h].0 {
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }

        // ── Phase 2: find cycle start ──
        let mut t2: usize = 0;
        let mut h2 = h;
        while t2 < n && h2 < n && self.pattern_history[t2].0 != self.pattern_history[h2].0 {
            t2 += 1;
            h2 += 1;
        }
        let cycle_start = t2;

        // ── Phase 3: measure cycle length ──
        let cycle_hash = self.pattern_history[cycle_start].0;
        let mut length: usize = 1;
        let mut cursor = cycle_start + 1;
        while cursor < n && self.pattern_history[cursor].0 != cycle_hash {
            length += 1;
            cursor += 1;
        }

        // Must cycle at least twice to be meaningful
        let repeat_count = (n - cycle_start).saturating_div(length.max(1));
        if repeat_count < 2 || length < 2 {
            return None;
        }

        // Verify the cycle repeats (not a random collision)
        let mut verified = true;
        for i in 0..(repeat_count.saturating_sub(1)) {
            for j in 0..length {
                let idx1 = cycle_start + j;
                let idx2 = cycle_start + (i + 1) * length + j;
                if idx2 >= n { break; }
                if self.pattern_history[idx1].0 != self.pattern_history[idx2].0 {
                    verified = false;
                    break;
                }
            }
            if !verified { break; }
        }

        if !verified {
            return None;
        }

        let first_tick = self.pattern_history[cycle_start].1;
        Some(LoopInfo {
            pattern_hash: cycle_hash,
            first_seen_tick: first_tick,
            repeat_count,
            duration_ticks: current_tick.saturating_sub(first_tick),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// METACORTEX — the full recursive self-observer with metacognition
// ═══════════════════════════════════════════════════════════════

/// The MetaCortex — recursive self-observer with self-modeling,
/// prediction, and counterfactual reasoning capabilities.
pub struct MetaCortex {
    // ── Existing: loop detection + guardrails ──
    observations: VecDeque<CortexObservation>,
    loop_detector: LoopDetector,
    guard: MetaGuard,
    pub last_analysis: MetaAnalysis,
    /// Accumulated meta-correction score (for evolution feedback)
    pub correction_score: f64,
    /// Total meta-interventions applied
    pub intervention_count: u64,

    // ── NEW: Self-modeling ──
    /// Learned dynamics model (predicts next state from current)
    self_model: SelfModel,
    /// Last observed brain state summary
    pub last_state: BrainStateSummary,
    /// Predicted next state (from last observation)
    pub predicted_next: BrainStateSummary,
    /// Last self-model prediction error
    pub last_self_model_error: f64,
    /// Ticks accumulated between self-model learning updates
    ticks_since_learn: u64,
    /// Self-model prediction error history (for trend analysis)
    error_history: VecDeque<f64>,
    /// State snapshots for /api/replay: (prev_state, current_state) pairs
    pub state_snapshots: VecDeque<(Vec<f64>, Vec<f64>)>,
    /// Whether the self-model is sufficiently trained for counterfactuals
    pub model_ready: bool,

    // ── NEW: Counterfactual history ──
    /// Recent counterfactual results
    pub recent_counterfactuals: VecDeque<CounterfactualResult>,

    // ── NEW: Recursive closed-loop control ──
    /// All intervention records (full history, ring buffer).
    pub intervention_history: VecDeque<InterventionRecord>,
    /// Pending interventions waiting for outcome measurement.
    pending_interventions: VecDeque<PendingIntervention>,
    /// Recursive loop statistics.
    pub loop_stats: RecursiveLoopStats,
    /// Next intervention id.
    next_intervention_id: u64,
    /// Cumulative health score tracking (for before/after comparison).
    last_health_snapshot: Option<VitalSnapshot>,
    /// Tick of the last health snapshot.
    last_health_tick: u64,
    /// Outcome history for success rate computation (ring buffer of bools).
    outcome_window: VecDeque<bool>,
    /// Tracks whether we're currently inside the cortical observation context.
    in_cortex_context: bool,
    /// Last computed novelty score from brain activation history.
    pub last_novelty: f64,

    // ── Runtime config (loaded from soullink.toml) ──
    pub config: crate::config::MetaConfig,
}

impl MetaCortex {
    pub fn new() -> Self {
        Self::new_with_config(crate::config::MetaConfig::default())
    }

    pub fn new_with_config(config: crate::config::MetaConfig) -> Self {
        Self {
            observations: VecDeque::with_capacity(config.max_observations),
            loop_detector: LoopDetector::with_capacity(config.max_observations),
            guard: MetaGuard::new_with_config(config.default_cooldown, config.max_actions_per_window),
            last_analysis: MetaAnalysis {
                is_stuck: false,
                stuck_duration: 0,
                suggested_action: MetaAction::None,
                confidence: 0.0,
                active_loops: Vec::new(),
                stagnation_ticks: 0,
                recommended_action: None,
                action_confidence: 0.0,
                last_fitness: 0.0,
                last_fitness_tick: 0,
            },
            correction_score: 0.0,
            intervention_count: 0,

            self_model: SelfModel::new(config.self_model_default_lr),
            last_state: BrainStateSummary::default(),
            predicted_next: BrainStateSummary::default(),
            last_self_model_error: 0.0,
            ticks_since_learn: 0,
            error_history: VecDeque::with_capacity(64),
            model_ready: false,

            recent_counterfactuals: VecDeque::with_capacity(16),

            intervention_history: VecDeque::with_capacity(config.max_observations),
            pending_interventions: VecDeque::with_capacity(config.max_pending_interventions),
            loop_stats: RecursiveLoopStats::default(),
            next_intervention_id: 0,
            last_health_snapshot: None,
            last_health_tick: 0,
            outcome_window: VecDeque::with_capacity(config.success_rate_window),
            in_cortex_context: false,
            last_novelty: 0.0,
            state_snapshots: VecDeque::with_capacity(config.max_observations),
            config,
        }
    }

    // ─────────────────────────────────────────────────────────
    // EXISTING: Cortex observation (loop detection)
    // ─────────────────────────────────────────────────────────

    /// Observe the cortex state this tick. Returns analysis results.
    pub fn observe(
        &mut self,
        tick: u64,
        prediction_error: f64,
        modulation_signals: &[f64],
        fitness: f64,
    ) -> &MetaAnalysis {
        let entropy = normalized_entropy(modulation_signals);
        let hash = if modulation_signals.is_empty() {
            0
        } else {
            pattern_hash(modulation_signals)
        };

        let obs = CortexObservation {
            tick,
            prediction_error,
            modulation_entropy: entropy,
            pattern_hash: hash,
            fitness,
        };

        self.observations.push_back(obs);
        if self.observations.len() > self.config.max_observations {
            self.observations.pop_front();
        }

        let loops = self.loop_detector.feed(hash, tick, self.config.max_observations);

        let is_stuck = !loops.is_empty()
            || (entropy < 0.1 && self.observations.len() > 10)
            || (prediction_error < 1e-6 && fitness < 0.5);

        let stuck_duration = if is_stuck {
            loops.first().map(|l| l.duration_ticks).unwrap_or(0)
        } else {
            0
        };

        let (action, confidence) = self.select_action(is_stuck, entropy, prediction_error, fitness, &loops);

        // Track stagnation
        let (stagnation_ticks, last_fitness, last_fitness_tick) = {
            let prev_fitness = self.last_analysis.last_fitness;
            let _prev_tick = self.last_analysis.last_fitness_tick;
            let stg = if (fitness - prev_fitness).abs() < 0.001 {
                self.last_analysis.stagnation_ticks + 1
            } else {
                0
            };
            (stg, fitness, tick)
        };

        self.last_analysis = MetaAnalysis {
            is_stuck,
            stuck_duration,
            suggested_action: action,
            confidence,
            active_loops: loops,
            stagnation_ticks,
            recommended_action: None,
            action_confidence: 0.0,
            last_fitness,
            last_fitness_tick,
        };

        &self.last_analysis
    }

    // ─────────────────────────────────────────────────────────
    // NEW: Brain state observation (feeds self-model)
    // ─────────────────────────────────────────────────────────

    /// Feed the self-model with the current brain state.
    /// The self-model learns to predict the *next* state from the *current* one.
    ///
    /// Call this once per brain step. The model:
    /// 1. Uses the previously predicted next state as a "guess" target
    ///    for the previous observation (offline-like learning).
    /// 2. Computes a forward prediction for the current state.
    /// 3. Stores current state for next learning update.
    pub fn observe_brain_state(
        &mut self,
        module_activations: &[f64],
        energy_avg: f64,
        arousal: f64,
        metabolic_pressure: f64,
        pain_composite: f64,
        firing_rate: f64,
    ) {
        let mut summary = BrainStateSummary::default();
        let n_mod = module_activations.len().min(11);
        summary.module_activations[..n_mod].copy_from_slice(&module_activations[..n_mod]);
        summary.energy_avg = energy_avg.clamp(0.0, 1.0);
        summary.arousal = arousal.clamp(0.0, 1.0);
        summary.metabolic_pressure = metabolic_pressure.clamp(0.0, 1.0);
        summary.pain_composite = pain_composite.clamp(0.0, 1.0);
        summary.firing_rate = firing_rate;

        let current_vec = summary.to_vec();

        // Learn: previous_state → current_state (delayed target)
        let prev_vec = self.last_state.to_vec();
        self.ticks_since_learn += 1;

        if self.ticks_since_learn >= self.config.self_model_learn_interval {
            let error = self.self_model.train_step(&prev_vec, &current_vec);
            self.last_self_model_error = error;
            self.error_history.push_back(error);
            if self.error_history.len() > 64 {
                self.error_history.pop_front();
            }
            self.self_model.decay_lr();
            self.ticks_since_learn = 0;

            // Store (prev, current) pair for /api/replay
            self.state_snapshots.push_back((prev_vec.clone(), current_vec.clone()));
            if self.state_snapshots.len() > 256 {
                self.state_snapshots.pop_front();
            }

            // Model is "ready" after enough training with low error
            if self.self_model.train_steps > 50 && self.self_model.avg_error < 0.1 {
                self.model_ready = true;
            }
        }

        // Predict next state (from current)
        let predicted_vec = self.self_model.forward(&current_vec);
        self.predicted_next = BrainStateSummary::from_vec(&predicted_vec);

        summary.novelty = self.last_novelty;

        // Store for next learning step
        self.last_state = summary;
    }

    /// Set the current novelty score (called from Brain::step).
    pub fn set_novelty(&mut self, novelty: f64) {
        self.last_novelty = novelty.clamp(0.0, 1.0);
    }

    /// Public accessor for /api/replay — runs self-model forward pass.
    pub fn self_model_forward(&self, input: &[f64]) -> Vec<f64> {
        self.self_model.forward(input)
    }

    // ─────────────────────────────────────────────────────────
    // Self-prediction — multi-step rollout
    // ─────────────────────────────────────────────────────────

    /// Predict future brain states N steps ahead using the learned self-model.
    ///
    /// Returns a vector of predicted states, where `result[0]` is 1 step ahead
    /// and `result[n-1]` is n steps ahead.
    ///
    /// If the horizon is very long (>20), predictions will degrade in accuracy
    /// due to compounding errors. The last value includes a `confidence` estimate.
    pub fn predict_future(&self, n_steps: usize) -> Vec<BrainStateSummary> {
        let steps = n_steps.min(MAX_PREDICTION_HORIZON);
        if steps == 0 {
            return vec![];
        }

        let mut trajectory = Vec::with_capacity(steps);
        let mut current = self.last_state.to_vec();

        for _ in 0..steps {
            let predicted = self.self_model.forward(&current);
            trajectory.push(BrainStateSummary::from_vec(&predicted));
            current = predicted;
        }

        trajectory
    }

    /// Predict future and return a divergence score: how much the predicted
    /// state at horizon `steps` differs from the current state.
    /// High divergence signals instability or ongoing change.
    pub fn predict_divergence(&self, steps: usize) -> f64 {
        let future = self.predict_future(steps);
        if future.is_empty() {
            return 0.0;
        }
        self.last_state.distance(&future[future.len() - 1])
    }

    /// Confidence estimate for predictions at a given horizon.
    /// Based on self-model error compounded over steps.
    pub fn prediction_confidence(&self, horizon: usize) -> f64 {
        let base_error = self.self_model.avg_error;
        // Error compounds: 1 - (1 - (1-error)^horizon)
        let compounding = 1.0 - (1.0 - base_error.min(0.99)).powi(horizon as i32);
        (1.0 - compounding).max(0.0)
    }

    // ─────────────────────────────────────────────────────────
    // NEW: Counterfactual reasoning
    // ─────────────────────────────────────────────────────────

    /// Run a counterfactual simulation: "What if we changed X?"
    ///
    /// 1. Takes the current brain state
    /// 2. Applies the hypothetical modifications to create a virtual starting state
    /// 3. Runs the self-model forward for `horizon` steps (baseline + counterfactual)
    /// 4. Compares trajectories to compute impact and divergence
    ///
    /// Returns None if the self-model isn't trained enough.
    pub fn reason_counterfactual(
        &self,
        scenario: CounterfactualScenario,
        horizon: usize,
    ) -> Option<CounterfactualResult> {
        if !self.model_ready {
            return None;
        }

        let steps = horizon.min(MAX_PREDICTION_HORIZON);
        if steps == 0 {
            return None;
        }

        // ── Build the counterfactual starting state ──
        let mut cf_start = self.last_state.to_vec();

        // Apply module tweaks (affect module_activations indices 0-10)
        for (module_name, delta) in &scenario.module_tweaks {
            // Map module names to indices (hardcoded from brain.rs init_modules order)
            let idx = match module_name.as_str() {
                "perception" => 0, "memory" => 1, "reasoning" => 2,
                "learning" => 3, "output" => 4, "attention" => 5,
                "language" => 6, "vision" => 7, "audio" => 8,
                "motor" => 9, "extra" => 10,
                _ => continue,
            };
            if idx < 11 {
                cf_start[idx] = (cf_start[idx] + delta).clamp(0.0, 1.0);
            }
        }

        // Apply state tweaks (affect global fields at indices 11-15)
        for (field, delta) in &scenario.state_tweaks {
            let idx = match field.as_str() {
                "energy_avg" => 11, "arousal" => 12,
                "metabolic_pressure" => 13, "pain_composite" => 14,
                "firing_rate" => 15,
                _ => continue,
            };
            if idx < SELF_MODEL_INPUT_DIM {
                cf_start[idx] = (cf_start[idx] + delta).clamp(0.0, 1.0);
            }
        }

        // Apply param changes — these indirectly affect the dynamics.
        // Since the self-model doesn't take params as explicit input,
        // we approximate the effect by biasing the relevant state dimensions.
        // This is a heuristic: param changes → estimated state deltas.
        for (param_name, _new_value) in &scenario.param_changes {
            // Heuristic: tau affects energy/pressure, stdp affects firing
            match param_name.as_str() {
                "tau" => {
                    cf_start[11] = (cf_start[11] + 0.05).min(1.0); // energy +5%
                }
                "stdp_learning_rate" => {
                    cf_start[15] = (cf_start[15] + 0.1).min(1.0); // firing +10%
                }
                "homeostatic_rate" => {
                    cf_start[12] = (cf_start[12] + 0.05).min(1.0); // arousal +5%
                }
                "prune_threshold" => {
                    cf_start[14] = (cf_start[14] - 0.05).max(0.0); // pain -5%
                }
                "metabolism_enabled" => {
                    cf_start[11] = (cf_start[11] + 0.1).min(1.0);
                }
                _ => {}
            }
        }

        // ── Baseline trajectory (no changes) ──
        let mut baseline = Vec::with_capacity(steps);
        let mut current_base = self.last_state.to_vec();
        for _ in 0..steps {
            let pred = self.self_model.forward(&current_base);
            baseline.push(BrainStateSummary::from_vec(&pred));
            current_base = pred;
        }

        // ── Counterfactual trajectory ──
        let mut trajectory = Vec::with_capacity(steps);
        let mut divergence = Vec::with_capacity(steps);
        let mut current_cf = cf_start;

        for step in 0..steps {
            let pred = self.self_model.forward(&current_cf);
            let state = BrainStateSummary::from_vec(&pred);
            let div = state.distance(&baseline[step]);
            divergence.push(div);
            trajectory.push(state);
            current_cf = pred;
        }

        // ── Impact score ──
        let max_div = divergence.iter().cloned().fold(0.0_f64, f64::max);
        let avg_div: f64 = divergence.iter().sum::<f64>() / steps.max(1) as f64;
        let impact_score = (max_div * 0.7 + avg_div * 0.3).min(1.0);

        // ── Heuristic beneficial-ness ──
        // Beneficial if: energy_avg increases AND pain decreases over time
        let likely_beneficial = if steps >= 3 {
            let final_cf = &trajectory[steps - 1];
            let final_bl = &baseline[steps - 1];
            let energy_improves = final_cf.energy_avg > final_bl.energy_avg;
            let pain_reduces = final_cf.pain_composite < final_bl.pain_composite;
            if energy_improves && pain_reduces {
                Some(true)
            } else if !energy_improves && !pain_reduces {
                Some(false)
            } else {
                None // mixed
            }
        } else {
            None
        };

        // ── Confidence: self-model accuracy compounded ──
        let confidence = self.prediction_confidence(steps);

        let result = CounterfactualResult {
            scenario,
            trajectory,
            baseline,
            divergence,
            impact_score,
            likely_beneficial,
            confidence,
        };

        Some(result)
    }

    /// Run a counterfactual and store it in history. Returns the result.
    pub fn run_counterfactual(
        &mut self,
        description: &str,
        param_changes: Vec<(String, f64)>,
        module_tweaks: Vec<(String, f64)>,
        state_tweaks: Vec<(String, f64)>,
        horizon: usize,
    ) -> Option<CounterfactualResult> {
        let scenario = CounterfactualScenario {
            description: description.to_string(),
            param_changes,
            module_tweaks,
            state_tweaks,
        };

        let result = self.reason_counterfactual(scenario, horizon)?;

        self.recent_counterfactuals.push_back(result.clone());
        if self.recent_counterfactuals.len() > 16 {
            self.recent_counterfactuals.pop_front();
        }

        Some(result)
    }

    // ─────────────────────────────────────────────────────────
    // EXISTING: Action selection + execution
    // ─────────────────────────────────────────────────────────

    fn select_action(
        &self,
        is_stuck: bool,
        entropy: f64,
        prediction_error: f64,
        fitness: f64,
        loops: &[LoopInfo],
    ) -> (MetaAction, f64) {
        if !is_stuck {
            return (MetaAction::None, 0.0);
        }

        if !self.guard.can_act(self.observations.back().map(|o| o.tick).unwrap_or(0)) {
            return (MetaAction::None, 0.0);
        }

        for li in loops {
            if self.guard.is_blacklisted(li.pattern_hash) {
                return (MetaAction::None, 0.0);
            }
        }

        if entropy < 0.05 && !loops.is_empty() {
            return (MetaAction::InjectNoise, 0.9);
        }

        if !loops.is_empty() && loops.iter().any(|l| l.duration_ticks > 200) {
            return (MetaAction::ResetCortexState, 0.8);
        }

        if prediction_error > 0.5 && fitness < 0.3 {
            return (MetaAction::SimplifyArchitecture, 0.7);
        }

        if is_stuck && fitness < 0.4 {
            return (MetaAction::IncreaseExploration, 0.6);
        }

        (MetaAction::None, 0.0)
    }

    /// Execute a meta-action with guardrails. Returns the action taken.
    pub fn execute_action(&mut self, current_tick: u64) -> MetaAction {
        if self.last_analysis.suggested_action == MetaAction::None {
            return MetaAction::None;
        }
        if !self.guard.can_act(current_tick) {
            self.loop_stats.phase = ClosedLoopPhase::WaitingForOutcome;
            return MetaAction::None;
        }

        let action = self.last_analysis.suggested_action.clone();
        self.guard.record_action(current_tick);

        if let Some(li) = self.last_analysis.active_loops.first() {
            self.guard.blacklist(li.pattern_hash);
        }

        self.intervention_count += 1;
        self.correction_score = (self.correction_score + self.last_analysis.confidence).min(100.0);

        action
    }

    // ─────────────────────────────────────────────────────────
    // CORRECTION 1: Decide and apply autonomous actions
    // ─────────────────────────────────────────────────────────

    /// Analyse l'état actuel et décide d'une action à appliquer.
    /// Retourne None si aucune intervention n'est jugée nécessaire.
    pub fn decide_action(&self, brain: &crate::Brain) -> Option<MetaAction> {
        use crate::brain_module::StressMode;

        // 1. PANIC: stress_mode == Panic depuis > 30 ticks → reset module mourant
        if brain.stress_mode == StressMode::Panic && brain.death_clock > 30 {
            let dead_module = brain
                .modules
                .iter()
                .find(|(_, m)| {
                    let active = m.neuron_indices.iter().filter(|&&idx| {
                        brain.neurons[idx].activation > 0.01
                    }).count();
                    active == 0
                })
                .map(|(name, _)| name.clone());

            if let Some(dead) = dead_module {
                return Some(MetaAction::ResetModule {
                    module_name: dead,
                });
            }
        }

        // 2. STAGNATION: fitness n'a pas bougé depuis N ticks
        if self.last_analysis.stagnation_ticks > 500 {
            if brain.energy_pool > brain.energy_pool_capacity * 0.5 {
                return Some(MetaAction::SpawnChild {
                    reason: "stagnation_escape".into(),
                });
            } else {
                return Some(MetaAction::MutateGenome {
                    target: "brain_params".into(),
                    strategy: "random_walk".into(),
                });
            }
        }

        // 3. OVERFIT: self_model_error < 0.01 depuis longtemps → rebalance attention
        if self.error_history.iter().rev().take(20).all(|&e| e < 0.01)
            && self.error_history.len() >= 20
        {
            return Some(MetaAction::RebalanceAttention {
                csa_weight: 0.6,
                hca_weight: 0.4,
            });
        }

        // 4. UNDERFIT: self_model_error > threshold → ajuster learning rate
        if self.last_self_model_error > 0.5 {
            return Some(MetaAction::AdjustParam {
                param_name: "global_learning_rate".into(),
                delta: 0.05,
                reason: "high_prediction_error".into(),
            });
        }

        // 5. RESOURCE: energy_pool < 20% → désactiver module non critique
        if brain.energy_pool < brain.energy_pool_capacity * 0.2 {
            return Some(MetaAction::ToggleModule {
                module_name: "extra".into(),
                enabled: false,
            });
        }

        None
    }

    /// Applique une action décidée par le meta-cortex sur le brain.
    /// Retourne un InterventionRecord décrivant le résultat.
    pub fn apply_action(
        &mut self,
        brain: &mut crate::Brain,
        action: MetaAction,
    ) -> InterventionRecord {
        self.intervention_count += 1;
        let tick = brain.stats.age_ticks;

        match &action {
            MetaAction::AdjustParam { param_name, delta, reason: _ } => {
                brain.evolution.genome.brain_params.apply_delta(param_name, *delta);
                InterventionRecord {
                    id: self.next_intervention_id(),
                    applied_tick: tick,
                    action,
                    state_before: self.vital_snapshot(brain),
                    state_after_short: None,
                    state_after_long: None,
                    predicted_divergence: 0.0,
                    outcome: InterventionOutcome::Pending,
                    elapsed_ticks: 0,
                    outcome_confidence: 0.0,
                }
            }
            MetaAction::ReconfigureCortex { cortex, param, value } => {
                match cortex {
                    CortexTarget::Ssm => {
                        if param == "enabled" {
                            brain.cortex.config.enabled = *value > 0.5;
                        }
                        if param == "tick_interval" {
                            brain.cortex.config.tick_interval = *value as u64;
                        }
                    }
                    CortexTarget::DeepSeek => {
                        if param == "enabled" {
                            brain.deepseek_cortex.config.enabled = *value > 0.5;
                        }
                        if param == "tick_interval" {
                            brain.deepseek_cortex.config.tick_interval = *value as u64;
                        }
                    }
                    CortexTarget::Meta => { /* ajuster ses propres paramètres */ }
                }
                InterventionRecord {
                    id: self.next_intervention_id(),
                    applied_tick: tick,
                    action,
                    state_before: self.vital_snapshot(brain),
                    state_after_short: None,
                    state_after_long: None,
                    predicted_divergence: 0.0,
                    outcome: InterventionOutcome::Pending,
                    elapsed_ticks: 0,
                    outcome_confidence: 0.0,
                }
            }
            MetaAction::ToggleModule { module_name, enabled } => {
                if let Some(module) = brain.modules.get_mut(module_name) {
                    // BrainModule doesn't have an "enabled" field — use energy_budget as proxy
                    module.energy_budget = if *enabled { 1.0 } else { 0.0 };
                }
                InterventionRecord {
                    id: self.next_intervention_id(),
                    applied_tick: tick,
                    action,
                    state_before: self.vital_snapshot(brain),
                    state_after_short: None,
                    state_after_long: None,
                    predicted_divergence: 0.0,
                    outcome: InterventionOutcome::Pending,
                    elapsed_ticks: 0,
                    outcome_confidence: 0.0,
                }
            }
            MetaAction::ResetModule { module_name } => {
                if let Some(module) = brain.modules.get_mut(module_name) {
                    for &idx in &module.neuron_indices {
                        brain.neurons[idx].potential = 0.3;
                        brain.neurons[idx].activation = 0.0;
                    }
                }
                InterventionRecord {
                    id: self.next_intervention_id(),
                    applied_tick: tick,
                    action,
                    state_before: self.vital_snapshot(brain),
                    state_after_short: None,
                    state_after_long: None,
                    predicted_divergence: 0.0,
                    outcome: InterventionOutcome::Pending,
                    elapsed_ticks: 0,
                    outcome_confidence: 0.0,
                }
            }
            MetaAction::RebalanceAttention { .. } | MetaAction::MutateGenome { .. } => {
                tracing::info!(
                    "🧠 MetaCortex: action {} applied (async — result pending)",
                    action.as_str()
                );
                InterventionRecord {
                    id: self.next_intervention_id(),
                    applied_tick: tick,
                    action,
                    state_before: self.vital_snapshot(brain),
                    state_after_short: None,
                    state_after_long: None,
                    predicted_divergence: 0.0,
                    outcome: InterventionOutcome::Pending,
                    elapsed_ticks: 0,
                    outcome_confidence: 0.0,
                }
            }
            MetaAction::SpawnChild { reason: _ } => {
                let genome = crate::evolution::Genome::from_brain(brain);
                match crate::reproduction::spawn_child(
                    &genome,
                    &brain.node_name,
                    brain.node_port,
                    &mut rand::rng(),
                ) {
                    Ok(_info) => InterventionRecord {
                        id: self.next_intervention_id(),
                        applied_tick: tick,
                        action,
                        state_before: self.vital_snapshot(brain),
                        state_after_short: None,
                        state_after_long: None,
                        predicted_divergence: 0.0,
                        outcome: InterventionOutcome::Pending,
                        elapsed_ticks: 0,
                        outcome_confidence: 0.0,
                    },
                    Err(_e) => InterventionRecord {
                        id: self.next_intervention_id(),
                        applied_tick: tick,
                        action,
                        state_before: self.vital_snapshot(brain),
                        state_after_short: None,
                        state_after_long: None,
                        predicted_divergence: 0.0,
                        outcome: InterventionOutcome::Blocked,
                        elapsed_ticks: 0,
                        outcome_confidence: 1.0,
                    },
                }
            }
            _ => InterventionRecord {
                id: self.next_intervention_id(),
                applied_tick: tick,
                action: MetaAction::NoOp,
                state_before: self.vital_snapshot(brain),
                state_after_short: None,
                state_after_long: None,
                predicted_divergence: 0.0,
                outcome: InterventionOutcome::NoEffect,
                elapsed_ticks: 0,
                outcome_confidence: 0.0,
            },
        }
    }

    fn next_intervention_id(&mut self) -> u64 {
        let id = self.next_intervention_id;
        self.next_intervention_id = self.next_intervention_id.wrapping_add(1);
        id
    }

    fn vital_snapshot(&self, brain: &crate::Brain) -> VitalSnapshot {
        VitalSnapshot {
            tick: brain.stats.age_ticks,
            fitness: brain.evolution.best_fitness,
            pain: brain.pain.composite as f64,
            energy_avg: brain.stats.energy_avg as f64,
            arousal: brain.arousal as f64,
            firing_rate: if brain.stats.total_neurons > 0 {
                brain.stats.firing_neurons as f64 / brain.stats.total_neurons as f64
            } else {
                0.0
            },
            entropy: 0.5,
            novelty: brain.compute_novelty_score(),
        }
    }

    // ─────────────────────────────────────────────────────────
    // RECURSIVE LOOP: closed-loop feedback control
    // ─────────────────────────────────────────────────────────

    /// Main orchestrator: advances the recursive feedback loop by one tick.
    ///
    /// Call this once per simulation tick. It:
    /// 1. Measures outcomes of any pending interventions
    /// 2. Adapts guardrail parameters based on recent success rate
    /// 3. Updates the loop phase
    /// 4. Records health snapshots for before/after comparison
    pub fn step_recursive_loop(&mut self, current_tick: u64, fitness: f64, entropy: f64) {
        // ── Phase 1: Measure pending outcomes ──
        self.measure_outcomes(current_tick);

        // ── Phase 2: Adapt guardrails ──
        self.adapt_guardrails();

        // ── Phase 3: Update loop phase ──
        self.update_loop_phase(current_tick);

        // ── Phase 4: Autonomous execution — when Analyzing, apply the action ──
        if self.loop_stats.phase == ClosedLoopPhase::Analyzing
            && self.last_analysis.confidence > 0.3
            && self.is_safe_to_act()
        {
            if self.last_novelty > 0.3 && self.last_analysis.suggested_action == MetaAction::None {
                self.last_analysis.suggested_action = MetaAction::InjectNoise;
                self.last_analysis.confidence = self.last_analysis.confidence.max(0.5);
            }
            let action = self.execute_action(current_tick);
            self.loop_stats.total_interventions += 1;
            self.loop_stats.phase = match action {
                MetaAction::None => ClosedLoopPhase::Observing,
                _ => ClosedLoopPhase::WaitingForOutcome,
            };
        }

        // ── Phase 5: Record periodic health snapshot ──
        if current_tick - self.last_health_tick >= SHORT_TERM_DELAY {
            self.last_health_snapshot = Some(VitalSnapshot::from_meta(self, fitness, entropy));
            self.last_health_tick = current_tick;
        }
    }

    /// Measure outcomes for all pending interventions whose delay has elapsed.
    fn measure_outcomes(&mut self, current_tick: u64) {
        let mut completed_indices = Vec::new();

        for (idx, pending) in self.pending_interventions.iter_mut().enumerate() {
            let mut updated = false;

            // Short-term measurement
            if pending.record.state_after_short.is_none() && current_tick >= pending.measure_at_short {
                pending.record.state_after_short = self.last_health_snapshot.clone();
                pending.record.elapsed_ticks = current_tick - pending.record.applied_tick;
                updated = true;
            }

            // Long-term measurement
            if pending.record.state_after_long.is_none() && current_tick >= pending.measure_at_long {
                pending.record.state_after_long = self.last_health_snapshot.clone();
                pending.record.elapsed_ticks = current_tick - pending.record.applied_tick;
                updated = true;
            }

            // Classify outcome once we have at least short-term data
            if updated && pending.record.state_after_short.is_some() {
                pending.record.outcome = classify_intervention_outcome(&pending.record);
            }

            // Mark complete when both measurements are done
            if pending.record.state_after_short.is_some() && pending.record.state_after_long.is_some() {
                completed_indices.push(idx);
            }
        }

        // Move completed interventions to history and update stats
        for &idx in completed_indices.iter().rev() {
            if let Some(completed) = self.pending_interventions.remove(idx) {
                let mut record = completed.record;

                // Final outcome classification
                record.outcome = classify_intervention_outcome(&record);
                record.outcome_confidence = outcome_confidence_for(&record);

                // Update stats
                self.loop_stats.total_interventions += 1;
                self.loop_stats.completed_cycles += 1;

                match record.outcome {
                    InterventionOutcome::Improved => {
                        self.loop_stats.successful_interventions += 1;
                        self.loop_stats.consecutive_failures = 0;
                        self.outcome_window.push_back(true);
                    }
                    InterventionOutcome::Worsened => {
                        self.loop_stats.harmful_interventions += 1;
                        self.loop_stats.consecutive_failures += 1;
                        self.outcome_window.push_back(false);
                    }
                    InterventionOutcome::NoEffect | InterventionOutcome::Mixed => {
                        // Neutral — counts as non-failure for conservative mode
                        self.loop_stats.consecutive_failures = self.loop_stats.consecutive_failures.saturating_sub(1);
                        self.outcome_window.push_back(true); // not harmful
                    }
                    _ => {}
                }

                // Trim outcome window
                if self.outcome_window.len() > SUCCESS_RATE_WINDOW {
                    self.outcome_window.pop_front();
                }

                // Update rolling success rate
                let successes = self.outcome_window.iter().filter(|&&b| b).count();
                let total = self.outcome_window.len().max(1);
                self.loop_stats.success_rate = successes as f64 / total as f64;

                // Push to history
                self.intervention_history.push_back(record);
                if self.intervention_history.len() > MAX_OBSERVATIONS {
                    self.intervention_history.pop_front();
                }
            }
        }
    }

    /// Drain completed intervention records (action + outcome) since last call.
    /// The caller (simulation loop) feeds these into the evolution engine
    /// to bridge meta-cortex learning into mutation_stats weights.
    pub fn drain_completed_interventions(&mut self) -> Vec<(MetaAction, InterventionOutcome)> {
        let mut out = Vec::new();
        while let Some(record) = self.intervention_history.pop_front() {
            if record.outcome != InterventionOutcome::Pending {
                out.push((record.action, record.outcome));
            }
        }
        out
    }
}

/// Standalone: classify the outcome of an intervention by comparing before/after vitals.
fn classify_intervention_outcome(record: &InterventionRecord) -> InterventionOutcome {
        let before = &record.state_before;
        let after = match &record.state_after_long {
            Some(a) => a,
            None => match &record.state_after_short {
                Some(a) => a,
                None => return InterventionOutcome::Pending,
            },
        };

        let health_before = before.health_score();
        let health_after = after.health_score();
        let delta = health_after - health_before;

        // Multi-signal analysis
        let fitness_delta = after.fitness - before.fitness;
        let pain_delta = after.pain - before.pain; // negative = less pain = good
        let energy_delta = after.energy_avg - before.energy_avg;

        let fitness_improved = fitness_delta > MIN_FITNESS_IMPROVEMENT;
        let pain_reduced = pain_delta < -0.01;
        let energy_improved = energy_delta > 0.02;
        let health_improved = delta > 0.03;

        let positive_signals = [fitness_improved, pain_reduced, energy_improved, health_improved]
            .iter().filter(|&&b| b).count();
        let negative_signals = [
            fitness_delta < -MIN_FITNESS_IMPROVEMENT,
            pain_delta > 0.01,
            energy_delta < -0.02,
            delta < -0.03,
        ].iter().filter(|&&b| b).count();

        if positive_signals >= 3 {
            InterventionOutcome::Improved
        } else if negative_signals >= 3 {
            InterventionOutcome::Worsened
        } else if positive_signals >= 1 && negative_signals >= 1 {
            InterventionOutcome::Mixed
        } else {
            InterventionOutcome::NoEffect
        }
    }

/// Standalone: estimate confidence in an outcome classification.
fn outcome_confidence_for(record: &InterventionRecord) -> f64 {
        // Confidence is higher when:
        // 1. Both short and long measurements agree
        // 2. The health delta is large (clear signal)
        let has_long = record.state_after_long.is_some();
        let has_short = record.state_after_short.is_some();
        if !has_short {
            return 0.0;
        }
        let base = if has_long { 0.8 } else { 0.4 };
        let delta = record.state_before.health_score()
            - record.state_after_short.as_ref().map(|s| s.health_score()).unwrap_or(0.0);
        let clarity = (delta.abs() * 5.0).min(0.2);
        (base + clarity).min(1.0)
}

impl MetaCortex {
    /// Adapt guardrail parameters (cooldown, action limits) based on success rate.
    fn adapt_guardrails(&mut self) {
        let sr = self.loop_stats.success_rate;

        // Adaptive cooldown: shorter when successful, longer when failing
        if sr > 0.7 {
            self.loop_stats.effective_cooldown = (DEFAULT_COOLDOWN as f64 * 0.5) as u64;
            self.loop_stats.effective_max_actions = MAX_ACTIONS_PER_WINDOW + 3;
        } else if sr > 0.5 {
            self.loop_stats.effective_cooldown = (DEFAULT_COOLDOWN as f64 * 0.75) as u64;
            self.loop_stats.effective_max_actions = MAX_ACTIONS_PER_WINDOW + 1;
        } else if sr > 0.3 {
            self.loop_stats.effective_cooldown = DEFAULT_COOLDOWN;
            self.loop_stats.effective_max_actions = MAX_ACTIONS_PER_WINDOW;
        } else {
            self.loop_stats.effective_cooldown = DEFAULT_COOLDOWN * 2;
            self.loop_stats.effective_max_actions = (MAX_ACTIONS_PER_WINDOW as f64 * 0.4) as usize;
        }

        // Conservative mode: triggered by 3+ consecutive failures
        let was_conservative = self.loop_stats.conservative_mode;
        self.loop_stats.conservative_mode = self.loop_stats.consecutive_failures >= 3;

        if self.loop_stats.conservative_mode && !was_conservative {
            // Reset recursive depth when entering conservative mode
            while self.loop_stats.current_depth > 0 {
                self.loop_stats.current_depth -= 1;
            }
        }

        if self.loop_stats.conservative_mode {
            self.loop_stats.effective_cooldown = DEFAULT_COOLDOWN * 3;
            self.loop_stats.effective_max_actions = 1;
        }

        // Exit conservative mode when success rate improves
        if was_conservative && !self.loop_stats.conservative_mode {
            self.loop_stats.effective_cooldown = DEFAULT_COOLDOWN;
            self.loop_stats.effective_max_actions = MAX_ACTIONS_PER_WINDOW;
            self.loop_stats.phase = ClosedLoopPhase::Observing;
        }

        // ── Circuit breaker: permanent freeze ──────────────────────
        let total = self.loop_stats.total_interventions;
        let harmful = self.loop_stats.harmful_interventions;
        let harm_ratio = if total > 0 { harmful as f64 / total as f64 } else { 0.0 };
        if total >= INTERVENTION_FREEZE_THRESHOLD
            && harm_ratio >= HARMFUL_RATIO_FREEZE_THRESHOLD
            && !self.loop_stats.frozen
        {
            self.loop_stats.frozen = true;
            self.loop_stats.frozen_at_tick = 0; // caller sets actual tick
            self.loop_stats.freeze_reason = format!(
                "harm_ratio={:.3} ({}/{}) exceeded threshold {:.3}",
                harm_ratio, harmful, total, HARMFUL_RATIO_FREEZE_THRESHOLD,
            );
        }
    }

    /// Update the closed-loop phase based on current state.
    fn update_loop_phase(&mut self, _current_tick: u64) {
        if self.loop_stats.frozen {
            self.loop_stats.phase = ClosedLoopPhase::Paused;
            return;
        }

        if self.loop_stats.conservative_mode {
            self.loop_stats.phase = ClosedLoopPhase::Paused;
            return;
        }

        if !self.pending_interventions.is_empty() {
            self.loop_stats.phase = ClosedLoopPhase::WaitingForOutcome;
            return;
        }

        if self.last_analysis.is_stuck && self.last_analysis.confidence > 0.3 {
            if self.is_safe_to_act() {
                self.loop_stats.phase = ClosedLoopPhase::Analyzing;
            } else {
                self.loop_stats.phase = ClosedLoopPhase::WaitingForOutcome;
            }
            return;
        }

        // Periodic learning phase
        if self.loop_stats.completed_cycles > 0 && self.loop_stats.completed_cycles % 5 == 0 {
            self.loop_stats.phase = ClosedLoopPhase::Learning;
        } else {
            self.loop_stats.phase = ClosedLoopPhase::Observing;
        }
    }

    /// Comprehensive safety check combining all guardrail layers.
    pub fn is_safe_to_act(&self) -> bool {
        if self.loop_stats.frozen {
            return false;
        }
        if self.loop_stats.conservative_mode {
            return false;
        }
        if self.loop_stats.current_depth >= MAX_RECURSIVE_DEPTH {
            return false;
        }
        if self.loop_stats.phase == ClosedLoopPhase::Paused {
            return false;
        }
        if self.pending_interventions.len() >= MAX_PENDING_INTERVENTIONS {
            return false;
        }
        if !self.model_ready && self.self_model.train_steps < 100 {
            return false; // need basic training before acting
        }
        true
    }

    /// Enter a deeper recursion level. Returns false if max depth reached.
    pub fn enter_recursive_depth(&mut self) -> bool {
        if self.loop_stats.current_depth >= MAX_RECURSIVE_DEPTH {
            self.loop_stats.depth_rejections += 1;
            return false;
        }
        self.loop_stats.current_depth += 1;
        if self.loop_stats.current_depth > self.loop_stats.max_depth_reached {
            self.loop_stats.max_depth_reached = self.loop_stats.current_depth;
        }
        true
    }

    /// Exit the current recursion level.
    pub fn exit_recursive_depth(&mut self) {
        self.loop_stats.current_depth = self.loop_stats.current_depth.saturating_sub(1);
    }

    /// Record a new intervention. Call this right after executing an action.
    /// Returns the intervention id, or None if the queue is full.
    pub fn record_intervention(
        &mut self,
        current_tick: u64,
        action: MetaAction,
        fitness: f64,
        entropy: f64,
    ) -> Option<u64> {
        if action == MetaAction::None {
            return None;
        }
        if self.pending_interventions.len() >= MAX_PENDING_INTERVENTIONS {
            return None;
        }

        let id = self.next_intervention_id;
        self.next_intervention_id += 1;

        let state_before = VitalSnapshot::from_meta(self, fitness, entropy);

        let record = InterventionRecord {
            id,
            applied_tick: current_tick,
            action,
            state_before: state_before.clone(),
            state_after_short: None,
            state_after_long: None,
            predicted_divergence: self.predict_divergence(10),
            outcome: InterventionOutcome::Pending,
            elapsed_ticks: 0,
            outcome_confidence: 0.0,
        };

        let health_before = state_before.health_score();

        self.pending_interventions.push_back(PendingIntervention {
            record,
            state_before_health: health_before,
            measure_at_short: current_tick + SHORT_TERM_DELAY,
            measure_at_long: current_tick + LONG_TERM_DELAY,
        });

        self.loop_stats.phase = ClosedLoopPhase::WaitingForOutcome;
        Some(id)
    }

    /// Check whether we should trigger a learning consolidation.
    pub fn should_consolidate_learning(&self) -> bool {
        self.loop_stats.completed_cycles > 0
            && self.loop_stats.completed_cycles % 10 == 0
            && self.model_ready
    }

    /// Get a summary of the recursive loop state for logging.
    pub fn loop_summary(&self) -> String {
        format!(
            "phase={} depth={}/{} interventions={} success={:.1}% completed={} mode={}",
            self.loop_stats.phase.as_str(),
            self.loop_stats.current_depth,
            MAX_RECURSIVE_DEPTH,
            self.loop_stats.total_interventions,
            self.loop_stats.success_rate * 100.0,
            self.loop_stats.completed_cycles,
            if self.loop_stats.conservative_mode { "conservative" } else { "normal" },
        )
    }

    /// Check if the self-model error is trending down (learning) or up (forgetting).
    pub fn error_trend(&self) -> f64 {
        if self.error_history.len() < 10 {
            return 0.0; // not enough data
        }
        let recent: Vec<f64> = self.error_history.iter().rev().take(10).copied().collect();
        let older: Vec<f64> = self.error_history.iter().rev().skip(10).take(10).copied().collect();
        if older.is_empty() {
            return 0.0;
        }
        let recent_avg = recent.iter().sum::<f64>() / recent.len().max(1) as f64;
        let older_avg = older.iter().sum::<f64>() / older.len().max(1) as f64;
        (older_avg - recent_avg) / older_avg.max(1e-6)
    }

    // ─────────────────────────────────────────────────────────
    // API Reporting
    // ─────────────────────────────────────────────────────────

    /// Full status for API reporting, including metacognition details.
    pub fn api_status(&self) -> serde_json::Value {
        serde_json::json!({
            "observations": self.observations.len(),
            "is_stuck": self.last_analysis.is_stuck,
            "stuck_duration": self.last_analysis.stuck_duration,
            "suggested_action": self.last_analysis.suggested_action.as_str(),
            "action_confidence": self.last_analysis.confidence,
            "active_loops": self.last_analysis.active_loops.len(),
            "intervention_count": self.intervention_count,
            "correction_score": (self.correction_score * 1000.0).round() / 1000.0,
            // ── Metacognition ──
            "metacognition": {
                "self_model_error": (self.last_self_model_error * 1000.0).round() / 1000.0,
                "self_model_avg_error": (self.self_model.avg_error * 1000.0).round() / 1000.0,
                "self_model_train_steps": self.self_model.train_steps,
                "model_ready": self.model_ready,
                "error_trend": (self.error_trend() * 1000.0).round() / 1000.0,
                "prediction_5step": self.prediction_confidence(5),
                "prediction_20step": self.prediction_confidence(20),
                "current_state": {
                    "energy_avg": (self.last_state.energy_avg * 1000.0).round() / 1000.0,
                    "arousal": (self.last_state.arousal * 1000.0).round() / 1000.0,
                    "metabolic_pressure": (self.last_state.metabolic_pressure * 1000.0).round() / 1000.0,
                    "pain_composite": (self.last_state.pain_composite * 1000.0).round() / 1000.0,
                    "firing_rate": (self.last_state.firing_rate * 1000.0).round() / 1000.0,
                },
                "predicted_next": {
                    "energy_avg": (self.predicted_next.energy_avg * 1000.0).round() / 1000.0,
                    "arousal": (self.predicted_next.arousal * 1000.0).round() / 1000.0,
                    "pain_composite": (self.predicted_next.pain_composite * 1000.0).round() / 1000.0,
                },
                "counterfactuals_ran": self.recent_counterfactuals.len(),
            },
            // ── Recursive Loop ──
            "recursive_loop": {
                "phase": self.loop_stats.phase.as_str(),
                "depth": self.loop_stats.current_depth,
                "max_depth": self.loop_stats.max_depth_reached,
                "depth_rejections": self.loop_stats.depth_rejections,
                "total_interventions": self.loop_stats.total_interventions,
                "successful": self.loop_stats.successful_interventions,
                "harmful": self.loop_stats.harmful_interventions,
                "success_rate": (self.loop_stats.success_rate * 1000.0).round() / 1000.0,
                "completed_cycles": self.loop_stats.completed_cycles,
                "effective_cooldown": self.loop_stats.effective_cooldown,
                "effective_max_actions": self.loop_stats.effective_max_actions,
                "conservative_mode": self.loop_stats.conservative_mode,
                "consecutive_failures": self.loop_stats.consecutive_failures,
                "pending_interventions": self.pending_interventions.len(),
                "intervention_history_len": self.intervention_history.len(),
            }
        })
    }

    /// Export prediction trajectory as JSON for API.
    pub fn predict_future_json(&self, steps: usize) -> serde_json::Value {
        let trajectory = self.predict_future(steps);
        let states: Vec<serde_json::Value> = trajectory.iter().map(|s| {
            serde_json::json!({
                "energy_avg": (s.energy_avg * 1000.0).round() / 1000.0,
                "arousal": (s.arousal * 1000.0).round() / 1000.0,
                "pain_composite": (s.pain_composite * 1000.0).round() / 1000.0,
                "metabolic_pressure": (s.metabolic_pressure * 1000.0).round() / 1000.0,
                "firing_rate": (s.firing_rate * 1000.0).round() / 1000.0,
            })
        }).collect();

        serde_json::json!({
            "horizon": steps,
            "confidence": (self.prediction_confidence(steps) * 1000.0).round() / 1000.0,
            "divergence_at_horizon": (self.predict_divergence(steps) * 1000.0).round() / 1000.0,
            "trajectory": states,
        })
    }

    /// Export counterfactual result as JSON for API.
    pub fn counterfactual_result_json(&self, result: &CounterfactualResult) -> serde_json::Value {
        serde_json::json!({
            "description": result.scenario.description,
            "impact_score": (result.impact_score * 1000.0).round() / 1000.0,
            "confidence": (result.confidence * 1000.0).round() / 1000.0,
            "likely_beneficial": result.likely_beneficial,
            "max_divergence": (result.divergence.iter().cloned().fold(0.0_f64, f64::max) * 1000.0).round() / 1000.0,
            "final_divergence": result.divergence.last().map(|d| (d * 1000.0).round() / 1000.0).unwrap_or(0.0),
            "final_cf_state": {
                "energy_avg": (result.trajectory.last().map(|s| s.energy_avg).unwrap_or(0.0) * 1000.0).round() / 1000.0,
                "arousal": (result.trajectory.last().map(|s| s.arousal).unwrap_or(0.0) * 1000.0).round() / 1000.0,
                "pain_composite": (result.trajectory.last().map(|s| s.pain_composite).unwrap_or(0.0) * 1000.0).round() / 1000.0,
            },
        })
    }

    /// Reset the self-model (forget learned dynamics).
    pub fn reset_self_model(&mut self) {
        self.self_model = SelfModel::new(SELF_MODEL_DEFAULT_LR);
        self.error_history.clear();
        self.last_self_model_error = 0.0;
        self.ticks_since_learn = 0;
        self.model_ready = false;
    }

    /// Force the model to be considered ready (for testing).
    #[cfg(test)]
    pub(crate) fn force_model_ready(&mut self) {
        self.model_ready = true;
    }

    // ─────────────────────────────────────────────────────────
    // CORRECTION 3: Persistence
    // ─────────────────────────────────────────────────────────

    /// Save meta-cortex state to a JSON file.
    pub fn save_state(&self, path: &str) -> Result<(), String> {
        let data = serde_json::json!({
            "observation_count": self.observations.len(),
            "correction_score": self.correction_score,
            "intervention_count": self.intervention_count,
            "last_self_model_error": self.last_self_model_error,
            "model_ready": self.model_ready,
            "train_steps": self.self_model.train_steps,
            "loop_stats": {
                "phase": self.loop_stats.phase.as_str(),
                "total_interventions": self.loop_stats.total_interventions,
                "successful_interventions": self.loop_stats.successful_interventions,
                "harmful_interventions": self.loop_stats.harmful_interventions,
                "success_rate": self.loop_stats.success_rate,
                "completed_cycles": self.loop_stats.completed_cycles,
                "frozen": self.loop_stats.frozen,
            },
        });
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("Meta serialize: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Meta save: {e}"))
    }

    /// Load meta-cortex state from a JSON file. Restores stats into a fresh MetaCortex.
    pub fn load_state(path: &str) -> Result<Self, String> {
        let json_str =
            std::fs::read_to_string(path).map_err(|e| format!("Meta read: {e}"))?;
        let data: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("Meta parse: {e}"))?;

        let mut mc = Self::new();
        if let Some(v) = data.get("correction_score").and_then(|v| v.as_f64()) {
            mc.correction_score = v;
        }
        if let Some(v) = data.get("intervention_count").and_then(|v| v.as_u64()) {
            mc.intervention_count = v;
        }
        if let Some(v) = data.get("last_self_model_error").and_then(|v| v.as_f64()) {
            mc.last_self_model_error = v;
        }
        if let Some(v) = data.get("model_ready").and_then(|v| v.as_bool()) {
            mc.model_ready = v;
        }
        Ok(mc)
    }
}

impl Default for MetaCortex {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MetaCortex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetaCortex")
            .field("observations", &self.observations.len())
            .field("is_stuck", &self.last_analysis.is_stuck)
            .field("interventions", &self.intervention_count)
            .field("self_model_error", &self.last_self_model_error)
            .field("model_ready", &self.model_ready)
            .finish()
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing tests ──
    #[test]
    fn test_pattern_hash_deterministic() {
        let a = pattern_hash(&[1.0, 2.0, 3.0]);
        let b = pattern_hash(&[1.0, 2.0, 3.0]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_pattern_hash_different() {
        let a = pattern_hash(&[1.0, 2.0, 3.0]);
        let b = pattern_hash(&[3.0, 2.0, 1.0]);
        assert_ne!(a, b);
    }

    #[test]
    fn test_entropy_uniform() {
        let e = normalized_entropy(&[1.0, 1.0, 1.0, 1.0]);
        assert!(e > 0.9, "Uniform distribution should have high entropy, got {e}");
    }

    #[test]
    fn test_entropy_spike() {
        let e = normalized_entropy(&[1.0, 0.0, 0.0, 0.0]);
        assert!(e < 0.3, "Spike should have low entropy, got {e}");
    }

    // ── Self-model tests ──
    #[test]
    fn test_brain_state_summary_to_from_vec_roundtrip() {
        let mut s = BrainStateSummary::default();
        s.module_activations = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.0, 0.0];
        s.energy_avg = 0.75;
        s.arousal = 0.6;
        s.metabolic_pressure = 0.1;
        s.pain_composite = 0.2;
        s.firing_rate = 0.1;

        let vec = s.to_vec();
        let s2 = BrainStateSummary::from_vec(&vec);

        assert!((s.energy_avg - s2.energy_avg).abs() < 1e-6);
        assert!((s.arousal - s2.arousal).abs() < 1e-6);
        assert!((s.pain_composite - s2.pain_composite).abs() < 1e-6);
    }

    #[test]
    fn test_self_model_forward_returns_correct_dim() {
        let model = SelfModel::new(0.01);
        let input = vec![0.5; SELF_MODEL_INPUT_DIM];
        let output = model.forward(&input);
        assert_eq!(output.len(), SELF_MODEL_INPUT_DIM);
    }

    #[test]
    fn test_self_model_train_reduces_error() {
        let mut model = SelfModel::new(0.05);
        // Train on identity mapping (next = current)
        let input: Vec<f64> = (0..SELF_MODEL_INPUT_DIM).map(|i| i as f64 / SELF_MODEL_INPUT_DIM as f64).collect();

        let initial_output = model.forward(&input);
        let initial_mse: f64 = initial_output.iter().zip(input.iter())
            .map(|(p, t)| (p - t).powi(2))
            .sum::<f64>() / SELF_MODEL_INPUT_DIM as f64;

        // Train for several steps
        for _ in 0..200 {
            model.train_step(&input, &input);
        }

        let final_output = model.forward(&input);
        let final_mse: f64 = final_output.iter().zip(input.iter())
            .map(|(p, t)| (p - t).powi(2))
            .sum::<f64>() / SELF_MODEL_INPUT_DIM as f64;

        assert!(final_mse < initial_mse * 0.8,
            "Training should reduce MSE: initial={:.4} final={:.4}", initial_mse, final_mse);
    }

    #[test]
    fn test_self_model_decay_lr() {
        let mut model = SelfModel::new(0.01);
        let orig_lr = model.lr;
        for _ in 0..100 {
            model.decay_lr();
        }
        assert!(model.lr < orig_lr, "LR should decay over time ({} < {})", model.lr, orig_lr);
        assert!(model.lr >= 0.0001, "LR should not go below min (got {})", model.lr);
    }

    // ── MetaCortex metacognition tests ──
    #[test]
    fn test_meta_cortex_new_is_not_stuck() {
        let mc = MetaCortex::new();
        assert!(!mc.last_analysis.is_stuck);
    }

    #[test]
    fn test_meta_cortex_initial_model_not_ready() {
        let mc = MetaCortex::new();
        assert!(!mc.model_ready);
    }

    #[test]
    fn test_observe_brain_state_updates_state() {
        let mut mc = MetaCortex::new();
        let mod_acts: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.0, 0.1];

        mc.observe_brain_state(&mod_acts, 0.75, 0.6, 0.1, 0.2, 0.05);

        assert!((mc.last_state.energy_avg - 0.75).abs() < 0.01);
        assert!((mc.last_state.arousal - 0.6).abs() < 0.01);
        assert!((mc.last_state.pain_composite - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_predict_future_returns_correct_count() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);

        let trajectory = mc.predict_future(5);
        assert_eq!(trajectory.len(), 5);

        // Also check clamping
        let trajectory = mc.predict_future(60);
        assert_eq!(trajectory.len(), MAX_PREDICTION_HORIZON);
    }

    #[test]
    fn test_predict_future_zero_steps() {
        let mc = MetaCortex::new();
        let trajectory = mc.predict_future(0);
        assert!(trajectory.is_empty());
    }

    #[test]
    fn test_counterfactual_not_ready_returns_none() {
        let mc = MetaCortex::new();
        let scenario = CounterfactualScenario {
            description: "test".to_string(),
            param_changes: vec![("tau".to_string(), 0.99)],
            module_tweaks: vec![],
            state_tweaks: vec![],
        };
        let result = mc.reason_counterfactual(scenario, 10);
        assert!(result.is_none(), "Should return None when model not ready");
    }

    #[test]
    fn test_counterfactual_returns_result_when_ready() {
        let mut mc = MetaCortex::new();

        // Feed enough observations to train the model
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..60 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.force_model_ready();

        let scenario = CounterfactualScenario {
            description: "test tau increase".to_string(),
            param_changes: vec![("tau".to_string(), 0.99)],
            module_tweaks: vec![],
            state_tweaks: vec![],
        };
        let result = mc.reason_counterfactual(scenario, 5);
        assert!(result.is_some(), "Should return a result when model is ready");
        let r = result.unwrap();
        assert_eq!(r.trajectory.len(), 5);
        assert_eq!(r.baseline.len(), 5);
        assert_eq!(r.divergence.len(), 5);
        assert!(r.impact_score >= 0.0 && r.impact_score <= 1.0);
    }

    #[test]
    fn test_counterfactual_module_tweak() {
        let mut mc = MetaCortex::new();

        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..60 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.force_model_ready();

        let scenario = CounterfactualScenario {
            description: "boost perception".to_string(),
            param_changes: vec![],
            module_tweaks: vec![("perception".to_string(), 0.5)],
            state_tweaks: vec![],
        };
        let result = mc.reason_counterfactual(scenario, 5);
        assert!(result.is_some());
    }

    #[test]
    fn test_predict_divergence() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..30 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        let div = mc.predict_divergence(5);
        // Should be a valid number
        assert!(div >= 0.0 && div <= 2.0, "Divergence should be reasonable, got {}", div);
    }

    #[test]
    fn test_prediction_confidence_decays_with_horizon() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..30 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        let c1 = mc.prediction_confidence(1);
        let c5 = mc.prediction_confidence(5);
        let c20 = mc.prediction_confidence(20);
        // Longer horizon should have lower or equal confidence
        assert!(c1 >= c5, "Short horizon confidence ({}) should be >= medium ({})", c1, c5);
        assert!(c5 >= c20, "Medium horizon confidence ({}) should be >= long ({})", c5, c20);
    }

    #[test]
    fn test_error_trend_initial_zero() {
        let mc = MetaCortex::new();
        assert!((mc.error_trend() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_reset_self_model() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..30 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        assert!(mc.self_model.train_steps > 0);

        mc.reset_self_model();
        assert_eq!(mc.self_model.train_steps, 0);
        assert!(!mc.model_ready);
        assert!(mc.error_history.is_empty());
    }

    #[test]
    fn test_run_counterfactual_stores_in_history() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..60 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.force_model_ready();

        let result = mc.run_counterfactual(
            "test scenario",
            vec![("tau".to_string(), 0.99)],
            vec![],
            vec![],
            5,
        );
        assert!(result.is_some());
        assert_eq!(mc.recent_counterfactuals.len(), 1);
    }

    #[test]
    fn test_detect_loop() {
        let mut mc = MetaCortex::new();
        let signals = vec![0.5; 16];
        for t in 0..20 {
            mc.observe(t, 0.01, &signals, 0.3);
        }
        assert!(mc.last_analysis.is_stuck, "Should detect loop after 20 identical observations");
    }

    #[test]
    fn test_normal_state_not_stuck() {
        let mut mc = MetaCortex::new();
        use rand::Rng;
        let mut rng = rand::rng();
        for t in 0..20 {
            let signals: Vec<f64> = (0..16).map(|_| rng.random::<f64>()).collect();
            mc.observe(t, 0.1 + rng.random::<f64>() * 0.1, &signals, 0.6);
        }
        assert!(!mc.last_analysis.is_stuck, "Random signals should not trigger stuck detection");
    }

    #[test]
    fn test_guard_cooldown() {
        let mut mc = MetaCortex::new();
        let signals = vec![0.5; 16];
        for t in 0..25 {
            mc.observe(t, 0.01, &signals, 0.3);
        }
        let action = mc.execute_action(25);
        assert_ne!(action, MetaAction::None);

        let action2 = mc.execute_action(26);
        assert_eq!(action2, MetaAction::None);
    }

    #[test]
    fn test_entropy_zero_for_empty() {
        let e = normalized_entropy(&[]);
        assert_eq!(e, 0.0);
    }

    #[test]
    fn test_state_summary_distance() {
        let mut a = BrainStateSummary::default();
        a.energy_avg = 1.0;
        let mut b = BrainStateSummary::default();
        b.energy_avg = 0.0;
        let dist = a.distance(&b);
        assert!(dist > 0.5, "Distance should be large between opposite states, got {}", dist);
    }

    #[test]
    fn test_state_summary_distance_identical_is_zero() {
        let a = BrainStateSummary::default();
        let b = BrainStateSummary::default();
        let dist = a.distance(&b);
        assert!(dist < 1e-10, "Identical states should have zero distance");
    }

    #[test]
    fn test_api_status_returns_metacognition_block() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);

        let status = mc.api_status();
        let mc_block = status.get("metacognition").unwrap();
        assert!(mc_block.get("self_model_error").unwrap().as_f64().is_some());
        assert!(mc_block.get("model_ready").unwrap().as_bool() == Some(false));
        assert!(mc_block.get("current_state").is_some());
        assert!(mc_block.get("predicted_next").is_some());
        // Recursive loop block should be present
        assert!(status.get("recursive_loop").is_some());
        let rl = status.get("recursive_loop").unwrap();
        assert!(rl.get("phase").is_some());
        assert!(rl.get("depth").is_some());
        assert!(rl.get("conservative_mode").unwrap().as_bool() == Some(false));
    }

    // ── Recursive loop tests ──

    #[test]
    fn test_step_recursive_loop_updates_phase() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..5 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.step_recursive_loop(10, 0.6, 0.2);
        assert_eq!(mc.loop_stats.phase, ClosedLoopPhase::Observing);
    }

    #[test]
    fn test_record_intervention_assigns_id() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        let id = mc.record_intervention(10, MetaAction::InjectNoise, 0.5, 0.2);
        assert!(id.is_some());
        assert_eq!(id.unwrap(), 0);
        assert_eq!(mc.pending_interventions.len(), 1);
        assert_eq!(mc.loop_stats.phase, ClosedLoopPhase::WaitingForOutcome);
    }

    #[test]
    fn test_record_intervention_none_action_returns_none() {
        let mut mc = MetaCortex::new();
        let id = mc.record_intervention(0, MetaAction::None, 0.5, 0.2);
        assert!(id.is_none());
    }

    #[test]
    fn test_record_intervention_queue_full_returns_none() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        for _ in 0..MAX_PENDING_INTERVENTIONS {
            let id = mc.record_intervention(10, MetaAction::InjectNoise, 0.5, 0.2);
            assert!(id.is_some());
        }
        let id = mc.record_intervention(10, MetaAction::InjectNoise, 0.5, 0.2);
        assert!(id.is_none());
    }

    #[test]
    fn test_measure_outcomes_completes_intervention() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..5 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.record_intervention(0, MetaAction::InjectNoise, 0.5, 0.2).unwrap();
        for t in 1..60 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
            mc.step_recursive_loop(t, 0.55, 0.2);
        }
        assert_eq!(mc.pending_interventions.len(), 0);
        assert_eq!(mc.loop_stats.total_interventions, 1);
        assert_eq!(mc.loop_stats.completed_cycles, 1);
        assert_eq!(mc.intervention_history.len(), 1);
    }

    #[test]
    fn test_is_safe_to_act_initial_state() {
        let mc = MetaCortex::new();
        assert!(!mc.is_safe_to_act());
    }

    #[test]
    fn test_is_safe_to_act_with_trained_model() {
        let mut mc = MetaCortex::new();
        let acts: Vec<f64> = vec![0.1; 11];
        for _ in 0..120 {
            mc.observe_brain_state(&acts, 0.5, 0.5, 0.1, 0.1, 0.05);
        }
        mc.force_model_ready();
        assert!(mc.is_safe_to_act());
    }

    #[test]
    fn test_is_safe_to_act_conservative_mode_blocks() {
        let mut mc = MetaCortex::new();
        mc.force_model_ready();
        mc.loop_stats.conservative_mode = true;
        assert!(!mc.is_safe_to_act());
    }

    #[test]
    fn test_is_safe_to_act_max_depth_blocks() {
        let mut mc = MetaCortex::new();
        mc.force_model_ready();
        mc.loop_stats.current_depth = MAX_RECURSIVE_DEPTH;
        assert!(!mc.is_safe_to_act());
    }

    #[test]
    fn test_enter_recursive_depth() {
        let mut mc = MetaCortex::new();
        assert!(mc.enter_recursive_depth());
        assert_eq!(mc.loop_stats.current_depth, 1);
        assert!(mc.enter_recursive_depth());
        assert_eq!(mc.loop_stats.current_depth, 2);
        assert!(mc.enter_recursive_depth());
        assert_eq!(mc.loop_stats.current_depth, 3);
        assert!(!mc.enter_recursive_depth());
        assert_eq!(mc.loop_stats.depth_rejections, 1);
    }

    #[test]
    fn test_exit_recursive_depth_saturating() {
        let mut mc = MetaCortex::new();
        mc.loop_stats.current_depth = 2;
        mc.exit_recursive_depth();
        assert_eq!(mc.loop_stats.current_depth, 1);
        mc.exit_recursive_depth();
        assert_eq!(mc.loop_stats.current_depth, 0);
        mc.exit_recursive_depth();
        assert_eq!(mc.loop_stats.current_depth, 0);
    }

    #[test]
    fn test_adapt_guardrails_high_success() {
        let mut mc = MetaCortex::new();
        mc.loop_stats.success_rate = 0.8;
        mc.adapt_guardrails();
        assert!(mc.loop_stats.effective_cooldown < DEFAULT_COOLDOWN);
        assert!(mc.loop_stats.effective_max_actions > MAX_ACTIONS_PER_WINDOW);
        assert!(!mc.loop_stats.conservative_mode);
    }

    #[test]
    fn test_adapt_guardrails_low_success_triggers_conservative() {
        let mut mc = MetaCortex::new();
        for _ in 0..3 {
            mc.outcome_window.push_back(false);
        }
        mc.loop_stats.success_rate = 0.0;
        mc.loop_stats.consecutive_failures = 3;
        mc.adapt_guardrails();
        assert!(mc.loop_stats.conservative_mode);
        assert_eq!(mc.loop_stats.effective_max_actions, 1);
    }

    #[test]
    fn test_classify_intervention_improved() {
        let before = VitalSnapshot {
            tick: 0, fitness: 0.4, pain: 0.5, energy_avg: 0.4, arousal: 0.5, firing_rate: 0.1, entropy: 0.2, novelty: 0.0,
        };
        let after = VitalSnapshot {
            tick: 50, fitness: 0.7, pain: 0.1, energy_avg: 0.8, arousal: 0.3, firing_rate: 0.15, entropy: 0.1, novelty: 0.0,
        };
        let record = InterventionRecord {
            id: 0, applied_tick: 0, action: MetaAction::InjectNoise,
            state_before: before, state_after_short: Some(after.clone()),
            state_after_long: Some(after), predicted_divergence: 0.1,
            outcome: InterventionOutcome::Pending, elapsed_ticks: 50, outcome_confidence: 0.0,
        };
        assert_eq!(classify_intervention_outcome(&record), InterventionOutcome::Improved);
    }

    #[test]
    fn test_classify_intervention_worsened() {
        let before = VitalSnapshot {
            tick: 0, fitness: 0.7, pain: 0.1, energy_avg: 0.8, arousal: 0.3, firing_rate: 0.1, entropy: 0.1, novelty: 0.0,
        };
        let after = VitalSnapshot {
            tick: 50, fitness: 0.3, pain: 0.8, energy_avg: 0.2, arousal: 0.6, firing_rate: 0.05, entropy: 0.4, novelty: 0.0,
        };
        let record = InterventionRecord {
            id: 0, applied_tick: 0, action: MetaAction::InjectNoise,
            state_before: before, state_after_short: Some(after.clone()),
            state_after_long: Some(after), predicted_divergence: 0.1,
            outcome: InterventionOutcome::Pending, elapsed_ticks: 50, outcome_confidence: 0.0,
        };
        assert_eq!(classify_intervention_outcome(&record), InterventionOutcome::Worsened);
    }

    #[test]
    fn test_classify_intervention_pending() {
        let before = VitalSnapshot {
            tick: 0, fitness: 0.5, pain: 0.2, energy_avg: 0.5, arousal: 0.5, firing_rate: 0.1, entropy: 0.2, novelty: 0.0,
        };
        let record = InterventionRecord {
            id: 0, applied_tick: 0, action: MetaAction::InjectNoise,
            state_before: before, state_after_short: None, state_after_long: None,
            predicted_divergence: 0.1,
            outcome: InterventionOutcome::Pending, elapsed_ticks: 0, outcome_confidence: 0.0,
        };
        assert_eq!(classify_intervention_outcome(&record), InterventionOutcome::Pending);
    }

    #[test]
    fn test_outcome_confidence_with_long_term() {
        let before = VitalSnapshot {
            tick: 0, fitness: 0.3, pain: 0.8, energy_avg: 0.2, arousal: 0.6, firing_rate: 0.05, entropy: 0.4, novelty: 0.0,
        };
        let after = VitalSnapshot {
            tick: 50, fitness: 0.7, pain: 0.1, energy_avg: 0.8, arousal: 0.3, firing_rate: 0.15, entropy: 0.1, novelty: 0.0,
        };
        let record = InterventionRecord {
            id: 0, applied_tick: 0, action: MetaAction::InjectNoise,
            state_before: before, state_after_short: Some(after.clone()),
            state_after_long: Some(after), predicted_divergence: 0.1,
            outcome: InterventionOutcome::Improved, elapsed_ticks: 50, outcome_confidence: 0.0,
        };
        let conf = outcome_confidence_for(&record);
        assert!(conf > 0.5, "Confidence should be >0.5 with both measurements, got {}", conf);
        assert!(conf <= 1.0);
    }

    #[test]
    fn test_outcome_confidence_no_data() {
        let before = VitalSnapshot {
            tick: 0, fitness: 0.5, pain: 0.2, energy_avg: 0.5, arousal: 0.5, firing_rate: 0.1, entropy: 0.2, novelty: 0.0,
        };
        let record = InterventionRecord {
            id: 0, applied_tick: 0, action: MetaAction::InjectNoise,
            state_before: before, state_after_short: None, state_after_long: None,
            predicted_divergence: 0.1,
            outcome: InterventionOutcome::Pending, elapsed_ticks: 0, outcome_confidence: 0.0,
        };
        assert_eq!(outcome_confidence_for(&record), 0.0);
    }

    #[test]
    fn test_loop_summary_contains_keywords() {
        let mut mc = MetaCortex::new();
        mc.loop_stats.total_interventions = 5;
        mc.loop_stats.success_rate = 0.6;
        let summary = mc.loop_summary();
        assert!(summary.contains("phase="));
        assert!(summary.contains("depth="));
        assert!(summary.contains("interventions=5"));
        assert!(summary.contains("success="));
    }

    #[test]
    fn test_should_consolidate_learning_initial() {
        let mc = MetaCortex::new();
        assert!(!mc.should_consolidate_learning());
    }

    #[test]
    fn test_should_consolidate_learning_after_cycles() {
        let mut mc = MetaCortex::new();
        mc.force_model_ready();
        mc.loop_stats.completed_cycles = 10;
        assert!(mc.should_consolidate_learning());
    }
}
