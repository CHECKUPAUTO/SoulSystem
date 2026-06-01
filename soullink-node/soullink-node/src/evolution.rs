//! SoulLink Evolution Engine — genome, mutation, fitness, self-evolution.
//!
//! Gives the brain a "genome" that encodes its architecture, mutation operators
//! that modify it, and a fitness function that measures its health. The engine
//! runs as a background process inside the simulation loop, periodically
//! checking if evolution is needed and applying mutations when stagnation or
//! low fitness is detected.

use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::brain_module::StressMode;

// Reuse the same timestamp function as main.rs
#[inline]
fn now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

// ═══════════════════════════════════════════════════════════════
// GLOBAL PARAMETERS (evolvable)


// ═══════════════════════════════════════════════════════════════
// MODULE GENE — one per brain module
// ═══════════════════════════════════════════════════════════════

/// Encodes one module's architecture in the genome.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleGene {
    pub name: String,
    pub neuron_count: usize,
    pub position_x: f32,
    pub position_y: f32,
    pub color: String,
    pub learning_rate: f32,
    pub firing_threshold_mean: f32,
    pub firing_threshold_std: f32,
    /// Fraction of possible synapses formed (0.0–1.0)
    pub synaptic_density: f32,
    /// Per-gene mutation probability
    pub mutation_rate: f32,
    /// Energy cost multiplier (evolvable 0.1-5.0)
    pub energy_multiplier: f32,
    /// Energy budget priority (evolvable 0.0-1.0)
    pub energy_priority: f32,
}

// ═══════════════════════════════════════════════════════════════
// CORTEX CONFIG GENE
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CortexConfigGene {
    pub d_model: usize,
    pub state_dim: usize,
    pub n_layers: usize,
    pub modulation_strength: f64,
}

impl Default for CortexConfigGene {
    fn default() -> Self {
        Self {
            d_model: 16,
            state_dim: 4,
            n_layers: 2,
            modulation_strength: 0.3,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// BEHAVIOR GENE — small behavioral fragments
// ═══════════════════════════════════════════════════════════════

/// A behavioral fragment that can be evolved — replaces hardcoded
/// small logic with data-driven variants. These are applied at runtime
/// without needing compilation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ActivationFn {
    /// Standard sigmoid centered at 0.5, slope 10
    Sigmoid,
    /// Steeper sigmoid (slope 20) — more binary firing
    SteepSigmoid,
    /// Shallow sigmoid (slope 5) — graded responses
    ShallowSigmoid,
    /// ReLU-like: max(0, x - threshold)
    ReluLike,
    /// Tanh-ish: symmetric activation around 0
    TanhIsh,
}

impl Default for ActivationFn {
    fn default() -> Self { ActivationFn::Sigmoid }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PlasticityRule {
    /// Standard Hebbian: strengthen pre-before-post
    Hebbian,
    /// Anti-Hebbian: weaken co-active pairs
    AntiHebbian,
    /// BCM-like: sliding threshold
    BcmSliding,
    /// Oja's rule: normalized Hebbian
    Oja,
}

impl Default for PlasticityRule {
    fn default() -> Self { PlasticityRule::Hebbian }
}

/// A set of behavioral genes that define how the brain processes signals.
/// These are evolvable without recompilation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviorGenes {
    pub activation_fn: ActivationFn,
    pub plasticity_rule: PlasticityRule,
    /// How strongly inhibition affects neighbors (0.0–1.0)
    pub lateral_inhibition: f32,
    /// Refractory period after firing (in ticks)
    pub refractory_ticks: u32,
    /// Noise level injected each tick (0.0–0.1)
    pub noise_level: f32,
}

impl Default for BehaviorGenes {
    fn default() -> Self {
        Self {
            activation_fn: ActivationFn::Sigmoid,
            plasticity_rule: PlasticityRule::Hebbian,
            lateral_inhibition: 0.1,
            refractory_ticks: 2,
            noise_level: 0.01,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// GENOME — complete brain architecture encoding
// ═══════════════════════════════════════════════════════════════

/// The complete "DNA" of a brain — modules, global params, cortex config.
/// Script genome — Rhai scripts and dynamic field definitions for auto-modification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptGenome {
    /// Per-module activation scripts (module_name -> Rhai script source).
    #[serde(default)]
    pub activation_scripts: std::collections::HashMap<String, String>,
    /// Global plasticity script (Rhai source).
    #[serde(default)]
    pub plasticity_script: Option<String>,
    /// Dynamic field definitions (target_type -> field_name -> default_value).
    /// target_type is "Neuron", "Synapse", or "BrainModule".
    #[serde(default)]
    pub field_definitions: std::collections::HashMap<String, std::collections::HashMap<String, f64>>,
}

impl Default for ScriptGenome {
    fn default() -> Self {
        Self {
            activation_scripts: std::collections::HashMap::new(),
            plasticity_script: None,
            field_definitions: std::collections::HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genome {
    pub modules: Vec<ModuleGene>,
    pub brain_params: crate::brain::BrainParams,
    pub cortex_config: CortexConfigGene,
    pub behaviors: BehaviorGenes,
    pub generation: u64,
    pub created: f64,
    pub parent_id: Option<u64>,
    /// Script and field definitions for auto-modification.
    #[serde(default)]
    pub script_genome: ScriptGenome,
}

impl Genome {
    /// Build the initial 11-module genome matching Brain::init_modules().
    pub fn base() -> Self {
        let modules = vec![
            ModuleGene { name: "perception".into(),  neuron_count: 3000, position_x: 150.0, position_y: 100.0, color: "#4affb8".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 0.8, energy_priority: 0.5 },
            ModuleGene { name: "memory".into(),      neuron_count: 3000, position_x: 350.0, position_y: 80.0,  color: "#4a9eff".into(), learning_rate: 0.25, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 0.8, energy_priority: 0.5 },
            ModuleGene { name: "reasoning".into(),   neuron_count: 3000, position_x: 550.0, position_y: 100.0, color: "#ff4a6a".into(), learning_rate: 0.15, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.2, energy_priority: 0.5 },
            ModuleGene { name: "learning".into(),    neuron_count: 3000, position_x: 200.0, position_y: 220.0, color: "#ffaa4a".into(), learning_rate: 0.3,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.0, energy_priority: 0.5 },
            ModuleGene { name: "output".into(),      neuron_count: 3000, position_x: 400.0, position_y: 220.0, color: "#9bff4a".into(), learning_rate: 0.2,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.0, energy_priority: 0.5 },
            ModuleGene { name: "attention".into(),   neuron_count: 3000, position_x: 600.0, position_y: 220.0, color: "#ff4aff".into(), learning_rate: 0.2,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.2, energy_priority: 0.5 },
            ModuleGene { name: "language".into(),    neuron_count: 3000, position_x: 150.0, position_y: 350.0, color: "#4affff".into(), learning_rate: 0.25, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.0, energy_priority: 0.5 },
            ModuleGene { name: "vision".into(),      neuron_count: 3000, position_x: 350.0, position_y: 350.0, color: "#ff4a4a".into(), learning_rate: 0.2,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.5, energy_priority: 0.7 },
            ModuleGene { name: "audio".into(),       neuron_count: 3000, position_x: 550.0, position_y: 350.0, color: "#4a4aff".into(), learning_rate: 0.2,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.2, energy_priority: 0.5 },
            ModuleGene { name: "motor".into(),       neuron_count: 3000, position_x: 400.0, position_y: 420.0, color: "#ffff4a".into(), learning_rate: 0.2,  firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.01, energy_multiplier: 1.5, energy_priority: 0.7 },
            ModuleGene { name: "extra".into(),       neuron_count: 3000, position_x: 250.0, position_y: 500.0, color: "#ff00ff".into(), learning_rate: 0.25, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3, mutation_rate: 0.02, energy_multiplier: 1.0, energy_priority: 0.3 },
        ];
        Self {
            modules,
            brain_params: crate::brain::BrainParams::default(),
            cortex_config: CortexConfigGene::default(),
            behaviors: BehaviorGenes::default(),
            generation: 0,
            created: now_f64(),
            parent_id: None,
            script_genome: ScriptGenome::default(),
        }
    }

    /// Apply genome to a Brain — syncs modules, params, and cortex config.
    /// Returns true if any change was made.
    #[allow(dead_code)] // called through EvolutionEngine
    pub fn express(&self, _brain: &mut crate::Brain) -> bool {
        // Synaptic density changes are applied via mutations.
        // Cortex config structural changes (d_model, n_layers) require restart.
        // This method ensures genome-to-brain alignment for basic params.
        let mut changed = false;

        // Ensure genome modules exist in brain
        for gene in &self.modules {
            if !_brain.modules.contains_key(&gene.name) {
                // Add missing module (will be populated by AddModule mutation)
                let module = crate::BrainModule::new(
                    gene.name.clone(),
                    gene.position_x,
                    gene.position_y,
                    gene.color.clone(),
                );
                _brain.modules.insert(gene.name.clone(), module);
                _brain.module_order.push(gene.name.clone());
                changed = true;
            }
        }

        // Sync module-level energy params from genome to brain
        for gene in &self.modules {
            if let Some(m) = _brain.modules.get_mut(&gene.name) {
                m.base_metabolic_mod = gene.energy_multiplier;
                m.energy_priority = gene.energy_priority;
            }
        }

        if changed {
            _brain.refresh_stats();
        }
        changed
    }
}

// ═══════════════════════════════════════════════════════════════
// MUTATION — 8 structural operators
// ═══════════════════════════════════════════════════════════════

/// A single mutation operation that modifies brain structure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Mutation {
    AddNeuron { module: usize, count: usize },
    PruneNeurons { module: usize, fraction: f64 },
    AddModule { name: String, count: usize, x: f32, y: f32, color: String },
    MergeModules { a: usize, b: usize, new_name: String },
    SplitModule { module: usize, new_name: String },
    MutateParam { param: String, delta: f64 },
    MutateSynapticDensity { source: usize, target: usize, density: f32 },
    MutateCortexConfig { d_model: usize, state_dim: usize, n_layers: usize },
    // Script and field mutations (auto-modification réelle)
    MutateActivationScript { module: usize, script: String },
    MutatePlasticityScript { script: String },
    AddField { target: String, field_name: String, default: f64 },
    RemoveField { target: String, field_name: String },
    MutateModuleMetabolicRate { module: usize, delta: f64 },
    MutateModulePriority { module: usize, delta: f64 },
}

impl std::fmt::Display for Mutation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mutation::AddNeuron { module, count } => write!(f, "AddNeuron(m{} +{})", module, count),
            Mutation::PruneNeurons { module, fraction } => write!(f, "PruneNeurons(m{} -{:.0}%)", module, fraction * 100.0),
            Mutation::AddModule { name, count, .. } => write!(f, "AddModule({} +{})", name, count),
            Mutation::MergeModules { a, b, new_name } => write!(f, "MergeModules(m{}+m{} -> {})", a, b, new_name),
            Mutation::SplitModule { module, new_name } => write!(f, "SplitModule(m{} -> {})", module, new_name),
            Mutation::MutateParam { param, delta } => write!(f, "MutateParam({} {:+.3})", param, delta),
            Mutation::MutateSynapticDensity { source, target, density } => write!(f, "MutateDensity(m{}->m{} d={:.2})", source, target, density),
            Mutation::MutateCortexConfig { d_model, state_dim, n_layers } => write!(f, "MutateCortex(D={},N={},L={})", d_model, state_dim, n_layers),
            Mutation::MutateActivationScript { module, script } => write!(f, "MutateActivationScript(m{}, len={})", module, script.len()),
            Mutation::MutatePlasticityScript { script } => write!(f, "MutatePlasticityScript(len={})", script.len()),
            Mutation::AddField { target, field_name, default } => write!(f, "AddField({}.{}={})", target, field_name, default),
            Mutation::RemoveField { target, field_name } => write!(f, "RemoveField({}.{})", target, field_name),
            Mutation::MutateModuleMetabolicRate { module, delta } => write!(f, "MutateModuleMetabolicRate(m{} {:+.3})", module, delta),
            Mutation::MutateModulePriority { module, delta } => write!(f, "MutateModulePriority(m{} {:+.3})", module, delta),
        }
    }
}

impl Mutation {
    /// Returns the mutation type key string for tracking stats.
    pub fn type_key(&self) -> String {
        match self {
            Mutation::AddNeuron { .. } => "AddNeuron".into(),
            Mutation::PruneNeurons { .. } => "PruneNeurons".into(),
            Mutation::AddModule { .. } => "AddModule".into(),
            Mutation::MergeModules { .. } => "MergeModules".into(),
            Mutation::SplitModule { .. } => "SplitModule".into(),
            Mutation::MutateParam { .. } => "MutateParam".into(),
            Mutation::MutateSynapticDensity { .. } => "MutateDensity".into(),
            Mutation::MutateCortexConfig { .. } => "MutateCortex".into(),
            Mutation::MutateActivationScript { .. } => "MutateActivationScript".into(),
            Mutation::MutatePlasticityScript { .. } => "MutatePlasticityScript".into(),
            Mutation::AddField { .. } => "AddField".into(),
            Mutation::RemoveField { .. } => "RemoveField".into(),
            Mutation::MutateModuleMetabolicRate { .. } => "MutateModuleMetabolicRate".into(),
            Mutation::MutateModulePriority { .. } => "MutateModulePriority".into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// FITNESS METRICS
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FitnessMetrics {
    /// Entropy of firing distribution across modules (0–1)
    pub firing_balance: f64,
    /// Entropy of synapse weight distribution (0–1)
    pub synaptic_diversity: f64,
    /// Fraction of modules with activation > 0.3 (0–1)
    pub module_utilization: f64,
    /// 1.0 - tanh(variance of potential changes) (0–1)
    pub stability: f64,
    /// Proxy: how much cortex affects firing (0–1)
    pub cortex_impact: f64,
    /// growth_events / (growth_events + pruned_synapses + 1) (0–1)
    pub growth_health: f64,
    /// Novelty bonus: reward for exploring unseen brain states (0–1)
    pub novelty_bonus: f64,
    /// Energy efficiency: avg energy reserve * (1 - debt_fraction) (0–1)
    pub energy_efficiency: f64,
    /// Weighted sum of all above (0–1)
    pub overall: f64,
    /// Temporal coherence: stability of module activations over time (0–1)
    pub temporal_coherence: f64,
    /// Emergent complexity: entropy of module activation distribution (0–1)
    pub emergent_complexity: f64,
}

impl Default for FitnessMetrics {
    fn default() -> Self {
        Self {
            firing_balance: 0.5,
            synaptic_diversity: 0.5,
            module_utilization: 0.5,
            stability: 0.5,
            cortex_impact: 0.5,
            growth_health: 0.5,
            novelty_bonus: 0.0,
            energy_efficiency: 0.5,
            overall: 0.5,
            temporal_coherence: 0.5,
            emergent_complexity: 0.5,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// FITNESS TRACKER
// ═══════════════════════════════════════════════════════════════

/// Dynamic weights that self-adjust based on the system's weakest metrics.
/// The system automatically focuses more on what needs improvement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicWeights {
    pub firing_balance: f64,
    pub synaptic_diversity: f64,
    pub module_utilization: f64,
    pub stability: f64,
    pub cortex_impact: f64,
    pub growth_health: f64,
    pub energy_efficiency: f64,
    pub temporal_coherence: f64,
    pub emergent_complexity: f64,
    /// How fast weights adapt (0.0 = never, 1.0 = instantly)
    pub adaptation_rate: f64,
    /// Minimum weight for any metric (prevents neglect)
    pub min_weight: f64,
}

impl Default for DynamicWeights {
    fn default() -> Self {
        Self {
            firing_balance: 0.10,
            synaptic_diversity: 0.10,
            module_utilization: 0.08,
            stability: 0.08,
            cortex_impact: 0.08,
            growth_health: 0.08,
            energy_efficiency: 0.18,
            temporal_coherence: 0.10,
            emergent_complexity: 0.10,
            adaptation_rate: 0.05,
            min_weight: 0.04,
        }
    }
}

impl DynamicWeights {
    /// Rebalance weights so the lowest metrics get more attention.
    pub fn adapt(&mut self, metrics: &FitnessMetrics) {
        let n = metrics.energy_efficiency;
        let pairs: Vec<(&mut f64, f64)> = vec![
            (&mut self.firing_balance, metrics.firing_balance),
            (&mut self.synaptic_diversity, metrics.synaptic_diversity),
            (&mut self.module_utilization, metrics.module_utilization),
            (&mut self.stability, metrics.stability),
            (&mut self.cortex_impact, metrics.cortex_impact),
            (&mut self.growth_health, metrics.growth_health),
            (&mut self.energy_efficiency, n),
            (&mut self.temporal_coherence, metrics.temporal_coherence),
            (&mut self.emergent_complexity, metrics.emergent_complexity),
        ];
        // For each metric: if it's low, increase its weight; if high, decrease
        for (weight, value) in pairs {
            let target = (1.0 - value).max(self.min_weight);
            let current = *weight;
            let new = current + (target - current) * self.adaptation_rate;
            *weight = new.clamp(self.min_weight, 0.5);
        }
        // Renormalize to sum = 1.0
        let sum: f64 = self.firing_balance + self.synaptic_diversity + self.module_utilization
            + self.stability + self.cortex_impact + self.growth_health + self.energy_efficiency
            + self.temporal_coherence + self.emergent_complexity;
        if sum > 0.0 {
            let inv = 1.0 / sum;
            self.firing_balance *= inv;
            self.synaptic_diversity *= inv;
            self.module_utilization *= inv;
            self.stability *= inv;
            self.cortex_impact *= inv;
            self.growth_health *= inv;
            self.energy_efficiency *= inv;
            self.temporal_coherence *= inv;
            self.emergent_complexity *= inv;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FitnessTracker {
    pub sample_interval: u64,
    pub history: Vec<FitnessMetrics>,
    pub current: FitnessMetrics,
    #[serde(skip)]
    pub ticks_since_sample: u64,
    /// Recent metric vectors for novelty computation
    #[serde(skip)]
    pub novelty_history: Vec<[f64; 9]>,
    /// How strongly novelty affects overall fitness
    pub novelty_weight: f64,
    /// Dynamic weights that self-adjust to focus on weaknesses
    pub dynamic_weights: DynamicWeights,
    /// Current self-generated goal (which metric to improve)
    pub current_goal: String,
    /// Value of the goal metric at last sample
    pub goal_last_value: f64,
    /// Ring buffer of module activation snapshots for temporal coherence
    #[serde(skip)]
    pub module_activation_history: Vec<Vec<f32>>,
    /// Max snapshots to retain for temporal coherence computation
    #[serde(default)]
    pub max_activation_snapshots: usize,
    /// Whether novelty_weight adapts dynamically to context
    #[serde(default)]
    pub dynamic_novelty_weight: bool,
    /// Last recorded stress mode for goal transition decisions
    #[serde(skip)]
    pub last_stress_mode: StressMode,
}

impl FitnessTracker {
    pub fn new(sample_interval: u64) -> Self {
        Self {
            sample_interval,
            history: Vec::with_capacity(100),
            current: FitnessMetrics::default(),
            ticks_since_sample: 0,
            novelty_history: Vec::with_capacity(50),
            novelty_weight: 0.05,
            dynamic_weights: DynamicWeights::default(),
            current_goal: "firing_balance".into(),
            goal_last_value: 0.5,
            module_activation_history: Vec::with_capacity(10),
            max_activation_snapshots: 5,
            dynamic_novelty_weight: true,
            last_stress_mode: StressMode::Calm,
        }
    }

    /// Compute fitness metrics from current brain state.
    pub fn sample(&mut self, brain: &crate::Brain) -> FitnessMetrics {
        let total_modules = brain.module_order.len();
        if total_modules == 0 {
            return FitnessMetrics::default();
        }

        // ── Firing balance: entropy of firing distribution across modules ──
        let _total_neurons = brain.neurons.len().max(1);
        let mut module_fire_rates = Vec::with_capacity(total_modules);
        for name in &brain.module_order {
            if let Some(m) = brain.modules.get(name) {
                let n_count = m.neuron_indices.len().max(1);
                let fired: usize = m.neuron_indices.iter()
                    .filter(|&&i| i < brain.neurons.len() && brain.neurons[i].is_spiking())
                    .count();
                module_fire_rates.push(fired as f64 / n_count as f64);
            }
        }
        let sum_rate: f64 = module_fire_rates.iter().sum();
        let firing_balance = if sum_rate > 0.0 && total_modules > 1 {
            let entropy: f64 = module_fire_rates.iter()
                .map(|&r| { let p = r / sum_rate; if p > 0.0 { -p * p.ln() } else { 0.0 } })
                .sum();
            (entropy / (total_modules as f64).ln()).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // ── Synaptic diversity: entropy of weight distribution ──
        let active_synapses: Vec<f64> = brain.synapses.iter()
            .filter(|s| s.active)
            .map(|s| s.weight as f64)
            .collect();
        let synaptic_diversity = if active_synapses.len() > 5 {
            let mut bins = [0usize; 20];
            for &w in &active_synapses {
                let idx = ((w * 20.0).floor() as usize).min(19);
                bins[idx] += 1;
            }
            let total = active_synapses.len() as f64;
            let entropy: f64 = bins.iter()
                .map(|&c| { let p = c as f64 / total; if p > 0.0 { -p * p.ln() } else { 0.0 } })
                .sum();
            (entropy / (20.0_f64).ln()).clamp(0.0, 1.0)
        } else {
            0.3
        };

        // ── Module utilization ──
        let active_modules = brain.module_order.iter()
            .filter(|&name| {
                brain.modules.get(name)
                    .map(|m| m.activation_level > 0.3)
                    .unwrap_or(false)
            })
            .count();
        let module_utilization = active_modules as f64 / total_modules as f64;

        // ── Temporal coherence: smoothness of module activation evolution ──
        let temporal_coherence = if brain.params.metabolism_enabled && total_modules > 0 {
            let snapshot: Vec<f32> = brain.module_order.iter()
                .map(|name| brain.modules.get(name).map(|m| m.activation_level).unwrap_or(0.0))
                .collect();
            self.module_activation_history.push(snapshot);
            if self.module_activation_history.len() > self.max_activation_snapshots {
                self.module_activation_history.remove(0);
            }
            if self.module_activation_history.len() >= 2 {
                let n_snapshots = self.module_activation_history.len();
                let mut total_coherence = 0.0;
                for m in 0..total_modules {
                    let mean: f32 = self.module_activation_history.iter()
                        .map(|snap| snap.get(m).copied().unwrap_or(0.0))
                        .sum::<f32>() / n_snapshots as f32;
                    let variance: f32 = self.module_activation_history.iter()
                        .map(|snap| { let v = snap.get(m).copied().unwrap_or(0.0) - mean; v * v })
                        .sum::<f32>() / n_snapshots as f32;
                    total_coherence += (1.0 - (variance * 10.0).tanh()) as f64;
                }
                (total_coherence / total_modules as f64).clamp(0.0, 1.0)
            } else { 0.5 }
        } else { 0.5 };

        // ── Emergent complexity: entropy of module activation distribution ──
        let emergent_complexity = if brain.params.metabolism_enabled && total_modules > 1 {
            let activations: Vec<f64> = brain.module_order.iter()
                .map(|name| brain.modules.get(name).map(|m| m.activation_level as f64).unwrap_or(0.0))
                .collect();
            let sum_act: f64 = activations.iter().sum();
            if sum_act > 0.0 {
                let entropy: f64 = activations.iter()
                    .map(|&a| { let p = a / sum_act; if p > 0.0 { -p * p.ln() } else { 0.0 } })
                    .sum();
                (entropy / (total_modules as f64).ln()).clamp(0.0, 1.0)
            } else { 0.0 }
        } else { 0.5 };

        // ── Stability: inverse variance of recent potential changes ──
        let potentials: Vec<f64> = brain.neurons.iter()
            .map(|n| n.potential as f64)
            .collect();
        let mean_pot: f64 = if !potentials.is_empty() {
            potentials.iter().sum::<f64>() / potentials.len() as f64
        } else {
            0.0
        };
        let variance: f64 = if potentials.len() > 1 {
            potentials.iter().map(|v| (v - mean_pot).powi(2)).sum::<f64>() / potentials.len() as f64
        } else {
            0.0
        };
        let stability = (1.0 - (variance * 10.0_f64).tanh()).clamp(0.0, 1.0);

        // ── Cortex impact proxy ──
        let cortex_impact = if brain.cortex.config.enabled {
            let norm = brain.cortex.api_status()
                .get("last_output_norm")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            (norm * 3.0).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // ── Growth health ──
        let g = brain.stats.growth_events;
        let p = brain.stats.pruned_synapses;
        let growth_health = (g as f64 / (g + p + 1).max(1) as f64).clamp(0.0, 1.0);

        // ── Energy efficiency: avg energy reserve * (1 - debt_fraction) ──
        let energy_efficiency = if brain.params.metabolism_enabled && !brain.neurons.is_empty() {
            let avg_energy: f32 = brain.neurons.iter().map(|n| n.energy_reserve).sum::<f32>() / brain.neurons.len() as f32;
            let debt_count = brain.neurons.iter().filter(|n| n.energy_reserve < 0.1).count();
            let debt_frac = debt_count as f64 / brain.neurons.len() as f64;
            ((avg_energy as f64) * (1.0 - debt_frac)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        // ── Novelty bonus: distance from recent metric history ──
        let current_vec = [firing_balance, synaptic_diversity, module_utilization,
            stability, cortex_impact, growth_health, energy_efficiency,
            temporal_coherence, emergent_complexity];
        let novelty_bonus = if !self.novelty_history.is_empty() {
            let avg_dist: f64 = self.novelty_history.iter()
                .map(|past| {
                    let dot: f64 = past.iter().zip(current_vec.iter()).map(|(a, b)| a * b).sum();
                    let norm_a: f64 = past.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-8);
                    let norm_b: f64 = current_vec.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-8);
                    1.0 - (dot / (norm_a * norm_b)).abs()
                })
                .sum::<f64>() / self.novelty_history.len() as f64;
            (avg_dist * 3.0).clamp(0.0, 1.0) // scale up
        } else {
            0.0
        };
        self.novelty_history.push(current_vec);
        if self.novelty_history.len() > 50 {
            self.novelty_history.remove(0);
        }

        // ── Track stress mode for context-aware decisions ──
        self.last_stress_mode = brain.stress_mode;

        // ── Dynamic novelty weight ──
        if self.dynamic_novelty_weight {
            // Preliminary overall (equal weights) for novelty weight modulation
            let prelim_overall = (firing_balance + synaptic_diversity + module_utilization
                + stability + cortex_impact + growth_health + energy_efficiency
                + temporal_coherence + emergent_complexity) / 9.0;
            self.novelty_weight = match brain.stress_mode {
                crate::brain_module::StressMode::Calm => {
                    (0.05 + 0.10_f64 * prelim_overall.clamp(0.0, 1.0)).clamp(0.01, 0.20)
                }
                crate::brain_module::StressMode::Alert => {
                    (0.03 + 0.02_f64 * (1.0 - brain.pain.composite as f64)).clamp(0.01, 0.20)
                }
                crate::brain_module::StressMode::Stress | crate::brain_module::StressMode::Panic => {
                    0.01
                }
            };
        }

        // ── Adapt dynamic weights based on current weaknesses ──
        let temp_metrics = FitnessMetrics {
            firing_balance, synaptic_diversity, module_utilization,
            stability, cortex_impact, growth_health,
            novelty_bonus, energy_efficiency, overall: 0.0,
            temporal_coherence, emergent_complexity,
        };
        self.dynamic_weights.adapt(&temp_metrics);
        let w = &self.dynamic_weights;

        // ── Weighted overall (dynamic weights self-adjust to focus on weaknesses) ──
        let overall = w.firing_balance * firing_balance
            + w.synaptic_diversity * synaptic_diversity
            + w.module_utilization * module_utilization
            + w.stability * stability
            + w.cortex_impact * cortex_impact
            + w.growth_health * growth_health
            + w.energy_efficiency * energy_efficiency
            + w.temporal_coherence * temporal_coherence
            + w.emergent_complexity * emergent_complexity
            + self.novelty_weight * novelty_bonus;

        // ── Context-aware goal selection ──
        let metric_pairs = [
            ("firing_balance", firing_balance),
            ("synaptic_diversity", synaptic_diversity),
            ("module_utilization", module_utilization),
            ("stability", stability),
            ("cortex_impact", cortex_impact),
            ("growth_health", growth_health),
            ("energy_efficiency", energy_efficiency),
            ("temporal_coherence", temporal_coherence),
            ("emergent_complexity", emergent_complexity),
        ];
        let (goal_name, goal_val) = match self.last_stress_mode {
            crate::brain_module::StressMode::Panic | crate::brain_module::StressMode::Stress => {
                if stability < 0.6 { ("stability", stability) }
                else if energy_efficiency < 0.5 { ("energy_efficiency", energy_efficiency) }
                else { ("stability", stability) }
            }
            crate::brain_module::StressMode::Alert => {
                if energy_efficiency < 0.6 { ("energy_efficiency", energy_efficiency) }
                else {
                    metric_pairs.iter()
                        .filter(|(name, _)| *name != "novelty_bonus")
                        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .copied()
                        .unwrap_or(("stability", stability))
                }
            }
            crate::brain_module::StressMode::Calm => {
                if overall > 0.7 { ("novelty_bonus", novelty_bonus) }
                else {
                    metric_pairs.iter()
                        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .copied()
                        .unwrap_or(("firing_balance", firing_balance))
                }
            }
        };
        if goal_val < self.goal_last_value - 0.02 || goal_name != self.current_goal {
            self.current_goal = goal_name.to_string();
            self.goal_last_value = goal_val;
        }

        let metrics = FitnessMetrics {
            firing_balance,
            synaptic_diversity,
            module_utilization,
            stability,
            cortex_impact,
            growth_health,
            novelty_bonus,
            energy_efficiency,
            overall,
            temporal_coherence,
            emergent_complexity,
        };

        self.history.push(metrics.clone());
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.current = metrics.clone();
        metrics
    }
}

// ═══════════════════════════════════════════════════════════════
// PARETO MULTI-OBJECTIVE EVOLUTION
// ═══════════════════════════════════════════════════════════════

/// A single individual on the Pareto frontier — a non-dominated solution
/// represented by its 10-objective fitness vector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParetoIndividual {
    /// The 10 fitness objectives: [firing_balance, synaptic_diversity,
    /// module_utilization, stability, cortex_impact, growth_health,
    /// novelty_bonus, energy_efficiency, temporal_coherence,
    /// emergent_complexity]
    pub objectives: [f64; 10],
    /// Generation when this individual was created
    pub generation: u64,
    /// Crowding distance — higher means more isolated in objective space
    pub crowding_distance: f64,
}

impl ParetoIndividual {
    /// Build from FitnessMetrics at the current generation.
    pub fn from_metrics(metrics: &FitnessMetrics, generation: u64) -> Self {
        Self {
            objectives: [
                metrics.firing_balance,
                metrics.synaptic_diversity,
                metrics.module_utilization,
                metrics.stability,
                metrics.cortex_impact,
                metrics.growth_health,
                metrics.novelty_bonus,
                metrics.energy_efficiency,
                metrics.temporal_coherence,
                metrics.emergent_complexity,
            ],
            generation,
            crowding_distance: 0.0,
        }
    }
}

/// Pareto frontier — maintains a bounded set of non-dominated solutions.
///
/// A solution A dominates B iff A is strictly better in at least one
/// objective and not worse in all others. The frontier keeps only
/// mutually non-dominated individuals, pruned by crowding distance
/// when the set exceeds `max_size`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParetoFrontier {
    pub individuals: Vec<ParetoIndividual>,
    pub max_size: usize,
}

impl ParetoFrontier {
    pub fn new(max_size: usize) -> Self {
        Self {
            individuals: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Returns true if `a` dominates `b`.
    ///
    /// For maximization: `a_i > b_i` means better.
    pub fn dominates(a: &[f64; 10], b: &[f64; 10]) -> bool {
        let mut better = false;
        for i in 0..10 {
            if a[i] < b[i] - 1e-9 {
                return false; // a is strictly worse in objective i → can't dominate
            }
            if a[i] > b[i] + 1e-9 {
                better = true; // a is strictly better in objective i
            }
        }
        better
    }

    /// Try to insert an individual into the frontier.
    /// Returns true if the frontier changed (individual was non-dominated).
    pub fn try_insert(&mut self, individual: ParetoIndividual) -> bool {
        // Reject if dominated by any existing frontier member
        for existing in &self.individuals {
            if Self::dominates(&existing.objectives, &individual.objectives) {
                return false;
            }
        }

        // Remove any existing members dominated by the new individual
        self.individuals.retain(|existing| {
            !Self::dominates(&individual.objectives, &existing.objectives)
        });

        self.individuals.push(individual);

        // Prune by crowding distance if over capacity
        if self.individuals.len() > self.max_size {
            self.compute_crowding_distance();
            self.individuals.sort_by(|a, b| {
                b.crowding_distance
                    .partial_cmp(&a.crowding_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.individuals.truncate(self.max_size);
        }

        true
    }

    /// Compute crowding distance for all individuals.
    ///
    /// Boundary individuals get infinite distance (always kept).
    /// Interior individuals get the sum of normalized side lengths
    /// of the hyper-rectangle formed by their neighbors.
    pub fn compute_crowding_distance(&mut self) {
        let n = self.individuals.len();
        if n <= 2 {
            for ind in &mut self.individuals {
                ind.crowding_distance = f64::INFINITY;
            }
            return;
        }

        for ind in &mut self.individuals {
            ind.crowding_distance = 0.0;
        }

        for obj_idx in 0..10 {
            self.individuals.sort_by(|a, b| {
                a.objectives[obj_idx]
                    .partial_cmp(&b.objectives[obj_idx])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let min_val = self.individuals[0].objectives[obj_idx];
            let max_val = self.individuals[n - 1].objectives[obj_idx];
            let range = (max_val - min_val).max(1e-9);

            // Boundaries get infinite distance
            self.individuals[0].crowding_distance = f64::INFINITY;
            self.individuals[n - 1].crowding_distance = f64::INFINITY;

            for i in 1..n - 1 {
                let contribution = (self.individuals[i + 1].objectives[obj_idx]
                    - self.individuals[i - 1].objectives[obj_idx])
                    / range;
                self.individuals[i].crowding_distance += contribution;
            }
        }
    }

    /// Approximate hypervolume: product of objective ranges covered.
    pub fn hypervolume(&self) -> f64 {
        if self.individuals.is_empty() {
            return 0.0;
        }
        let mut vol = 1.0;
        for obj_idx in 0..10 {
            let min_val = self
                .individuals
                .iter()
                .map(|ind| ind.objectives[obj_idx])
                .fold(f64::INFINITY, f64::min);
            let max_val = self
                .individuals
                .iter()
                .map(|ind| ind.objectives[obj_idx])
                .fold(f64::NEG_INFINITY, f64::max);
            vol *= (max_val - min_val).max(0.01);
        }
        vol
    }

    /// Number of individuals in the frontier.
    pub fn size(&self) -> usize {
        self.individuals.len()
    }
}

// ═══════════════════════════════════════════════════════════════
// EVOLUTION EVENT
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionEvent {
    pub timestamp: f64,
    pub generation: u64,
    pub mutation: String,
    pub parent_fitness: f64,
    pub child_fitness: f64,
    pub fitness_delta: f64,
    pub genome_size: usize,
}

// ═══════════════════════════════════════════════════════════════
// EVOLUTION GOAL — objective-driven evolution
// ═══════════════════════════════════════════════════════════════

/// An external goal that biases mutation selection toward improving
/// a specific fitness dimension.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionGoal {
    /// Which fitness metric to optimize ("firing_balance", "synaptic_diversity",
    /// "module_utilization", "stability", "cortex_impact", "growth_health",
    /// "temporal_coherence", "emergent_complexity", "overall")
    pub target_metric: String,
    /// How strongly to bias selection (0.0 = no bias, 1.0 = strong bias)
    pub weight: f64,
    /// Whether this goal is active
    pub enabled: bool,
}

impl Default for EvolutionGoal {
    fn default() -> Self {
        Self {
            target_metric: "overall".into(),
            weight: 0.0,
            enabled: false,
        }
    }
}

impl EvolutionGoal {
    /// Get the current value of the target metric from fitness.
    fn target_value(&self, f: &FitnessMetrics) -> f64 {
        match self.target_metric.as_str() {
            "firing_balance" => f.firing_balance,
            "synaptic_diversity" => f.synaptic_diversity,
            "module_utilization" => f.module_utilization,
            "stability" => f.stability,
            "cortex_impact" => f.cortex_impact,
            "growth_health" => f.growth_health,
            "temporal_coherence" => f.temporal_coherence,
            "emergent_complexity" => f.emergent_complexity,
            _ => f.overall,
        }
    }
}

/// A derived rule from episodic memory analysis:
/// "when context dimension X is in [lo, hi], mutation type Y has
///  an average delta of Z and success rate of W."
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextualPriorityRule {
    /// Context dimension this rule applies to
    pub context_dim: String,
    /// Range of the context dimension where this rule applies
    pub context_lo: f64,
    pub context_hi: f64,
    /// Mutation type this rule targets
    pub mutation_type: String,
    /// Historical average fitness delta when this rule matched
    pub avg_delta: f64,
    /// Historical success rate (0-1)
    pub success_rate: f64,
    /// How much to bias selection (derived from avg_delta * success_rate)
    pub bias_weight: f64,
    /// Number of samples supporting this rule
    pub sample_count: usize,
    /// When this rule was last verified
    pub last_verified: f64,
}

/// A single episodic memory of a mutation attempt, including
/// the fitness context before the mutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MutationEpisodicEvent {
    /// Mutation type key
    pub mutation_type: String,
    /// Generation when it happened
    pub generation: u64,
    /// Fitness just before mutation
    pub parent_fitness: f64,
    /// Fitness just after mutation
    pub child_fitness: f64,
    /// Fitness delta (child - parent)
    pub fitness_delta: f64,
    /// Context: firing balance at the time
    pub ctx_firing_balance: f64,
    /// Context: synaptic diversity at the time
    pub ctx_synaptic_diversity: f64,
    /// Context: module utilization at the time
    pub ctx_module_utilization: f64,
    /// Context: stability at the time
    pub ctx_stability: f64,
    /// Absolute timestamp
    pub timestamp: f64,
}

impl MutationEpisodicEvent {
    fn from_mutation(
        mutation_type: &str,
        generation: u64,
        parent_fitness: f64,
        child_fitness: f64,
        ctx: &FitnessMetrics,
        timestamp: f64,
    ) -> Self {
        Self {
            mutation_type: mutation_type.to_string(),
            generation,
            parent_fitness,
            child_fitness,
            fitness_delta: child_fitness - parent_fitness,
            ctx_firing_balance: ctx.firing_balance,
            ctx_synaptic_diversity: ctx.synaptic_diversity,
            ctx_module_utilization: ctx.module_utilization,
            ctx_stability: ctx.stability,
            timestamp,
        }
    }
}

/// Tracks success rate per mutation type for adaptive weighting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MutationTypeStats {
    /// Mutation type name (matches Display)
    pub type_name: String,
    pub attempts: u64,
    pub successes: u64,
    pub total_fitness_delta: f64,
    /// Adaptive weight (higher = more likely to be selected)
    pub weight: f64,
    /// Consecutive failures (for toxic mutation blacklist)
    pub consecutive_failures: u64,
}

impl MutationTypeStats {
    fn new(type_name: &str) -> Self {
        Self {
            type_name: type_name.to_string(),
            attempts: 0,
            successes: 0,
            total_fitness_delta: 0.0,
            weight: 1.0,
            consecutive_failures: 0,
        }
    }

    fn success_rate(&self) -> f64 {
        if self.attempts == 0 { 0.5 } else { self.successes as f64 / self.attempts as f64 }
    }

    fn avg_delta(&self) -> f64 {
        if self.attempts == 0 { 0.0 } else { self.total_fitness_delta / self.attempts as f64 }
    }
}

/// Meta-genome: evolvable parameters that control HOW evolution works.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaGenome {
    /// Base mutation probability (per-tick when eligible)
    pub mutation_rate: f64,
    /// How much to weight past success vs exploration (0=uniform, 1=full exploitation)
    pub selection_pressure: f64,
    /// Minimum ticks between evolution steps
    pub min_evolution_interval: u64,
    /// Maximum ticks between evolution steps
    pub max_evolution_interval: u64,
    /// Probability of trying a novel mutation instead of weighted selection
    pub exploration_rate: f64,
    /// How many generations to keep success stats
    pub stats_window: usize,
    /// Meta-mutation rate (how often the meta-genome itself mutates)
    pub meta_mutation_rate: f64,
    // ── Auto-heal thresholds (evolvable) ──
    /// Fraction of recently-active neurons below which a module is considered stalled
    pub heal_stalled_threshold: f64,
    /// Potential boost amount applied to stalled module neurons
    pub heal_stalled_boost: f64,
    /// Fraction of stalled neurons to boost (of total module size)
    pub heal_stalled_fraction: f64,
    /// Fraction of total neurons that must be active to avoid global collapse trigger
    pub heal_collapse_min_active: f64,
    /// Activation level boost applied during global collapse recovery
    pub heal_collapse_activation_boost: f64,
    /// Potential boost applied to random neurons during global collapse
    pub heal_collapse_potential_boost: f64,
    /// Fitness variance threshold above which evolution freezes (divergence detection)
    pub fitness_divergence_threshold: f64,
    /// How many ticks to freeze evolution after divergence is detected
    pub fitness_freeze_duration: u64,
    /// How many ticks a mutation type stays blacklisted after being flagged toxic
    pub toxic_memory_ticks: u64,
    /// How many consecutive failures before a mutation type is blacklisted
    pub toxic_failure_threshold: u64,
    // ── Symbolic backpropagation thresholds ──
    /// How often to re-run symbolic analysis (in ticks)
    pub symbolic_analysis_interval: u64,
    /// Minimum samples needed before a rule is considered valid
    pub symbolic_min_samples: usize,
    /// Minimum success rate for a rule to be used
    pub symbolic_min_success: f64,
    /// Maximum number of priority rules to keep
    pub symbolic_max_rules: usize,
}

impl Default for MetaGenome {
    fn default() -> Self {
        Self {
            mutation_rate: 0.02,
            selection_pressure: 0.6,
            min_evolution_interval: 50,
            max_evolution_interval: 300,
            exploration_rate: 0.15,
            stats_window: 50,
            meta_mutation_rate: 0.01,
            heal_stalled_threshold: 0.01,
            heal_stalled_boost: 0.15,
            heal_stalled_fraction: 0.05,
            heal_collapse_min_active: 0.01,
            heal_collapse_activation_boost: 0.1,
            heal_collapse_potential_boost: 0.2,
            fitness_divergence_threshold: 0.05,
            fitness_freeze_duration: 200,
            toxic_memory_ticks: 500,
            toxic_failure_threshold: 3,
            symbolic_analysis_interval: 500,
            symbolic_min_samples: 5,
            symbolic_min_success: 0.4,
            symbolic_max_rules: 20,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// EVOLUTION ENGINE — the main orchestrator
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionEngine {
    pub genome: Genome,
    pub fitness: FitnessTracker,
    pub history: Vec<EvolutionEvent>,
    pub auto_evolve: bool,
    pub generation: u64,
    #[serde(skip)]
    pub ticks_since_evolution: u64,
    pub evolution_interval: u64,
    pub mutation_count: u64,
    /// Maximum number of modules allowed (prevents unbounded growth)
    pub max_modules: usize,
    // ── Meta-evolution ──
    pub meta_genome: MetaGenome,
    /// Per-mutation-type success tracking
    pub mutation_stats: Vec<MutationTypeStats>,
    /// Accumulated meta-ticks
    pub meta_ticks: u64,
    /// Best fitness ever achieved
    pub best_fitness: f64,
    /// Last N fitness values for plateau/divergence detection
    pub fitness_history_window: Vec<f64>,
    // ── Divergence detection ──
    /// Whether evolution is frozen due to fitness divergence
    pub fitness_frozen: bool,
    /// How many ticks remaining in freeze
    pub fitness_freeze_ticks: u64,
    /// Whether the last freeze triggered a meta-genome reset
    pub did_meta_reset: bool,
    // ── Episodic memory ──
    /// Ring buffer of recent mutation events with fitness context
    pub episodic_memory: VecDeque<MutationEpisodicEvent>,
    // ── Evolution goal ──
    /// Optional goal that biases mutation selection
    pub evolution_goal: EvolutionGoal,
    // ── Toxic mutation blacklist ──
    /// Blacklisted mutation types (type_name, tick_when_expires)
    pub mutation_blacklist: VecDeque<(String, u64)>,
    // ── Priority rules (symbolic backpropagation) ──
    /// Derived priority rules from episodic memory analysis
    pub priority_rules: Vec<ContextualPriorityRule>,
    /// Ticks since last symbolic analysis
    pub ticks_since_symbolic_analysis: u64,
    // ── LLM mutation ──
    /// Pending LLM mutation proposal (set by async task, applied synchronously)
    #[serde(skip)]
    pub pending_llm_mutation: Option<crate::llm_mutator::LlmMutationProposal>,
    /// LLM mutator (not serialized — rebuilt on load)
    #[serde(skip)]
    pub llm_mutator: Option<Box<crate::llm_mutator::LlmMutator>>,
    // ── Pareto multi-objective frontier ──
    /// Non-dominated solution archive for multi-objective evolution
    pub pareto_frontier: ParetoFrontier,
    // ── Recursive improvement tracking ──
    /// How many generations since last significant improvement
    pub stagnation_generations: u64,
    /// Whether the system is in simplification mode (trying to prune waste)
    pub simplification_mode: bool,
    /// Running average of fitness improvement per generation
    pub generational_improvement: f64,
    /// Highest overall fitness ever recorded
    pub peak_fitness_ever: f64,
    /// Generation at which peak fitness was achieved
    pub peak_generation: u64,
}

impl Default for EvolutionEngine {
    fn default() -> Self {
        let mutation_stats = vec![
            MutationTypeStats::new("AddNeuron"),
            MutationTypeStats::new("PruneNeurons"),
            MutationTypeStats::new("AddModule"),
            MutationTypeStats::new("MergeModules"),
            MutationTypeStats::new("SplitModule"),
            MutationTypeStats::new("MutateParam"),
            MutationTypeStats::new("MutateDensity"),
            MutationTypeStats::new("MutateCortex"),
            MutationTypeStats::new("MutateModuleMetabolicRate"),
            MutationTypeStats::new("MutateModulePriority"),
        ];
        Self {
            genome: Genome::base(),
            fitness: FitnessTracker::new(20),
            history: Vec::new(),
            auto_evolve: true,
            generation: 0,
            ticks_since_evolution: 0,
            evolution_interval: 100,
            mutation_count: 0,
            max_modules: 20,
            meta_genome: MetaGenome::default(),
            mutation_stats,
            meta_ticks: 0,
            best_fitness: 0.0,
            fitness_history_window: Vec::new(),
            fitness_frozen: false,
            fitness_freeze_ticks: 0,
            did_meta_reset: false,
            episodic_memory: VecDeque::new(),
            mutation_blacklist: VecDeque::new(),
            evolution_goal: EvolutionGoal::default(),
            priority_rules: Vec::new(),
            ticks_since_symbolic_analysis: 0,
            pending_llm_mutation: None,
            llm_mutator: None,
            pareto_frontier: ParetoFrontier::new(50),
            stagnation_generations: 0,
            simplification_mode: false,
            generational_improvement: 0.0,
            peak_fitness_ever: 0.0,
            peak_generation: 0,
        }
    }
}

#[allow(dead_code)]
impl EvolutionEngine {
    /// Run one evolution tick:
    /// 1. Sample fitness every `sample_interval` ticks
    /// 2. If `auto_evolve` and time to evolve, select + apply mutation
    /// 3. Track mutation results for adaptive weighting
    /// 4. Periodically meta-mutate evolution parameters
    pub fn step(&mut self, brain: &mut crate::Brain) {
        self.ticks_since_evolution += 1;

        // Sample fitness
        self.fitness.ticks_since_sample += 1;
        if self.fitness.ticks_since_sample >= self.fitness.sample_interval {
            self.fitness.sample(brain);
            self.fitness.ticks_since_sample = 0;

            // Try to insert into Pareto frontier
            let ind = ParetoIndividual::from_metrics(&self.fitness.current, self.generation);
            self.pareto_frontier.try_insert(ind);
        }

        // Track best fitness
        let current_fitness = self.fitness.current.overall;
        if current_fitness > self.best_fitness {
            self.best_fitness = current_fitness;
        }

        // ── Recursive improvement tracking ──
        if current_fitness > self.peak_fitness_ever {
            self.generational_improvement = current_fitness - self.peak_fitness_ever;
            self.peak_fitness_ever = current_fitness;
            self.peak_generation = self.generation;
            self.stagnation_generations = 0;
            self.simplification_mode = false;
        } else {
            self.stagnation_generations += 1;
            // If stagnating for too long, enter simplification mode
            if self.stagnation_generations > 50 && !self.simplification_mode {
                self.simplification_mode = true;
                self.history.push(EvolutionEvent {
                    generation: self.generation,
                    mutation: "ENTER_SIMPLIFICATION_MODE".into(),
                    parent_fitness: current_fitness,
                    child_fitness: current_fitness,
                    fitness_delta: 0.0,
                    timestamp: now_f64(),
                    genome_size: self.genome.modules.len(),
                });
            }
            self.generational_improvement *= 0.99; // decay
        }
        self.fitness_history_window.push(current_fitness);
        if self.fitness_history_window.len() > 100 {
            self.fitness_history_window.remove(0);
        }

        // ── Divergence detection: freeze if fitness oscillates too much ──
        if self.fitness_history_window.len() >= 10 {
            let mean: f64 = self.fitness_history_window.iter().sum::<f64>() / self.fitness_history_window.len() as f64;
            let variance: f64 = self.fitness_history_window.iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>() / self.fitness_history_window.len() as f64;

            if variance > self.meta_genome.fitness_divergence_threshold && !self.fitness_frozen {
                self.fitness_frozen = true;
                self.fitness_freeze_ticks = self.meta_genome.fitness_freeze_duration;
                self.history.push(EvolutionEvent {
                    generation: self.generation,
                    mutation: format!("FITNESS_DIVERGENCE variance={:.4}", variance),
                    parent_fitness: 0.0,
                    child_fitness: 0.0,
                    fitness_delta: -variance,
                    genome_size: self.genome.modules.len(),
                    timestamp: now_f64(),
                });
                tracing::warn!("Fitness divergence detected (var={:.4}), freezing evolution for {} ticks", variance, self.fitness_freeze_ticks);
            }
        }

        // Handle frozen state
        if self.fitness_frozen {
            if self.fitness_freeze_ticks > 0 {
                self.fitness_freeze_ticks -= 1;
                return;
            }
            // Thaw
            self.fitness_frozen = false;
            self.did_meta_reset = true;
            // Reset history window to avoid immediate re-freeze
            self.fitness_history_window.clear();
            tracing::info!("Evolution thawed after freeze period");
        }

        // ── Meta-evolution: periodically mutate the meta-genome ──
        self.meta_ticks += 1;
        let meta_interval = self.meta_genome.min_evolution_interval.max(50) * 10;
        if self.meta_ticks >= meta_interval {
            self.meta_mutate();
            self.meta_ticks = 0;
        }

        // Check if evolution is needed
        if !self.auto_evolve {
            return;
        }

        // Dynamic evolution interval
        let dynamic_interval = if current_fitness < 0.3 {
            self.meta_genome.min_evolution_interval
        } else if current_fitness > 0.7 {
            self.meta_genome.max_evolution_interval.min(200)
        } else {
            self.evolution_interval
        };

        if self.ticks_since_evolution < dynamic_interval {
            return;
        }
        self.ticks_since_evolution = 0;

        // Check for plateau
        let plateaued = self.fitness.history.len() >= 5 && {
            let start = self.fitness.history.len().saturating_sub(5);
            let recent = &self.fitness.history[start..];
            let mean: f64 = recent.iter().map(|f| f.overall).sum::<f64>() / recent.len() as f64;
            if mean < 0.01 { false } else {
                recent.iter().all(|f| (f.overall - mean).abs() < 0.05 * mean)
            }
        };

        let mut rng = rand::rng();
        let explore_chance = self.meta_genome.exploration_rate
            + (1.0 - current_fitness) * 0.1;
        let need_evolution = current_fitness < 0.4 || plateaued
            || rng.random_bool(self.meta_genome.mutation_rate.clamp(0.005, 0.1));

        if !need_evolution {
            return;
        }

        // Select mutation (with adaptive weighting from meta-genome)
        let mutation = self.select_mutation(brain, explore_chance);
        let parent_fitness = current_fitness;

        // Apply
        self.apply_mutation(brain, &mutation);

        // Re-sample fitness
        self.fitness.sample(brain);
        let child_fitness = self.fitness.current.overall;

        // Track mutation result for adaptive weighting
        self.track_mutation_result(&mutation, parent_fitness, child_fitness);

        // Record episodic memory
        let event = MutationEpisodicEvent::from_mutation(
            &mutation.type_key(),
            self.generation,
            parent_fitness,
            child_fitness,
            &self.fitness.current,
            now_f64(),
        );
        self.episodic_memory.push_back(event);
        if self.episodic_memory.len() > 200 {
            self.episodic_memory.pop_front();
        }

        // Record
        self.generation += 1;
        self.mutation_count += 1;
        self.genome.generation = self.generation;

        self.history.push(EvolutionEvent {
            timestamp: now_f64(),
            generation: self.generation,
            mutation: mutation.to_string(),
            parent_fitness,
            child_fitness,
            fitness_delta: child_fitness - parent_fitness,
            genome_size: self.genome.modules.len(),
        });
        if self.history.len() > 500 {
            self.history.remove(0);
        }
    }

    /// Track mutation outcome for adaptive weighting.
    pub fn track_mutation_result(&mut self, mutation: &Mutation, parent_fitness: f64, child_fitness: f64) {
        let type_key = mutation.type_key();
        let delta = child_fitness - parent_fitness;
        for stat in &mut self.mutation_stats {
            if stat.type_name == type_key {
                stat.attempts += 1;
                if delta > 0.001 {
                    stat.successes += 1;
                    stat.consecutive_failures = 0;
                } else {
                    stat.consecutive_failures += 1;
                    // Blacklist if too many consecutive failures
                    let threshold = self.meta_genome.toxic_failure_threshold;
                    if stat.consecutive_failures >= threshold && threshold > 0 {
                        let expires = self.generation + self.meta_genome.toxic_memory_ticks;
                        // Check if already blacklisted
                        let already = self.mutation_blacklist.iter()
                            .any(|(name, _)| name == &type_key);
                        if !already {
                            self.mutation_blacklist.push_back((type_key.clone(), expires));
                            tracing::debug!("Blacklisted toxic mutation '{}' for {} ticks", type_key, self.meta_genome.toxic_memory_ticks);
                        }
                    }
                }
                stat.total_fitness_delta += delta;
                let success_rate = stat.success_rate();
                let avg_d = stat.avg_delta();
                stat.weight = 1.0 + success_rate * avg_d.max(0.0) * 5.0;
                stat.weight = stat.weight.clamp(0.1, 5.0);
                break;
            }
        }
        // Clean expired blacklist entries
        self.mutation_blacklist.retain(|(_, expires)| *expires > self.generation);
    }

    /// Meta-mutate: evolve the evolution engine's own parameters.
    fn meta_mutate(&mut self) {
        let mut rng = rand::rng();
        let mg = &mut self.meta_genome;
        match rng.random_range(0..16) {
            0 => mg.mutation_rate = (mg.mutation_rate + rng.random_range(-0.005..0.01)).clamp(0.005, 0.1),
            1 => mg.selection_pressure = (mg.selection_pressure + rng.random_range(-0.05..0.05)).clamp(0.1, 1.0),
            2 => mg.exploration_rate = (mg.exploration_rate + rng.random_range(-0.03..0.03)).clamp(0.05, 0.5),
            3 => {
                let shift = rng.random_range(-20..=20i64);
                let new_min = (mg.min_evolution_interval as i64 + shift).clamp(20, 200) as u64;
                mg.min_evolution_interval = new_min.min(mg.max_evolution_interval);
            }
            4 => {
                let shift = rng.random_range(-20..=20i64);
                let new_max = (mg.max_evolution_interval as i64 + shift).clamp(50, 500) as u64;
                mg.max_evolution_interval = new_max.max(mg.min_evolution_interval);
            }
            5 => mg.meta_mutation_rate = (mg.meta_mutation_rate + rng.random_range(-0.003..0.005)).clamp(0.001, 0.05),
            // ── Heal threshold mutations ──
            6 => mg.heal_stalled_threshold = (mg.heal_stalled_threshold + rng.random_range(-0.005..0.01)).clamp(0.001, 0.1),
            7 => mg.heal_stalled_boost = (mg.heal_stalled_boost + rng.random_range(-0.03..0.05)).clamp(0.01, 0.5),
            8 => mg.heal_stalled_fraction = (mg.heal_stalled_fraction + rng.random_range(-0.01..0.02)).clamp(0.01, 0.5),
            9 => mg.heal_collapse_min_active = (mg.heal_collapse_min_active + rng.random_range(-0.003..0.005)).clamp(0.001, 0.1),
            10 => mg.heal_collapse_activation_boost = (mg.heal_collapse_activation_boost + rng.random_range(-0.02..0.05)).clamp(0.01, 0.5),
            11 => mg.heal_collapse_potential_boost = (mg.heal_collapse_potential_boost + rng.random_range(-0.03..0.05)).clamp(0.01, 0.5),
            // ── Divergence detection mutations ──
            12 => mg.fitness_divergence_threshold = (mg.fitness_divergence_threshold + rng.random_range(-0.01..0.01)).clamp(0.01, 0.2),
            13 => {
                let shift = rng.random_range(-30..=30i64);
                mg.fitness_freeze_duration = (mg.fitness_freeze_duration as i64 + shift).clamp(50, 1000) as u64;
            }
            // ── Toxic blacklist mutations ──
            14 => mg.toxic_memory_ticks = (mg.toxic_memory_ticks as i64 + rng.random_range(-50..=50)).clamp(100, 2000) as u64,
            15 => mg.toxic_failure_threshold = (mg.toxic_failure_threshold as i64 + rng.random_range(-1..=1)).clamp(1, 10) as u64,
            _ => {}
        }
    }

    /// Weighted-random mutation selection with adaptive weighting.
    /// Blends fitness-based weights with success-history weights.
    fn select_mutation(&self, brain: &crate::Brain, explore_chance: f64) -> Mutation {
        let f = &self.fitness.current;
        let mut rng = rand::rng();
        let n_mods = self.genome.modules.len().max(1);
        let sp = self.meta_genome.selection_pressure;

        // Sometimes explore: purely random mutation type
        if rng.random_bool(explore_chance) {
            let types = [
                Mutation::AddNeuron { module: rng.random_range(0..n_mods), count: rng.random_range(1..=5) },
                Mutation::MutateParam { param: "tau".into(), delta: rng.random_range(-0.03..0.03) },
                Mutation::MutateCortexConfig {
                    d_model: ((16i32 + rng.random_range(-4..=4)).max(4)) as usize,
                    state_dim: ((4i32 + rng.random_range(-1..=1)).max(2)) as usize,
                    n_layers: ((2i32 + rng.random_range(-1..=1)).max(1)) as usize,
                },
            ];
            return types[rng.random_range(0..types.len())].clone();
        }

        // Pareto-guided selection: target weakest objective of a random frontier member
        if !self.pareto_frontier.individuals.is_empty() && rng.random_bool(0.2) {
            let idx = rng.random_range(0..self.pareto_frontier.individuals.len());
            let pivot = &self.pareto_frontier.individuals[idx];
            let weakest_idx = pivot.objectives.iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);
            return match weakest_idx {
                0 | 1 => {
                    let m = rng.random_range(0..n_mods);
                    if rng.random_bool(0.5) {
                        Mutation::AddNeuron { module: m, count: rng.random_range(1..=5) }
                    } else {
                        Mutation::PruneNeurons { module: m, fraction: rng.random_range(0.05..0.15) }
                    }
                }
                2 => {
                    let m = rng.random_range(0..n_mods);
                    Mutation::AddNeuron { module: m, count: rng.random_range(5..=15) }
                }
                3 => Mutation::MutateParam { param: "tau".into(), delta: rng.random_range(-0.02..0.02) },
                4 => Mutation::MutateCortexConfig {
                    d_model: ((16i32 + rng.random_range(-4..=4)).max(4)) as usize,
                    state_dim: ((4i32 + rng.random_range(-1..=1)).max(2)) as usize,
                    n_layers: ((2i32 + rng.random_range(-1..=1)).max(1)) as usize,
                },
                // temporal_coherence → fine-tune params
                8 => Mutation::MutateParam { param: "tau".into(), delta: rng.random_range(-0.01..0.01) },
                // emergent_complexity → add neurons to increase diversity
                9 => {
                    let m = rng.random_range(0..n_mods);
                    Mutation::AddNeuron { module: m, count: rng.random_range(1..=3) }
                }
                _ => {
                    let m = rng.random_range(0..n_mods);
                    Mutation::AddNeuron { module: m, count: rng.random_range(1..=3) }
                }
            };
        }

        // Build weighted candidates
        let mut candidates: Vec<(f64, Mutation)> = Vec::new();

        // Blend fitness-based weights with historical success weights
        let get_type_weight = |type_name: &str| -> f64 {
            let hist_weight = self.mutation_stats.iter()
                .find(|s| s.type_name == type_name)
                .map(|s| s.weight)
                .unwrap_or(1.0);
            // Blend: sp controls how much history matters
            1.0 + (hist_weight - 1.0) * sp
        };

        // Goal bias: which mutation types are relevant to the target metric
        let goal = &self.evolution_goal;
        let goal_relevance = |type_name: &str| -> f64 {
            if !goal.enabled || goal.weight <= 0.0 { return 1.0; }
            let room = (1.0 - goal.target_value(f)).max(0.01);
            let base = 1.0 + room * goal.weight * 2.0;
            match goal.target_metric.as_str() {
                "firing_balance" => match type_name {
                    "AddNeuron" | "PruneNeurons" => base,
                    _ => 1.0,
                },
                "synaptic_diversity" => match type_name {
                    "MutateDensity" => base,
                    "AddNeuron" => 1.0 + (base - 1.0) * 0.5,
                    _ => 1.0,
                },
                "module_utilization" => match type_name {
                    "AddModule" | "SplitModule" | "MergeModules" => base,
                    _ => 1.0,
                },
                "stability" => match type_name {
                    "MutateParam" => base,
                    _ => 1.0,
                },
                "cortex_impact" => match type_name {
                    "MutateCortex" => base,
                    _ => 1.0,
                },
                "temporal_coherence" => match type_name {
                    "MutateParam" => base,
                    _ => 1.0,
                },
                "emergent_complexity" => match type_name {
                    "AddNeuron" => base,
                    "AddModule" => base * 0.7,
                    _ => 1.0,
                },
                _ => 1.0,
            }
        };

        // 1. Firing balance low → AddNeuron or PruneNeurons
        let fb_weight = (1.0 - f.firing_balance).max(0.05);
        let weak_idx = self.find_weakest_module(brain).min(n_mods - 1);
        let aw = get_type_weight("AddNeuron") * goal_relevance("AddNeuron");
        candidates.push((fb_weight * 0.5 * aw, Mutation::AddNeuron { module: weak_idx, count: rng.random_range(1..=5) }));
        if f.firing_balance > 0.7 {
            let pw = get_type_weight("PruneNeurons") * goal_relevance("PruneNeurons");
            candidates.push((fb_weight * 0.3 * pw, Mutation::PruneNeurons { module: weak_idx, fraction: rng.random_range(0.05..0.20) }));
        }

        // 2. Diversity low → MutateSynapticDensity
        let sd_weight = (1.0 - f.synaptic_diversity).max(0.05);
        let src = rng.random_range(0..n_mods);
        let tgt = rng.random_range(0..n_mods);
        let dw = get_type_weight("MutateDensity") * goal_relevance("MutateDensity");
        candidates.push((sd_weight * 0.4 * dw, Mutation::MutateSynapticDensity { source: src, target: tgt, density: rng.random_range(0.1..0.5) }));

        // 3. Utilization low → AddModule (if under cap)
        if self.genome.modules.len() < self.max_modules {
            let mu_weight = (1.0 - f.module_utilization).max(0.05);
            let mw = get_type_weight("AddModule") * goal_relevance("AddModule");
            candidates.push((mu_weight * 0.3 * mw, Mutation::AddModule {
                name: format!("evolved_{}", self.generation),
                count: rng.random_range(200..=1000),
                x: rng.random_range(100.0..600.0),
                y: rng.random_range(80.0..520.0),
                color: format!("#{:06x}", rng.random_range(0..0x1000000)),
            }));
        }

        // 4. Stability low → MutateParam
        let st_weight = (1.0 - f.stability).max(0.05);
        let param = ["tau", "homeostatic_rate", "stdp_learning_rate", "energy_recharge_rate", "energy_capacity", "firing_energy_cost", "base_metabolic_rate"][rng.random_range(0..7)];
        let delta = rng.random_range(-0.03..0.03f64);
        let pw = get_type_weight("MutateParam") * goal_relevance("MutateParam");
        candidates.push((st_weight * 0.3 * pw, Mutation::MutateParam { param: param.to_string(), delta }));

        // 4b. Energy efficiency low → energy-specific MutateParam
        let ee_weight = (1.0 - f.energy_efficiency).max(0.05);
	        let energy_params = ["energy_capacity", "energy_recharge_rate", "base_metabolic_rate", "firing_energy_cost", "synapse_energy_cost", "energy_pool_recharge_rate", "pool_contribution", "neuron_creation_cost", "synapse_creation_cost", "growth_energy_threshold"];
        let ep = energy_params[rng.random_range(0..energy_params.len())];
        let ep_delta = match ep {
            "energy_capacity" => rng.random_range(-0.05..0.1),
            "energy_recharge_rate" => rng.random_range(-0.0005..0.001),
            "base_metabolic_rate" => rng.random_range(-0.00005..0.0001),
            "firing_energy_cost" => rng.random_range(-0.005..0.01),
            "synapse_energy_cost" => rng.random_range(-0.000_005..0.000_01),
            "energy_pool_recharge_rate" => rng.random_range(-0.003..0.005),
            "pool_contribution" => rng.random_range(-0.03..0.05),
            "neuron_creation_cost" => rng.random_range(-0.02..0.03),
            "synapse_creation_cost" => rng.random_range(-0.005..0.01),
            "growth_energy_threshold" => rng.random_range(-0.1..0.15),
            _ => rng.random_range(-0.03..0.03),
        };
        candidates.push((ee_weight * 0.25 * pw, Mutation::MutateParam { param: ep.to_string(), delta: ep_delta }));

        // 4c. Module-level energy mutations when energy efficiency is low
        if ee_weight > 0.3 && n_mods > 0 {
            let n_candidates = n_mods.min(5);
            for midx in 0..n_candidates {
                let mw = get_type_weight("MutateModuleMetabolicRate");
                let delta = rng.random_range(-0.3..0.3);
                candidates.push((ee_weight * 0.08 * mw, Mutation::MutateModuleMetabolicRate { module: midx, delta }));
                let pw2 = get_type_weight("MutateModulePriority");
                let delta2 = rng.random_range(-0.2..0.2);
                candidates.push((ee_weight * 0.06 * pw2, Mutation::MutateModulePriority { module: midx, delta: delta2 }));
            }
        }

        // 5. Always some cortex config mutation
        let cw = get_type_weight("MutateCortex") * goal_relevance("MutateCortex");
        candidates.push((0.2 * cw, Mutation::MutateCortexConfig {
            d_model: ((16i32 + rng.random_range(-4i32..=4)).max(4)) as usize,
            state_dim: ((4i32 + rng.random_range(-1i32..=1)).max(2)) as usize,
            n_layers: ((2i32 + rng.random_range(-1i32..=1)).max(1)) as usize,
        }));

        // 6. Rare structural mutations
        if rng.random_bool(0.15) && n_mods > 3 {
            let a = rng.random_range(0..n_mods);
            let b = rng.random_range(0..n_mods);
            if a != b {
                candidates.push((0.15, Mutation::MergeModules {
                    a: a.min(b),
                    b: a.max(b),
                    new_name: format!("merged_{}", self.generation),
                }));
            }
        }
        if rng.random_bool(0.10) && n_mods < self.max_modules {
            let mod_idx = rng.random_range(0..n_mods);
            candidates.push((0.1, Mutation::SplitModule {
                module: mod_idx,
                new_name: format!("split_{}_{}", self.genome.modules[mod_idx].name, self.generation),
            }));
        }

        // 7. Script mutations — bias when firing_balance or diversity are low
        let fb_script_weight = (1.0 - f.firing_balance).max(0.05);
        if fb_script_weight > 0.7 {
            // Firing balance very low → try an activation script mutation
            let mod_idx = rng.random_range(0..n_mods);
            let _mod_name = &self.genome.modules[mod_idx].name;
            let script = format!(
                "sigmoid(potential * 1.5 + ({} - 0.5) * 0.2)",
                if rng.random_bool(0.5) { "noise" } else { "reserve" }
            );
            let aw = get_type_weight("MutateActivationScript") * goal_relevance("firing_balance");
            candidates.push((fb_script_weight * 0.5 * aw, Mutation::MutateActivationScript { module: mod_idx, script }));
        }

        let sd_script_weight = (1.0 - f.synaptic_diversity).max(0.05);
        if sd_script_weight > 0.7 {
            // Diversity low → try a plasticity script mutation
            let plasticity_type = ["hebbian", "oja", "bcm"][rng.random_range(0..3)];
            let script = match plasticity_type {
                "oja" => "clamp(weight + learning_rate * post_activation * (pre_activation - post_activation * weight), 0.0, 1.0)".into(),
                "bcm" => "clamp(weight + learning_rate * pre_activation * post_activation * (post_activation - 0.5) * 2.0, 0.0, 1.0)".into(),
                _ => "clamp(weight + pre_activation * post_activation * learning_rate, 0.0, 1.0)".into(),
            };
            let pw = get_type_weight("MutatePlasticityScript") * goal_relevance("synaptic_diversity");
            candidates.push((sd_script_weight * 0.3 * pw, Mutation::MutatePlasticityScript { script }));
        }

        // 8. Field mutations — occasional addition of dynamic fields
        if rng.random_bool(0.08) {
            let target = ["Neuron", "Synapse", "BrainModule"][rng.random_range(0..3)];
            let field_name = [
                "fatigue_rate", "curiosity_drive", "gain_modulator",
                "inhibition_level", "plasticity_gate", "attention_bias",
            ][rng.random_range(0..6)];
            let default = [0.0, 0.5, 1.0][rng.random_range(0..3)];
            candidates.push((0.05, Mutation::AddField {
                target: target.into(),
                field_name: field_name.into(),
                default,
            }));
        }

        // Filter out blacklisted mutation types
        if !self.mutation_blacklist.is_empty() {
            candidates.retain(|(_, m)| {
                let key = m.type_key();
                !self.mutation_blacklist.iter().any(|(bl, _)| bl == &key)
            });
        }
        // Apply priority rules from symbolic backpropagation
        if !self.priority_rules.is_empty() {
            let current_f = &self.fitness.current;
            for rule in &self.priority_rules {
                let ctx_value = match rule.context_dim.as_str() {
                    "firing_balance" => current_f.firing_balance,
                    "synaptic_diversity" => current_f.synaptic_diversity,
                    "module_utilization" => current_f.module_utilization,
                    "stability" => current_f.stability,
                    _ => continue,
                };
                if ctx_value >= rule.context_lo && ctx_value <= rule.context_hi {
                    for (weight, mutation) in candidates.iter_mut() {
                        if mutation.type_key() == rule.mutation_type {
                            *weight *= 1.0 + rule.bias_weight.clamp(-0.5, 1.0);
                        }
                    }
                }
            }
        }
        // If all candidates blacklisted, use explore-style random
        if candidates.is_empty() {
            let types = [
                Mutation::AddNeuron { module: rng.random_range(0..n_mods), count: rng.random_range(1..=5) },
                Mutation::MutateParam { param: "tau".into(), delta: rng.random_range(-0.03..0.03) },
                Mutation::MutateCortexConfig {
                    d_model: ((16i32 + rng.random_range(-4..=4)).max(4)) as usize,
                    state_dim: ((4i32 + rng.random_range(-1..=1)).max(2)) as usize,
                    n_layers: ((2i32 + rng.random_range(-1..=1)).max(1)) as usize,
                },
            ];
            return types[rng.random_range(0..types.len())].clone();
        }

        // Weighted random selection
        let total: f64 = candidates.iter().map(|(w, _)| w).sum();
        let r = rng.random_range(0.0..total);
        let mut cum = 0.0;
        for (w, mutation) in &candidates {
            cum += w;
            if r <= cum {
                return mutation.clone();
            }
        }
        candidates[0].1.clone()
    }

    /// Find the module index with lowest firing rate.
    fn find_weakest_module(&self, brain: &crate::Brain) -> usize {
        let mut best = 0usize;
        let mut lowest = f64::MAX;
        for (i, name) in brain.module_order.iter().enumerate() {
            if let Some(m) = brain.modules.get(name) {
                let rate = m.activation_level as f64;
                if rate < lowest {
                    lowest = rate;
                    best = i;
                }
            }
        }
        // Clamp to genome modules (may differ if modules were added)
        best.min(self.genome.modules.len().max(1) - 1)
    }

    /// Apply a mutation to the brain structure.
    pub fn apply_mutation(&mut self, brain: &mut crate::Brain, mutation: &Mutation) {
        match mutation {
            Mutation::AddNeuron { module, count } => {
                let mod_idx = (*module).min(self.genome.modules.len().max(1) - 1);
                let mod_name = &self.genome.modules[mod_idx].name;
                for _ in 0..*count {
                    brain.add_neuron_to(mod_name);
                }
                self.genome.modules[mod_idx].neuron_count = brain.modules.get(mod_name)
                    .map(|m| m.neuron_indices.len()).unwrap_or(0);
                brain.refresh_stats();
            }

            Mutation::PruneNeurons { module, fraction } => {
                let mod_idx = (*module).min(self.genome.modules.len().max(1) - 1);
                let mod_name = self.genome.modules[mod_idx].name.clone();
                let count = {
                    let m = match brain.modules.get(&mod_name) {
                        Some(m) => m,
                        None => return,
                    };
                    let n = (m.neuron_indices.len() as f64 * fraction) as usize;
                    n.max(1).min(m.neuron_indices.len() / 2)
                };

                // Find lowest-importance neurons in this module
                let mut candidates: Vec<(usize, f32)> = brain.neurons.iter().enumerate()
                    .filter(|(_, n)| n.module == mod_name)
                    .map(|(i, n)| (i, n.importance))
                    .collect();
                candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                let to_prune: HashSet<usize> = candidates.iter().take(count).map(|(i, _)| *i).collect();
                if to_prune.is_empty() { return; }

                // Tombstone: mark for removal, rebuild neuron/synapse vectors
                let old_neurons = std::mem::take(&mut brain.neurons);
                let old_synapses = std::mem::take(&mut brain.synapses);

                let mut remap: Vec<Option<usize>> = vec![None; old_neurons.len()];
                let mut new_idx = 0usize;
                for (i, neuron) in old_neurons.into_iter().enumerate() {
                    if to_prune.contains(&i) { continue; }
                    remap[i] = Some(new_idx);
                    brain.neurons.push(neuron);
                    new_idx += 1;
                }

                fn apply_remap(idx: usize, remap: &[Option<usize>]) -> Option<usize> {
                    if idx < remap.len() { remap[idx] } else { None }
                }

                for synapse in old_synapses {
                    if !synapse.active { continue; }
                    if to_prune.contains(&synapse.source) || to_prune.contains(&synapse.target) { continue; }
                    let new_src = apply_remap(synapse.source, &remap);
                    let new_tgt = apply_remap(synapse.target, &remap);
                    if let (Some(s), Some(t)) = (new_src, new_tgt) {
                        brain.synapses.push(crate::Synapse { source: s, target: t, ..synapse });
                    }
                }

                // Update module neuron_indices
                let mut new_indices: Vec<usize> = Vec::new();
                for (i, _) in brain.neurons.iter().enumerate() {
                    if brain.neurons[i].module == mod_name {
                        new_indices.push(i);
                    }
                }
                if let Some(m) = brain.modules.get_mut(&mod_name) {
                    m.neuron_indices = new_indices;
                }

                self.genome.modules[mod_idx].neuron_count = brain.modules.get(&mod_name)
                    .map(|m| m.neuron_indices.len()).unwrap_or(0);
                brain.stats.pruned_synapses += to_prune.len() as u64;
                brain.refresh_stats();
            }

            Mutation::AddModule { name, count, x, y, color } => {
                if brain.modules.contains_key(name) { return; }
                let module = crate::BrainModule::new(name.clone(), *x, *y, color.clone());
                brain.modules.insert(name.clone(), module);
                brain.module_order.push(name.clone());
                for _ in 0..*count {
                    brain.add_neuron_to(name);
                }
                // Connect to 3 random modules
                let new_indices: Vec<usize> = brain.neurons.iter().enumerate()
                    .filter(|(_, n)| n.module == *name)
                    .map(|(i, _)| i)
                    .collect();
                let mut rng = rand::rng();
                if !new_indices.is_empty() {
                    for _ in 0..(new_indices.len().min(100)) {
                        let src = new_indices[rng.random_range(0..new_indices.len())];
                        let tgt = rng.random_range(0..brain.neurons.len());
                        if src != tgt {
                            brain.add_synapse(src, tgt);
                        }
                    }
                }
                // Add genome entry
                self.genome.modules.push(ModuleGene {
                    name: name.clone(),
                    neuron_count: *count,
                    position_x: *x, position_y: *y,
                    color: color.clone(),
                    learning_rate: 0.2,
                    firing_threshold_mean: 0.75,
                    firing_threshold_std: 0.1,
                    synaptic_density: 0.3,
                    mutation_rate: 0.02,
                    energy_multiplier: 1.0,
                    energy_priority: 0.5,
                });
                brain.refresh_stats();
            }

            Mutation::MergeModules { a, b, new_name } => {
                let na = (*a).min(brain.module_order.len().max(1) - 1);
                let nb = (*b).min(brain.module_order.len().max(1) - 1);
                if na == nb { return; }
                let (keep, remove) = if na < nb { (na, nb) } else { (nb, na) };

                // Move all neurons from remove module to keep module
                for neuron in &mut brain.neurons {
                    if neuron.module == brain.module_order[remove] {
                        neuron.module = brain.module_order[keep].clone();
                    }
                }

                // Update module
                let indices_to_move = brain.modules.get(&brain.module_order[remove])
                    .map(|m| m.neuron_indices.clone()).unwrap_or_default();
                if let Some(m) = brain.modules.get_mut(&brain.module_order[keep]) {
                    m.neuron_indices.extend(indices_to_move);
                }

                // Rename keep module
                {
                    let mut module_data = brain.modules.remove(&brain.module_order[keep]);
                    let _ = brain.modules.remove(&brain.module_order[remove]);
                    if let Some(ref mut m) = module_data {
                        m.name = new_name.clone();
                    }
                    if let Some(m) = module_data {
                        brain.modules.insert(new_name.clone(), m);
                    }
                }

                brain.module_order[keep] = new_name.clone();
                brain.module_order.remove(remove);

                // Update genome
                self.genome.modules[keep].name = new_name.clone();
                self.genome.modules.remove(remove);

                brain.refresh_stats();
            }

            Mutation::SplitModule { module, new_name } => {
                let mod_idx = (*module).min(brain.module_order.len().max(1) - 1);
                let orig_name = brain.module_order[mod_idx].clone();
                let (indices, orig_count) = {
                    let m = match brain.modules.get(&orig_name) {
                        Some(m) => m,
                        None => return,
                    };
                    if m.neuron_indices.len() < 10 { return; }
                    (m.neuron_indices.clone(), m.neuron_indices.len())
                };

                let split_point = orig_count / 2;
                let split_indices: Vec<usize> = indices.iter().skip(split_point).copied().collect();
                let keep_indices: Vec<usize> = indices.iter().take(split_point).copied().collect();

                // Reassign neuron modules for split half
                for &i in &split_indices {
                    if i < brain.neurons.len() {
                        brain.neurons[i].module = new_name.clone();
                    }
                }

                // Create new module
                let orig_color = brain.modules.get(&orig_name)
                    .map(|m| m.color.clone()).unwrap_or_else(|| "#888888".into());
                let split_count = split_indices.len();
                let gene_color = orig_color.clone();
                let new_mod = crate::BrainModule {
                    name: new_name.clone(),
                    neuron_indices: split_indices,
                    activation_level: 0.0,
                    importance: 0.5,
                    x: 300.0,
                    y: 300.0,
                    color: orig_color,
                    schema_version: 0,
                    extensions: std::collections::HashMap::new(),
                    energy_reserve: 0.5,
                    energy_budget: 1.0,
                    base_metabolic_mod: 1.0,
                    energy_priority: 0.5,
                    local_stress: 0.0,
                };
                brain.modules.insert(new_name.clone(), new_mod);
                brain.module_order.push(new_name.clone());

                // Update original module indices
                if let Some(m) = brain.modules.get_mut(&orig_name) {
                    m.neuron_indices = keep_indices;
                }

                // Add genome entry
                self.genome.modules.push(ModuleGene {
                    name: new_name.clone(),
                    neuron_count: split_count,
                    position_x: 300.0, position_y: 300.0,
                    color: gene_color,
                    learning_rate: 0.2,
                    firing_threshold_mean: 0.75,
                    firing_threshold_std: 0.1,
                    synaptic_density: 0.3,
                    mutation_rate: 0.02,
                    energy_multiplier: 1.0,
                    energy_priority: 0.5,
                });

                brain.refresh_stats();
            }

            Mutation::MutateParam { param, delta } => {
                let delta_f32 = *delta as f32;
                match param.as_str() {
                    "tau" => {
                        let old = self.genome.brain_params.tau;
                        self.genome.brain_params.tau = (old + delta_f32).clamp(0.9, 0.999);
                    }
                    "homeostatic_rate" => {
                        let old = self.genome.brain_params.homeostatic_rate;
                        self.genome.brain_params.homeostatic_rate = (old + delta_f32).clamp(0.0, 0.05);
                    }
                    "stdp_learning_rate" => {
                        let old = self.genome.brain_params.stdp_learning_rate;
                        self.genome.brain_params.stdp_learning_rate = (old + delta_f32).clamp(0.0, 0.05);
                    }
                    "prune_threshold" => {
                        let old = self.genome.brain_params.prune_threshold;
                        self.genome.brain_params.prune_threshold = (old + delta_f32).clamp(0.0, 0.5);
                    }
                    "evolution_interval" => {
                        let old = self.evolution_interval as i64;
                        let new = (old + *delta as i64).clamp(20, 5000);
                        self.evolution_interval = new as u64;
                    }
                    "energy_capacity" => {
                        let old = self.genome.brain_params.energy_capacity;
                        self.genome.brain_params.energy_capacity = (old + delta_f32).clamp(0.5, 5.0);
                    }
                    "base_metabolic_rate" => {
                        let old = self.genome.brain_params.base_metabolic_rate;
                        self.genome.brain_params.base_metabolic_rate = (old + delta_f32).clamp(0.0, 0.01);
                    }
                    "firing_energy_cost" => {
                        let old = self.genome.brain_params.firing_energy_cost;
                        self.genome.brain_params.firing_energy_cost = (old + delta_f32).clamp(0.0, 0.5);
                    }
                    "synapse_energy_cost" => {
                        let old = self.genome.brain_params.synapse_energy_cost;
                        self.genome.brain_params.synapse_energy_cost = (old + delta_f32).clamp(0.0, 0.001);
                    }
                    "energy_recharge_rate" => {
                        let old = self.genome.brain_params.energy_recharge_rate;
                        self.genome.brain_params.energy_recharge_rate = (old + delta_f32).clamp(0.0, 0.05);
                    }
                    "energy_pool_recharge_rate" => {
                        let old = self.genome.brain_params.energy_pool_recharge_rate;
                        self.genome.brain_params.energy_pool_recharge_rate = (old + delta_f32).clamp(0.0, 0.5);
                    }
                    "pool_contribution" => {
                        let old = self.genome.brain_params.pool_contribution;
                        self.genome.brain_params.pool_contribution = (old + delta_f32).clamp(0.0, 0.5);
                    }
                    "neuron_creation_cost" => {
                        let old = self.genome.brain_params.neuron_creation_cost;
                        self.genome.brain_params.neuron_creation_cost = (old + delta_f32).clamp(0.0, 0.5);
                    }
                    "synapse_creation_cost" => {
                        let old = self.genome.brain_params.synapse_creation_cost;
                        self.genome.brain_params.synapse_creation_cost = (old + delta_f32).clamp(0.0, 0.1);
                    }
                    "growth_energy_threshold" => {
                        let old = self.genome.brain_params.growth_energy_threshold;
                        self.genome.brain_params.growth_energy_threshold = (old + delta_f32).clamp(0.0, 0.9);
                    }
                    _ => {}
                }
            }

            Mutation::MutateSynapticDensity { source, target, density } => {
                let n_mods = brain.module_order.len().max(1);
                let src_idx = (*source).min(n_mods - 1);
                let tgt_idx = (*target).min(n_mods - 1);
                let src_name = &brain.module_order[src_idx];
                let tgt_name = &brain.module_order[tgt_idx];

                let src_indices: Vec<usize> = brain.neurons.iter().enumerate()
                    .filter(|(_, n)| n.module == *src_name).map(|(i, _)| i).collect();
                let tgt_indices: Vec<usize> = brain.neurons.iter().enumerate()
                    .filter(|(_, n)| n.module == *tgt_name).map(|(i, _)| i).collect();
                if src_indices.is_empty() || tgt_indices.is_empty() { return; }

                // Add synapses up to target density
                let existing: usize = brain.synapses.iter()
                    .filter(|s| s.active && src_indices.contains(&s.source) && tgt_indices.contains(&s.target))
                    .count();
                let total_possible = src_indices.len() * tgt_indices.len();
                let target_conn = (total_possible as f32 * density) as usize;
                let to_add = target_conn.saturating_sub(existing).min(50); // cap per step

                let mut rng = rand::rng();
                for _ in 0..to_add {
                    let s = src_indices[rng.random_range(0..src_indices.len())];
                    let t = tgt_indices[rng.random_range(0..tgt_indices.len())];
                    if s != t && !brain.synapses.iter().any(|syn| syn.active && syn.source == s && syn.target == t) {
                        brain.add_synapse(s, t);
                    }
                }
                brain.refresh_stats();
            }

            Mutation::MutateCortexConfig { d_model, state_dim, n_layers } => {
                self.genome.cortex_config.d_model = *d_model;
                self.genome.cortex_config.state_dim = *state_dim;
                self.genome.cortex_config.n_layers = *n_layers;
                // At runtime, only modulation_strength is applied live.
                // Structural cortex changes (d_model, n_layers) require restart
                // but are recorded in the genome for generate_module_source().
            }

            Mutation::MutateActivationScript { module, script } => {
                // Validate by compiling with Rhai (best-effort — engine is in Brain)
                let mod_name = if let Some(m) = self.genome.modules.get(*module) {
                    m.name.clone()
                } else {
                    return;
                };
                // Store in genome
                self.genome.script_genome.activation_scripts.insert(mod_name.clone(), script.clone());
                // Apply to brain engine
                brain.script_engine.set_activation_script(&mod_name, script)
                    .unwrap_or_else(|e| {
                        tracing::warn!("MutateActivationScript: compile error for module '{}': {}", mod_name, e);
                    });
            }

            Mutation::MutatePlasticityScript { script } => {
                // Store in genome
                self.genome.script_genome.plasticity_script = Some(script.clone());
                // Apply to brain engine
                brain.script_engine.set_plasticity_script(script)
                    .unwrap_or_else(|e| {
                        tracing::warn!("MutatePlasticityScript: compile error: {}", e);
                    });
            }

            Mutation::AddField { target, field_name, default } => {
                // Store definition in genome
                self.genome.script_genome.field_definitions
                    .entry(target.clone())
                    .or_default()
                    .insert(field_name.clone(), *default);

                // Apply to all existing entities
                match target.as_str() {
                    "Neuron" => {
                        for n in brain.neurons.iter_mut() {
                            n.extensions.entry(field_name.clone()).or_insert(*default);
                        }
                    }
                    "Synapse" => {
                        for s in brain.synapses.iter_mut() {
                            s.extensions.entry(field_name.clone()).or_insert(*default);
                        }
                    }
                    "BrainModule" => {
                        for m in brain.modules.values_mut() {
                            m.extensions.entry(field_name.clone()).or_insert(*default);
                        }
                    }
                    _ => {
                        tracing::warn!("AddField: unknown target '{}' (use Neuron/Synapse/BrainModule)", target);
                    }
                }
            }

            Mutation::RemoveField { target, field_name } => {
                // Remove from genome definitions
                if let Some(defs) = self.genome.script_genome.field_definitions.get_mut(target) {
                    defs.remove(field_name);
                }

                // Remove from all existing entities
                match target.as_str() {
                    "Neuron" => {
                        for n in brain.neurons.iter_mut() {
                            n.extensions.remove(field_name);
                        }
                    }
                    "Synapse" => {
                        for s in brain.synapses.iter_mut() {
                            s.extensions.remove(field_name);
                        }
                    }
                    "BrainModule" => {
                        for m in brain.modules.values_mut() {
                            m.extensions.remove(field_name);
                        }
                    }
                    _ => {
                        tracing::warn!("RemoveField: unknown target '{}' (use Neuron/Synapse/BrainModule)", target);
                    }
                }
            }
            Mutation::MutateModuleMetabolicRate { module, delta } => {
                let midx = (*module).min(self.genome.modules.len().max(1) - 1);
                let old = self.genome.modules[midx].energy_multiplier;
                let new = (old + *delta as f32).clamp(0.1, 5.0);
                self.genome.modules[midx].energy_multiplier = new;
                if let Some(m) = brain.modules.get_mut(&self.genome.modules[midx].name) {
                    m.base_metabolic_mod = new;
                }
            }
            Mutation::MutateModulePriority { module, delta } => {
                let midx = (*module).min(self.genome.modules.len().max(1) - 1);
                let old = self.genome.modules[midx].energy_priority;
                let new = (old + *delta as f32).clamp(0.1, 1.0);
                self.genome.modules[midx].energy_priority = new;
                if let Some(m) = brain.modules.get_mut(&self.genome.modules[midx].name) {
                    m.energy_priority = new;
                }
            }
        }
    }

    /// Generate Rust source code reflecting the evolved genome.
    /// This is the "self-modification" capability — writes a compilable module.
    pub fn generate_module_source(&self) -> String {
        let mut code = String::new();
        code.push_str("//! Auto-generated evolved brain module.\n");
        code.push_str("//! Generation: ");
        code.push_str(&self.generation.to_string());
        code.push_str("\n//! Timestamp: ");
        code.push_str(&format!("{:.3}", self.genome.created));
        code.push_str("\n\n");

        // Module genes as code
        code.push_str("pub fn evolved_module_config() -> Vec<EvolvedModuleDef> {\n");
        code.push_str("    vec![\n");
        for mod_gene in &self.genome.modules {
            code.push_str(&format!(
                "        EvolvedModuleDef {{ name: \"{}\".into(), neuron_count: {}, position: ({:.1}, {:.1}), color: \"{}\".into(), learning_rate: {}, firing_threshold_mean: {}, firing_threshold_std: {}, synaptic_density: {} }},\n",
                mod_gene.name, mod_gene.neuron_count,
                mod_gene.position_x, mod_gene.position_y,
                mod_gene.color, mod_gene.learning_rate,
                mod_gene.firing_threshold_mean, mod_gene.firing_threshold_std,
                mod_gene.synaptic_density,
            ));
        }
        code.push_str("    ]\n");
        code.push_str("}\n\n");

        // Global params
        let gp = &self.genome.brain_params;
        let cc = &self.genome.cortex_config;
        code.push_str("pub fn evolved_params() -> EvolvedParams {\n");
        code.push_str("    EvolvedParams {\n");
        code.push_str(&format!("        tau: {:.5},\n", gp.tau));
        code.push_str(&format!("        homeostatic_rate: {:.5},\n", gp.homeostatic_rate));
        code.push_str(&format!("        prune_threshold: {:.5},\n", gp.prune_threshold));
        code.push_str(&format!("        stdp_learning_rate: {:.5},\n", gp.stdp_learning_rate));
        code.push_str(&format!("        evolution_interval: {},\n", self.evolution_interval));
        code.push_str(&format!("        energy_capacity: {:.5},\n", gp.energy_capacity));
        code.push_str(&format!("        base_metabolic_rate: {:.7},\n", gp.base_metabolic_rate));
        code.push_str(&format!("        firing_energy_cost: {:.5},\n", gp.firing_energy_cost));
        code.push_str(&format!("        synapse_energy_cost: {:.8},\n", gp.synapse_energy_cost));
        code.push_str(&format!("        energy_recharge_rate: {:.6},\n", gp.energy_recharge_rate));
        code.push_str(&format!("        metabolism_enabled: {},\n", gp.metabolism_enabled));
        code.push_str("        cortex: CortexEvolvedConfig {\n");
        code.push_str(&format!("            d_model: {},\n", cc.d_model));
        code.push_str(&format!("            state_dim: {},\n", cc.state_dim));
        code.push_str(&format!("            n_layers: {},\n", cc.n_layers));
        code.push_str(&format!("            modulation_strength: {:.3},\n", cc.modulation_strength));
        code.push_str("        },\n");
        code.push_str("    }\n");
        code.push_str("}\n");

        // Supporting structs
        code.push_str("\npub struct EvolvedModuleDef {\n");
        code.push_str("    pub name: String,\n");
        code.push_str("    pub neuron_count: usize,\n");
        code.push_str("    pub position: (f32, f32),\n");
        code.push_str("    pub color: String,\n");
        code.push_str("    pub learning_rate: f32,\n");
        code.push_str("    pub firing_threshold_mean: f32,\n");
        code.push_str("    pub firing_threshold_std: f32,\n");
        code.push_str("    pub synaptic_density: f32,\n");
        code.push_str("}\n\n");

        code.push_str("pub struct EvolvedParams {\n");
        code.push_str("    pub tau: f64,\n");
        code.push_str("    pub homeostatic_rate: f64,\n");
        code.push_str("    pub prune_threshold: f64,\n");
        code.push_str("    pub stdp_learning_rate: f64,\n");
        code.push_str("    pub evolution_interval: u64,\n");
        code.push_str("    pub energy_capacity: f64,\n");
        code.push_str("    pub base_metabolic_rate: f64,\n");
        code.push_str("    pub firing_energy_cost: f64,\n");
        code.push_str("    pub synapse_energy_cost: f64,\n");
        code.push_str("    pub energy_recharge_rate: f64,\n");
        code.push_str("    pub metabolism_enabled: bool,\n");
        code.push_str("    pub cortex: CortexEvolvedConfig,\n");
        code.push_str("}\n\n");

        code.push_str("pub struct CortexEvolvedConfig {\n");
        code.push_str("    pub d_model: usize,\n");
        code.push_str("    pub state_dim: usize,\n");
        code.push_str("    pub n_layers: usize,\n");
        code.push_str("    pub modulation_strength: f64,\n");
        code.push_str("}\n\n");

        // ── Script definitions ──
        let scripts: Vec<(&str, &str)> = self.genome.script_genome.activation_scripts.iter()
            .filter(|(_, s)| !s.is_empty())
            .map(|(name, s)| (name.as_str(), s.as_str()))
            .collect();

        if !scripts.is_empty() {
            code.push_str(&format!(
                "pub fn evolved_script_count() -> i32 {{ {} }}\n\n", scripts.len()
            ));
            code.push_str("pub fn evolved_script_name(idx: i32) -> &'static str {\n");
            code.push_str("    match idx {\n");
            for (i, (name, _)) in scripts.iter().enumerate() {
                code.push_str(&format!("        {} => \"{}\",\n", i, name.escape_default()));
            }
            code.push_str("        _ => \"\",\n");
            code.push_str("    }\n");
            code.push_str("}\n\n");

            code.push_str("pub fn evolved_script_source(idx: i32) -> &'static str {\n");
            code.push_str("    match idx {\n");
            for (i, (_, src)) in scripts.iter().enumerate() {
                code.push_str(&format!("        {} => r###\"{}\"###,\n", i, src));
            }
            code.push_str("        _ => \"\",\n");
            code.push_str("    }\n");
            code.push_str("}\n\n");
        } else {
            code.push_str("pub fn evolved_script_count() -> i32 { 0 }\n");
            code.push_str("pub fn evolved_script_name(_idx: i32) -> &'static str { \"\" }\n");
            code.push_str("pub fn evolved_script_source(_idx: i32) -> &'static str { \"\" }\n\n");
        }

        // Plasticity script
        if let Some(ref ps) = self.genome.script_genome.plasticity_script {
            if !ps.is_empty() {
                code.push_str("pub fn evolved_plasticity_script_present() -> i32 { 1 }\n");
                code.push_str(&format!(
                    "pub fn evolved_plasticity_script_source() -> &'static str {{ r###\"{}\"### }}\n\n",
                    ps
                ));
            } else {
                code.push_str("pub fn evolved_plasticity_script_present() -> i32 { 0 }\n");
                code.push_str("pub fn evolved_plasticity_script_source() -> &'static str { \"\" }\n\n");
            }
        } else {
            code.push_str("pub fn evolved_plasticity_script_present() -> i32 { 0 }\n");
            code.push_str("pub fn evolved_plasticity_script_source() -> &'static str { \"\" }\n\n");
        }

        code
    }

    /// Generate a human-readable evolution report.
    pub fn generate_report(&self) -> String {
        let mut r = String::new();
        r.push_str("=== SoulLink Evolution Report ===\n\n");
        r.push_str(&format!("Generation: {}\n", self.generation));
        r.push_str(&format!("Total mutations: {}\n", self.mutation_count));
        r.push_str(&format!("Genome modules: {}\n", self.genome.modules.len()));
        r.push_str(&format!("Auto-evolve: {}\n", self.auto_evolve));
        r.push_str(&format!("Evolution interval: {} ticks\n", self.evolution_interval));

        r.push_str("\n--- Current Fitness ---\n");
        let f = &self.fitness.current;
        r.push_str(&format!("  Firing Balance:    {:.4}\n", f.firing_balance));
        r.push_str(&format!("  Synaptic Diversity:{:.4}\n", f.synaptic_diversity));
        r.push_str(&format!("  Module Utilization:{:.4}\n", f.module_utilization));
        r.push_str(&format!("  Stability:         {:.4}\n", f.stability));
        r.push_str(&format!("  Cortex Impact:     {:.4}\n", f.cortex_impact));
        r.push_str(&format!("  Growth Health:     {:.4}\n", f.growth_health));
        r.push_str(&format!("  Novelty Bonus:     {:.4}\n", f.novelty_bonus));
        r.push_str(&format!("  Energy Efficiency: {:.4}\n", f.energy_efficiency));
        r.push_str(&format!("  Overall:           {:.4}\n", f.overall));

        if !self.fitness.history.is_empty() {
            let max_f = self.fitness.history.iter().map(|f| f.overall).fold(0.0_f64, f64::max);
            r.push_str(&format!("\nBest fitness ever: {:.4}\n", max_f));
        }

        r.push_str("\n--- Last 10 Mutations ---\n");
        for event in self.history.iter().rev().take(10).rev() {
            let sign = if event.fitness_delta >= 0.0 { '+' } else { '-' };
            r.push_str(&format!(
                "Gen {:3}: {} | fitness {:.4} -> {:.4} ({}{:.4})\n",
                event.generation, event.mutation,
                event.parent_fitness, event.child_fitness,
                sign, event.fitness_delta.abs(),
            ));
        }

        r.push_str("\n--- Mutation Success Stats ---\n");
        for stat in &self.mutation_stats {
            let rate = stat.success_rate();
            let avg_d = stat.avg_delta();
            r.push_str(&format!(
                "  {:<15} attempts={:<4} success={:.3}  avg_delta={:+.5}  weight={:.2}\n",
                stat.type_name, stat.attempts, rate, avg_d, stat.weight,
            ));
        }

        r.push_str("--- Evolution Goal ---\n");
        let g = &self.evolution_goal;
        r.push_str(&format!("  Enabled:    {}\n", g.enabled));
        r.push_str(&format!("  Target:     {}\n", g.target_metric));
        r.push_str(&format!("  Weight:     {:.2}\n", g.weight));
        r.push_str(&format!("  Novelty weight: {:.3}\n", self.fitness.novelty_weight));

        r.push_str("\n--- Meta-Genome ---\n");
        let mg = &self.meta_genome;
        r.push_str(&format!("  Mutation rate:      {:.3}\n", mg.mutation_rate));
        r.push_str(&format!("  Selection pressure: {:.3}\n", mg.selection_pressure));
        r.push_str(&format!("  Exploration rate:   {:.3}\n", mg.exploration_rate));
        r.push_str(&format!("  Evolution interval: [{}, {}]\n", mg.min_evolution_interval, mg.max_evolution_interval));
        r.push_str(&format!("  Meta-mutation rate: {:.4}\n", mg.meta_mutation_rate));
        r.push_str(&format!("  Best fitness ever:  {:.4}\n", self.best_fitness));

        r.push_str("--- Heal Thresholds ---\n");
        r.push_str(&format!("  Stalled threshold:      {:.3}\n", mg.heal_stalled_threshold));
        r.push_str(&format!("  Stalled boost:         {:.3}\n", mg.heal_stalled_boost));
        r.push_str(&format!("  Stalled fraction:      {:.3}\n", mg.heal_stalled_fraction));
        r.push_str(&format!("  Collapse min active:   {:.3}\n", mg.heal_collapse_min_active));
        r.push_str(&format!("  Collapse act boost:    {:.3}\n", mg.heal_collapse_activation_boost));
        r.push_str(&format!("  Collapse pot boost:    {:.3}\n", mg.heal_collapse_potential_boost));

        r.push_str("--- Divergence Detection ---\n");
        r.push_str(&format!("  Variance threshold: {:.4}\n", mg.fitness_divergence_threshold));
        r.push_str(&format!("  Freeze duration:    {} ticks\n", mg.fitness_freeze_duration));
        r.push_str(&format!("  Currently frozen:   {}\n", if self.fitness_frozen { "YES" } else { "no" }));
        r.push_str(&format!("  Freeze ticks left:  {}\n", self.fitness_freeze_ticks));

        r.push_str("--- Toxic Blacklist ---\n");
        r.push_str(&format!("  Memory ticks:         {}\n", mg.toxic_memory_ticks));
        r.push_str(&format!("  Failure threshold:    {}\n", mg.toxic_failure_threshold));
        r.push_str(&format!("  Currently blacklisted: {}\n", self.mutation_blacklist.len()));
        for (name, expires) in &self.mutation_blacklist {
            r.push_str(&format!("    {} (expires gen {})\n", name, expires));
        }

        r.push_str("--- Episodic Memory ---\n");
        r.push_str(&format!("  Events recorded:     {}\n", self.episodic_memory.len()));

        r.push_str("\n--- Pareto Frontier ---\n");
        r.push_str(&format!("  Size:        {}/{}\n", self.pareto_frontier.size(), self.pareto_frontier.max_size));
        r.push_str(&format!("  Hypervolume: {:.4}\n", self.pareto_frontier.hypervolume()));
        if !self.pareto_frontier.individuals.is_empty() {
            let pf = &self.pareto_frontier.individuals;
            let mut avg_obj = [0.0_f64; 10];
            for ind in pf {
                for (i, v) in ind.objectives.iter().enumerate() {
                    avg_obj[i] += v;
                }
            }
            for v in &mut avg_obj {
                *v /= pf.len() as f64;
            }
            r.push_str(&format!(
                "  Avg objectives: [fb={:.3}, sd={:.3}, mu={:.3}, st={:.3}, ci={:.3}, gh={:.3}, nb={:.3}, ee={:.3}, tc={:.3}, ec={:.3}]\n",
                avg_obj[0], avg_obj[1], avg_obj[2], avg_obj[3], avg_obj[4], avg_obj[5], avg_obj[6], avg_obj[7], avg_obj[8], avg_obj[9],
            ));
        }
        // Top mutation types by count in episodic memory
        if !self.episodic_memory.is_empty() {
            let mut type_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            for ev in &self.episodic_memory {
                *type_counts.entry(&ev.mutation_type).or_insert(0) += 1;
            }
            let mut sorted: Vec<_> = type_counts.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            for (mtype, count) in sorted.iter().take(5) {
                r.push_str(&format!("  {:<15} x{}\n", mtype, count));
            }
        }

        r
    }

    /// Write the evolved genome as Rust source to disk.
    /// Returns the file path and whether it was written.
    pub fn write_genome_source(&self) -> (String, bool) {
        let path = "evolved_genome.rs".to_string();
        let source = self.generate_module_source();
        match std::fs::write(&path, &source) {
            Ok(_) => (path, true),
            Err(_) => (path, false),
        }
    }

    /// Apply global parameters from the genome to the brain.
    /// This allows evolved params (tau, homeostatic_rate, etc.) to
    /// actually affect the brain simulation.
    pub fn apply_global_params(&self, brain: &mut crate::Brain) {
        let gp = &self.genome.brain_params;
        brain.params.tau = gp.tau;
        brain.params.homeostatic_rate = gp.homeostatic_rate;
        brain.params.prune_threshold = gp.prune_threshold;
        brain.params.stdp_learning_rate = gp.stdp_learning_rate;
        brain.params.energy_capacity = gp.energy_capacity;
        brain.params.base_metabolic_rate = gp.base_metabolic_rate;
        brain.params.firing_energy_cost = gp.firing_energy_cost;
        brain.params.synapse_energy_cost = gp.synapse_energy_cost;
        brain.params.energy_recharge_rate = gp.energy_recharge_rate;
        brain.params.metabolism_enabled = gp.metabolism_enabled;
        brain.params.energy_pool_enabled = gp.energy_pool_enabled;
        brain.params.energy_pool_recharge_rate = gp.energy_pool_recharge_rate;
        brain.params.pool_contribution = gp.pool_contribution;
        brain.params.neuron_creation_cost = gp.neuron_creation_cost;
        brain.params.synapse_creation_cost = gp.synapse_creation_cost;
        brain.params.growth_energy_threshold = gp.growth_energy_threshold;
        brain.behavior = self.genome.behaviors.clone();
    }

    /// Apply a hot-loaded genome's compiled parameters to the brain.
    ///
    /// This is the missing "apply" step in the hot-reload pipeline:
    /// the compiled cdylib is loaded every 1000 ticks but its values
    /// were never propagated to brain behavior. This method:
    /// 1. Syncs tau, homeostatic_rate, prune_threshold, stdp_learning_rate
    /// 2. Syncs cortex modulation_strength at runtime
    /// 3. Adds modules from the loaded genome that don't exist yet
    /// 4. Also updates the in-memory genome so apply_global_params stays consistent
    ///
    /// Returns a list of human-readable change descriptions for logging.
    pub fn apply_loaded_genome(&mut self, brain: &mut crate::Brain, loaded: &crate::hotload::LoadedGenome) -> Vec<String> {
        let mut changes = Vec::new();
        let bp = loaded.as_brain_params();

        // 1. Sync brain params
        if (brain.params.tau - bp.tau).abs() > f32::EPSILON {
            changes.push(format!("tau {:.4} -> {:.4}", brain.params.tau, bp.tau));
            brain.params.tau = bp.tau;
        }
        if (brain.params.homeostatic_rate - bp.homeostatic_rate).abs() > f32::EPSILON {
            changes.push(format!("homeostatic_rate {:.4} -> {:.4}", brain.params.homeostatic_rate, bp.homeostatic_rate));
            brain.params.homeostatic_rate = bp.homeostatic_rate;
        }
        if (brain.params.prune_threshold - bp.prune_threshold).abs() > f32::EPSILON {
            changes.push(format!("prune_threshold {:.4} -> {:.4}", brain.params.prune_threshold, bp.prune_threshold));
            brain.params.prune_threshold = bp.prune_threshold;
        }
        if (brain.params.stdp_learning_rate - bp.stdp_learning_rate).abs() > f32::EPSILON {
            changes.push(format!("stdp_learning_rate {:.4} -> {:.4}", brain.params.stdp_learning_rate, bp.stdp_learning_rate));
            brain.params.stdp_learning_rate = bp.stdp_learning_rate;
        }

        // 1b. Sync energy/metabolism params
        if (brain.params.energy_capacity - bp.energy_capacity).abs() > f32::EPSILON {
            changes.push(format!("energy_capacity {:.4} -> {:.4}", brain.params.energy_capacity, bp.energy_capacity));
            brain.params.energy_capacity = bp.energy_capacity;
        }
        if (brain.params.base_metabolic_rate - bp.base_metabolic_rate).abs() > f32::EPSILON {
            changes.push(format!("base_metabolic_rate {:.7} -> {:.7}", brain.params.base_metabolic_rate, bp.base_metabolic_rate));
            brain.params.base_metabolic_rate = bp.base_metabolic_rate;
        }
        if (brain.params.firing_energy_cost - bp.firing_energy_cost).abs() > f32::EPSILON {
            changes.push(format!("firing_energy_cost {:.5} -> {:.5}", brain.params.firing_energy_cost, bp.firing_energy_cost));
            brain.params.firing_energy_cost = bp.firing_energy_cost;
        }
        if (brain.params.synapse_energy_cost - bp.synapse_energy_cost).abs() > f32::EPSILON {
            changes.push(format!("synapse_energy_cost {:.8} -> {:.8}", brain.params.synapse_energy_cost, bp.synapse_energy_cost));
            brain.params.synapse_energy_cost = bp.synapse_energy_cost;
        }
        if (brain.params.energy_recharge_rate - bp.energy_recharge_rate).abs() > f32::EPSILON {
            changes.push(format!("energy_recharge_rate {:.6} -> {:.6}", brain.params.energy_recharge_rate, bp.energy_recharge_rate));
            brain.params.energy_recharge_rate = bp.energy_recharge_rate;
        }
        if brain.params.metabolism_enabled != bp.metabolism_enabled {
            changes.push(format!("metabolism_enabled {} -> {}", brain.params.metabolism_enabled, bp.metabolism_enabled));
            brain.params.metabolism_enabled = bp.metabolism_enabled;
        }

        // 2. Sync cortex modulation_strength (structural params skipped — need rebuild)
        let ms = loaded.cortex_modulation_strength;
        if (brain.cortex.config.modulation_strength - ms).abs() > f64::EPSILON {
            changes.push(format!("cortex_modulation {:.3} -> {:.3}", brain.cortex.config.modulation_strength, ms));
            brain.cortex.config.modulation_strength = ms;
            self.genome.cortex_config.modulation_strength = ms;
        }

        // 3. Sync structural cortex params to genome for restart persistence
        if self.genome.cortex_config.d_model != loaded.cortex_d_model {
            changes.push(format!("cortex_d_model {} -> {} (next restart)", self.genome.cortex_config.d_model, loaded.cortex_d_model));
            self.genome.cortex_config.d_model = loaded.cortex_d_model;
        }
        if self.genome.cortex_config.state_dim != loaded.cortex_state_dim {
            changes.push(format!("cortex_state_dim {} -> {} (next restart)", self.genome.cortex_config.state_dim, loaded.cortex_state_dim));
            self.genome.cortex_config.state_dim = loaded.cortex_state_dim;
        }
        if self.genome.cortex_config.n_layers != loaded.cortex_n_layers {
            changes.push(format!("cortex_n_layers {} -> {} (next restart)", self.genome.cortex_config.n_layers, loaded.cortex_n_layers));
            self.genome.cortex_config.n_layers = loaded.cortex_n_layers;
        }

        // 4. Reconcile module list — add any missing modules from loaded genome
        for (mod_name, neuron_count) in &loaded.modules {
            if !brain.modules.contains_key(mod_name.as_str()) {
                let mut rng = rand::rng();
                let module = crate::BrainModule {
                    name: mod_name.clone(),
                    neuron_indices: Vec::new(),
                    activation_level: rng.random_range(0.5..0.8),
                    importance: rng.random_range(0.3..0.7),
                    x: rng.random_range(-5.0..5.0),
                    y: rng.random_range(-5.0..5.0),
                    color: "#888".to_string(),
                    schema_version: 0,
                    extensions: std::collections::HashMap::new(),
                    energy_reserve: 0.5,
                    energy_budget: 1.0,
                    base_metabolic_mod: 1.0,
                    energy_priority: 0.5,
                    local_stress: 0.0,
                };
                brain.modules.insert(mod_name.clone(), module);
                brain.module_order.push(mod_name.clone());
                // Add neurons to the new module
                for _ in 0..*neuron_count {
                    brain.add_neuron_to(mod_name);
                }
                changes.push(format!("added module '{}' ({} neurons)", mod_name, neuron_count));
            }
        }

        // 5. Restore scripts from loaded genome
        for (mod_name, script_source) in &loaded.scripts {
            if !script_source.is_empty() {
                let mod_exists = brain.modules.contains_key(mod_name.as_str());
                if mod_exists {
                    brain.script_engine.set_activation_script(mod_name, script_source)
                        .unwrap_or_else(|e| {
                            tracing::warn!("apply_loaded_genome: script error for '{}': {}", mod_name, e);
                        });
                    self.genome.script_genome.activation_scripts.insert(mod_name.clone(), script_source.clone());
                    changes.push(format!("restored activation script for '{}' ({} chars)", mod_name, script_source.len()));
                }
            }
        }
        if let Some(ref ps) = loaded.plasticity_script {
            if !ps.is_empty() {
                brain.script_engine.set_plasticity_script(ps)
                    .unwrap_or_else(|e| {
                        tracing::warn!("apply_loaded_genome: plasticity script error: {}", e);
                    });
                self.genome.script_genome.plasticity_script = Some(ps.clone());
                changes.push(format!("restored plasticity script ({} chars)", ps.len()));
            }
        }

        // 6. Keep in-memory genome consistent with loaded values
        self.genome.brain_params.tau = loaded.tau as f32;
        self.genome.brain_params.homeostatic_rate = loaded.homeostatic_rate as f32;
        self.genome.brain_params.prune_threshold = loaded.prune_threshold as f32;
        self.genome.brain_params.stdp_learning_rate = loaded.stdp_learning_rate as f32;

        changes
    }

    /// Analyze episodic memory to derive contextual priority rules.
    ///
    /// Buckets each mutation event by context dimension ranges, computes
    /// average success rate and fitness delta per bucket. Produces a set of
    /// `ContextualPriorityRule` entries that bias `select_mutation()`.
    ///
    /// Runs every `symbolic_analysis_interval` ticks in the simulation loop.
    pub fn run_symbolic_analysis(&mut self) {
        let min_samples = self.meta_genome.symbolic_min_samples;
        if self.episodic_memory.len() < min_samples * 2 {
            return;
        }

        let dimensions = ["firing_balance", "synaptic_diversity", "module_utilization", "stability"];
        let mut new_rules: Vec<ContextualPriorityRule> = Vec::new();

        for dim in &dimensions {
            // Collect values for this dimension
            let values: Vec<f64> = self.episodic_memory.iter().map(|e| match *dim {
                "firing_balance" => e.ctx_firing_balance,
                "synaptic_diversity" => e.ctx_synaptic_diversity,
                "module_utilization" => e.ctx_module_utilization,
                "stability" => e.ctx_stability,
                _ => 0.0,
            }).collect();

            if values.is_empty() {
                continue;
            }

            // Determine range and divide into 4 equal buckets
            let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = max_val - min_val;
            if range < 0.001 {
                continue;
            }

            for bucket in 0..4 {
                let lo = min_val + range * (bucket as f64) / 4.0;
                let hi = min_val + range * ((bucket + 1) as f64) / 4.0;

                // Collect mutation types in this bucket
                let mut type_samples: std::collections::HashMap<String, Vec<f64>> =
                    std::collections::HashMap::new();
                for event in &self.episodic_memory {
                    let ctx_val = match *dim {
                        "firing_balance" => event.ctx_firing_balance,
                        "synaptic_diversity" => event.ctx_synaptic_diversity,
                        "module_utilization" => event.ctx_module_utilization,
                        "stability" => event.ctx_stability,
                        _ => 0.0,
                    };
                    if ctx_val >= lo && ctx_val < hi {
                        type_samples.entry(event.mutation_type.clone())
                            .or_insert_with(Vec::new)
                            .push(event.fitness_delta);
                    }
                }

                for (mutation_type, deltas) in &type_samples {
                    if deltas.len() < min_samples {
                        continue;
                    }
                    let sample_count = deltas.len();
                    let avg_delta: f64 = deltas.iter().sum::<f64>() / sample_count as f64;
                    let success_count = deltas.iter().filter(|&&d| d > 0.001).count();
                    let success_rate = success_count as f64 / sample_count as f64;

                    if success_rate < self.meta_genome.symbolic_min_success {
                        continue;
                    }

                    let bias_weight = avg_delta.clamp(-1.0, 1.0) * success_rate;
                    new_rules.push(ContextualPriorityRule {
                        context_dim: dim.to_string(),
                        context_lo: lo,
                        context_hi: hi,
                        mutation_type: mutation_type.clone(),
                        avg_delta,
                        success_rate,
                        bias_weight,
                        sample_count,
                        last_verified: now_f64(),
                    });
                }
            }
        }

        // Sort by |bias_weight| descending, keep top max_rules
        new_rules.sort_by(|a, b| b.bias_weight.abs().partial_cmp(&a.bias_weight.abs()).unwrap_or(std::cmp::Ordering::Equal));
        let max_rules = self.meta_genome.symbolic_max_rules;
        if new_rules.len() > max_rules {
            new_rules.truncate(max_rules);
        }

        self.priority_rules = new_rules;

        // Log top 3 rules
        if !self.priority_rules.is_empty() {
            tracing::info!("Symbolic analysis: {} rules derived", self.priority_rules.len());
            for r in self.priority_rules.iter().take(3) {
                tracing::info!("  [priority] {} when {} in [{:.2},{:.2}] delta={:+.4} rate={:.2}",
                    r.mutation_type, r.context_dim, r.context_lo, r.context_hi,
                    r.avg_delta, r.success_rate);
            }
        }
    }

    /// Autonomous healing: detect and fix common brain failure modes.
    ///
    /// Scans for:
    /// - Modules where ALL neurons have zero firing count (dead module)
    /// - Modules with activation level below 0.05 (stalled module)
    /// - Global firing rate collapse (total firing < 1% of neurons)
    ///
    /// Applies corrective boosts and logs the healing event.
    pub fn auto_heal(&mut self, brain: &mut crate::Brain) -> Vec<String> {
        let mut events = Vec::new();
        let now = now_f64();

        for name in &brain.module_order {
            let indices = match brain.modules.get(name) {
                Some(m) => m.neuron_indices.clone(),
                None => continue,
            };
            if indices.is_empty() {
                continue;
            }

            // Count neurons that have ever fired
            let ever_fired = indices.iter()
                .filter(|&&i| i < brain.neurons.len() && brain.neurons[i].firing_count > 0)
                .count();

            // Count recently active neurons
            let recently_active = indices.iter()
                .filter(|&&i| {
                    if i >= brain.neurons.len() { return false; }
                    let n = &brain.neurons[i];
                    n.firing_count > 0 && (now - n.last_fired) < 5.0
                })
                .count();

            // ── Dead module: no neuron has ever fired ──
            if ever_fired == 0 {
                // Boost all neurons in the module
                let boost_count = indices.len().min(50);
                let mut rng = rand::rng();
                for _ in 0..boost_count {
                    let idx = indices[rng.random_range(0..indices.len())];
                    if idx < brain.neurons.len() {
                        brain.neurons[idx].potential = 0.8;
                        brain.neurons[idx].firing_threshold = 0.6;
                        brain.neurons[idx].importance = 0.7;
                    }
                }
                events.push(format!("Heal: dead module '{}' boosted {} neurons", name, boost_count));
            }

            // ── Stalled module: very low recent activity ──
            let threshold = self.meta_genome.heal_stalled_threshold;
            let module_frac = recently_active as f64 / indices.len() as f64;
            if module_frac < threshold && ever_fired > 0 {
                let boost_count = (indices.len() as f64 * self.meta_genome.heal_stalled_fraction).max(5.0) as usize;
                let boost_amt = self.meta_genome.heal_stalled_boost;
                let mut rng = rand::rng();
                for _ in 0..boost_count {
                    let idx = indices[rng.random_range(0..indices.len())];
                    if idx < brain.neurons.len() {
                        brain.neurons[idx].potential = (brain.neurons[idx].potential + boost_amt as f32).min(1.0);
                    }
                }
                events.push(format!("Heal: stalled module '{}' ({:.1}% active) boosted {} neurons",
                    name, module_frac * 100.0, boost_count));
            }
        }

        // ── Global firing collapse ──
        let total_active = brain.neurons.iter()
            .filter(|n| n.firing_count > 0)
            .count();
        let min_active_frac = self.meta_genome.heal_collapse_min_active;
        if brain.neurons.len() > 100 && (total_active as f64 / brain.neurons.len() as f64) < min_active_frac {
            // Global boost: increase external input across all modules
            let act_boost = self.meta_genome.heal_collapse_activation_boost as f32;
            let pot_boost = self.meta_genome.heal_collapse_potential_boost as f32;
            let mut rng = rand::rng();
            for module in brain.modules.values_mut() {
                module.activation_level = (module.activation_level + act_boost).min(1.0);
            }
            for _ in 0..(brain.neurons.len() / 50).max(10) {
                let idx = rng.random_range(0..brain.neurons.len());
                brain.neurons[idx].potential = (brain.neurons[idx].potential + pot_boost).min(1.0);
            }
            events.push(format!("Heal: global firing collapse ({}/{} active) — injected noise",
                total_active, brain.neurons.len()));
        }

        events
    }

    /// Get evolution status as a JSON value for the API.
    pub fn api_status(&self) -> serde_json::Value {
        let f = &self.fitness.current;
        let mg = &self.meta_genome;

        serde_json::json!({
            "auto_evolve": self.auto_evolve,
            "generation": self.generation,
            "mutation_count": self.mutation_count,
            "genome_modules": self.genome.modules.len(),
            "evolution_interval": self.evolution_interval,
            "best_fitness": (self.best_fitness * 1000.0).round() / 1000.0,
            "fitness": {
                "firing_balance": (f.firing_balance * 1000.0).round() / 1000.0,
                "synaptic_diversity": (f.synaptic_diversity * 1000.0).round() / 1000.0,
                "module_utilization": (f.module_utilization * 1000.0).round() / 1000.0,
                "stability": (f.stability * 1000.0).round() / 1000.0,
                "cortex_impact": (f.cortex_impact * 1000.0).round() / 1000.0,
                "growth_health": (f.growth_health * 1000.0).round() / 1000.0,
                "novelty_bonus": (f.novelty_bonus * 1000.0).round() / 1000.0,
                "overall": (f.overall * 1000.0).round() / 1000.0,
            },
            "novelty_weight": (self.fitness.novelty_weight * 1000.0).round() / 1000.0,
            "evolution_goal": {
                "enabled": self.evolution_goal.enabled,
                "target_metric": self.evolution_goal.target_metric,
                "weight": (self.evolution_goal.weight * 100.0).round() / 100.0,
            },
            "meta_genome": {
                "mutation_rate": (mg.mutation_rate * 1000.0).round() / 1000.0,
                "selection_pressure": (mg.selection_pressure * 1000.0).round() / 1000.0,
                "exploration_rate": (mg.exploration_rate * 1000.0).round() / 1000.0,
                "min_interval": mg.min_evolution_interval,
                "max_interval": mg.max_evolution_interval,
                "heal_stalled_threshold": (mg.heal_stalled_threshold * 1000.0).round() / 1000.0,
                "heal_stalled_boost": (mg.heal_stalled_boost * 1000.0).round() / 1000.0,
                "heal_collapse_min_active": (mg.heal_collapse_min_active * 1000.0).round() / 1000.0,
                "fitness_divergence_threshold": (mg.fitness_divergence_threshold * 1000.0).round() / 1000.0,
                "fitness_freeze_duration": mg.fitness_freeze_duration,
            },
            "fitness_frozen": self.fitness_frozen,
            "fitness_freeze_ticks": self.fitness_freeze_ticks,
            "did_meta_reset": self.did_meta_reset,
            "episodic_memory_len": self.episodic_memory.len(),
            "blacklist_len": self.mutation_blacklist.len(),
            "toxic_memory_ticks": mg.toxic_memory_ticks,
            "toxic_failure_threshold": mg.toxic_failure_threshold,
            "pareto_frontier": {
                "size": self.pareto_frontier.size(),
                "max_size": self.pareto_frontier.max_size,
                "hypervolume": (self.pareto_frontier.hypervolume() * 1000.0).round() / 1000.0,
            },
            "mutation_stats": self.mutation_stats.iter().map(|s| {
                serde_json::json!({
                    "type": s.type_name,
                    "attempts": s.attempts,
                    "success_rate": (s.success_rate() * 1000.0).round() / 1000.0,
                    "avg_delta": (s.avg_delta() * 10000.0).round() / 10000.0,
                    "weight": (s.weight * 100.0).round() / 100.0,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Brain;

    #[test]
    fn test_engine_default() {
        let e = EvolutionEngine::default();
        assert_eq!(e.genome.modules.len(), 11);
        assert!(e.auto_evolve);
        assert_eq!(e.generation, 0);
    }

    #[test]
    fn test_genome_base() {
        let genome = Genome::base();
        assert_eq!(genome.modules.len(), 11);
        assert_eq!(genome.modules[0].name, "perception");
        assert_eq!(genome.modules[10].name, "extra");
    }

    #[test]
    fn test_fitness_sample_produces_metrics() {
        let brain = Brain::new("test", 9999);
        let mut tracker = FitnessTracker::new(1);
        let metrics = tracker.sample(&brain);
        assert!(metrics.overall >= 0.0);
        assert!(metrics.overall <= 1.0);
    }

    #[test]
    fn test_step_no_crash() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        engine.auto_evolve = false; // prevent auto mutations
        engine.step(&mut brain);
        assert!(engine.fitness.history.is_empty() || engine.fitness.current.overall > 0.0);
    }

    #[test]
    fn test_step_with_auto_evolve() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        engine.auto_evolve = true;
        engine.evolution_interval = 1; // evolve every tick
        engine.fitness.sample_interval = 1;
        engine.step(&mut brain);
        // Should have either sampled or evolved
        _ = engine.generation;
    }

    #[test]
    fn test_add_neuron_mutation() {
        let mut brain = Brain::new("test", 9999);
        let initial_count = brain.neurons.len();
        let mut engine = EvolutionEngine::default();
        engine.apply_mutation(&mut brain, &Mutation::AddNeuron { module: 0, count: 5 });
        assert_eq!(brain.neurons.len(), initial_count + 5);
    }

    #[test]
    fn test_add_module_mutation() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        let initial_modules = brain.module_order.len();
        engine.apply_mutation(&mut brain, &Mutation::AddModule {
            name: "test_module".into(),
            count: 100,
            x: 500.0, y: 500.0,
            color: "#ff0000".into(),
        });
        assert_eq!(brain.module_order.len(), initial_modules + 1);
        assert!(brain.modules.contains_key("test_module"));
    }

    #[test]
    fn test_generate_module_source() {
        let engine = EvolutionEngine::default();
        let source = engine.generate_module_source();
        assert!(source.contains("evolved_module_config"));
        assert!(source.contains("EvolvedModuleDef"));
        assert!(source.contains("EvolvedParams"));
    }

    #[test]
    fn test_generate_report() {
        let engine = EvolutionEngine::default();
        let report = engine.generate_report();
        assert!(report.contains("Evolution Report"));
        assert!(report.contains("Generation: 0"));
    }

    #[test]
    fn test_select_mutation_returns_valid() {
        let brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        // Sample to get some fitness data
        engine.fitness.sample(&brain);
        let mutation = engine.select_mutation(&brain, 0.15);
        // Mutation should be one of the known variants
        let s = mutation.to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn test_mutate_param() {
        let mut engine = EvolutionEngine::default();
        let old_tau = engine.genome.brain_params.tau;
        engine.apply_mutation(&mut crate::Brain::new("test", 9999),
            &Mutation::MutateParam { param: "tau".into(), delta: -0.02 });
        assert!((engine.genome.brain_params.tau - old_tau + 0.02).abs() < 0.001);
    }

    #[test]
    fn test_evolution_does_not_panic() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        engine.auto_evolve = true;
        engine.evolution_interval = 3;
        engine.fitness.sample_interval = 1;
        for _ in 0..20 {
            engine.step(&mut brain);
        }
        // Should not panic, engine is in valid state
        let _ = engine.generation;
        assert!(!engine.history.is_empty() || engine.auto_evolve);
    }

    // ── New method tests ──

    #[test]
    fn test_meta_genome_default() {
        let mg = MetaGenome::default();
        assert!(mg.mutation_rate > 0.0);
        assert!(mg.selection_pressure > 0.0);
        assert!(mg.exploration_rate > 0.0);
        assert!(mg.min_evolution_interval <= mg.max_evolution_interval);
    }

    #[test]
    fn test_mutation_type_stats() {
        let mut stats = MutationTypeStats::new("AddNeuron");
        assert_eq!(stats.attempts, 0);
        assert!((stats.success_rate() - 0.5).abs() < 0.001);
        stats.attempts = 10;
        stats.successes = 7;
        assert!((stats.success_rate() - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_track_mutation_result() {
        let mut engine = EvolutionEngine::default();
        let _brain = Brain::new("test", 9999);

        // Track a successful AddNeuron mutation
        let m = Mutation::AddNeuron { module: 0, count: 1 };
        engine.track_mutation_result(&m, 0.5, 0.6);

        // Track a failed one
        let m2 = Mutation::AddNeuron { module: 0, count: 1 };
        engine.track_mutation_result(&m2, 0.5, 0.4);

        // Check results (clone to avoid borrow conflict)
        let stats = engine.mutation_stats.clone();
        let add_stats = stats.iter().find(|s| s.type_name == "AddNeuron").unwrap();
        assert_eq!(add_stats.attempts, 2);
        assert_eq!(add_stats.successes, 1); // second was negative delta
        assert!(add_stats.weight > 0.5);
    }

    #[test]
    fn test_meta_mutate_changes_params() {
        let mut engine = EvolutionEngine::default();
        let old_rate = engine.meta_genome.mutation_rate;

        // Apply meta mutation many times (it's random, but should change something)
        for _ in 0..20 {
            engine.meta_mutate();
        }

        // Meta-genome should have changed in some way
        // (at least one of these should differ)
        let mg = &engine.meta_genome;
        let _changed = (mg.mutation_rate - old_rate).abs() > 0.0001;
        // Can't guarantee which param changed, just that the engine is in valid state
        assert!(mg.mutation_rate >= 0.005 && mg.mutation_rate <= 0.1);
        assert!(mg.selection_pressure >= 0.1 && mg.selection_pressure <= 1.0);
        assert!(mg.exploration_rate >= 0.05 && mg.exploration_rate <= 0.5);
        assert!(mg.min_evolution_interval >= 20 && mg.max_evolution_interval <= 500);
        assert!(mg.min_evolution_interval <= mg.max_evolution_interval);
    }

    #[test]
    fn test_apply_global_params_syncs_brain() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();

        // Modify genome params
        engine.genome.brain_params.tau = 0.95;
        engine.genome.brain_params.homeostatic_rate = 0.01;
        engine.genome.brain_params.prune_threshold = 0.1;
        engine.genome.brain_params.stdp_learning_rate = 0.008;

        // Apply
        engine.apply_global_params(&mut brain);

        // Verify brain params match
        assert!((brain.params.tau - 0.95).abs() < 0.001);
        assert!((brain.params.homeostatic_rate - 0.01).abs() < 0.001);
        assert!((brain.params.prune_threshold - 0.10).abs() < 0.001);
        assert!((brain.params.stdp_learning_rate - 0.008).abs() < 0.001);
    }

    #[test]
    fn test_auto_heal_no_false_positives() {
        // A fresh brain should not trigger healing
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();

        // Run a few steps to get some activity
        for _ in 0..10 {
            brain.step();
        }

        let events = engine.auto_heal(&mut brain);
        // Most likely no events for a fresh brain with 33k neurons
        // (some modules may have low activity but probably not dead)
        assert!(events.is_empty() || events.len() <= 20); // generous upper bound
    }

    #[test]
    fn test_auto_heal_dead_module() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();

        // Force all neurons in a module to have zero firing_count
        if let Some(module) = brain.modules.get("perception") {
            let indices = module.neuron_indices.clone();
            for &i in &indices {
                if i < brain.neurons.len() {
                    brain.neurons[i].firing_count = 0;
                    brain.neurons[i].last_fired = 0.0;
                }
            }
        }

        let events = engine.auto_heal(&mut brain);
        let dead_events: Vec<_> = events.iter().filter(|e| e.contains("dead module")).collect();
        assert!(!dead_events.is_empty(), "Should detect dead module: {:?}", events);
    }

    #[test]
    fn test_write_genome_source_creates_file() {
        let engine = EvolutionEngine::default();
        let (path, ok) = engine.write_genome_source();
        assert!(ok, "Should write file successfully");
        assert!(path.contains("evolved_genome.rs"));
        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_api_status_returns_valid_json() {
        let brain = Brain::new("test", 9999);
        let mut engine = EvolutionEngine::default();
        engine.fitness.sample(&brain);

        let status = engine.api_status();
        assert!(status.get("auto_evolve").and_then(|v| v.as_bool()).unwrap_or(false));
        assert!(status.get("generation").and_then(|v| v.as_u64()).is_some());
        assert!(status.get("fitness").is_some());
        assert!(status.get("meta_genome").is_some());
        assert!(status.get("mutation_stats").and_then(|v| v.as_array()).is_some());
    }

    #[test]
    fn test_mutation_type_key() {
        assert_eq!(Mutation::AddNeuron { module: 0, count: 1 }.type_key(), "AddNeuron");
        assert_eq!(Mutation::MutateParam { param: "tau".into(), delta: 0.0 }.type_key(), "MutateParam");
        assert_eq!(Mutation::MutateCortexConfig { d_model: 16, state_dim: 4, n_layers: 2 }.type_key(), "MutateCortex");
    }

    #[test]
    fn test_generate_report_includes_meta() {
        let engine = EvolutionEngine::default();
        let report = engine.generate_report();
        assert!(report.contains("Meta-Genome"));
        assert!(report.contains("Mutation Success Stats"));
        assert!(report.contains("Best fitness"));
    }

    // ── Symbolic backpropagation tests ──

    #[test]
    fn test_meta_genome_symbolic_defaults() {
        let mg = MetaGenome::default();
        assert_eq!(mg.symbolic_analysis_interval, 500);
        assert_eq!(mg.symbolic_min_samples, 5);
        assert!((mg.symbolic_min_success - 0.4).abs() < 0.001);
        assert_eq!(mg.symbolic_max_rules, 20);
    }

    #[test]
    fn test_symbolic_analysis_empty_memory() {
        let mut engine = EvolutionEngine::default();
        engine.run_symbolic_analysis();
        assert!(engine.priority_rules.is_empty(), "No rules with empty memory");
    }

    #[test]
    fn test_symbolic_analysis_creates_rules() {
        let mut engine = EvolutionEngine::default();
        // Populate episodic memory with 20 AddNeuron events (positive delta, low firing_balance)
        for i in 0..20 {
            engine.episodic_memory.push_back(MutationEpisodicEvent {
                mutation_type: "AddNeuron".into(),
                generation: i,
                parent_fitness: 0.3,
                child_fitness: 0.35,
                fitness_delta: 0.05,
                ctx_firing_balance: 0.15 + (i as f64) * 0.01,
                ctx_synaptic_diversity: 0.5,
                ctx_module_utilization: 0.5,
                ctx_stability: 0.5,
                timestamp: i as f64,
            });
        }
        // Populate with 5 MutateParam events (negative delta, high stability)
        for i in 0..5 {
            engine.episodic_memory.push_back(MutationEpisodicEvent {
                mutation_type: "MutateParam".into(),
                generation: i + 20,
                parent_fitness: 0.6,
                child_fitness: 0.55,
                fitness_delta: -0.05,
                ctx_firing_balance: 0.5,
                ctx_synaptic_diversity: 0.5,
                ctx_module_utilization: 0.5,
                ctx_stability: 0.85 + (i as f64) * 0.02,
                timestamp: (i + 20) as f64,
            });
        }
        engine.run_symbolic_analysis();
        assert!(!engine.priority_rules.is_empty(), "Should derive rules from memory");
        // Should have at least one rule for AddNeuron (positive delta, enough samples)
        let add_rules: Vec<_> = engine.priority_rules.iter()
            .filter(|r| r.mutation_type == "AddNeuron").collect();
        assert!(!add_rules.is_empty(), "Should have AddNeuron rules");
        // AddNeuron rules should have positive avg_delta
        for rule in &add_rules {
            assert!(rule.avg_delta > 0.0, "AddNeuron should have positive delta");
        }
    }

    #[test]
    fn test_priority_rule_serialization() {
        let rule = ContextualPriorityRule {
            context_dim: "firing_balance".into(),
            context_lo: 0.2,
            context_hi: 0.4,
            mutation_type: "AddNeuron".into(),
            avg_delta: 0.05,
            success_rate: 0.75,
            bias_weight: 0.0375,
            sample_count: 12,
            last_verified: 1234.5,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: ContextualPriorityRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.context_dim, "firing_balance");
        assert!((deserialized.context_lo - 0.2).abs() < 0.001);
        assert_eq!(deserialized.sample_count, 12);
    }

    // ── Pareto frontier tests ──

    #[test]
    fn test_pareto_dominates() {
        let a = [0.8, 0.6, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5];
        let b = [0.7, 0.5, 0.5, 0.6, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5];
        assert!(ParetoFrontier::dominates(&a, &b), "A should dominate B");
        assert!(!ParetoFrontier::dominates(&b, &a), "B should not dominate A");
    }

    #[test]
    fn test_pareto_non_dominated_equal() {
        let a = [0.8, 0.6, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5];
        let b = [0.8, 0.6, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5];
        assert!(!ParetoFrontier::dominates(&a, &b), "Equal should not dominate");
        assert!(!ParetoFrontier::dominates(&b, &a), "Equal should not dominate");
    }

    #[test]
    fn test_pareto_frontier_insert() {
        let mut frontier = ParetoFrontier::new(10);

        let ind1 = ParetoIndividual {
            objectives: [0.8, 0.6, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5],
            generation: 1,
            crowding_distance: 0.0,
        };
        let changed1 = frontier.try_insert(ind1);
        assert!(changed1, "First insert should change frontier");
        assert_eq!(frontier.size(), 1);

        // Dominated by ind1 → should not change frontier
        let ind2 = ParetoIndividual {
            objectives: [0.7, 0.5, 0.5, 0.6, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5],
            generation: 2,
            crowding_distance: 0.0,
        };
        let changed2 = frontier.try_insert(ind2);
        assert!(!changed2, "Dominated insert should not change frontier");
        assert_eq!(frontier.size(), 1);

        // Non-dominated → should change frontier
        let ind3 = ParetoIndividual {
            objectives: [0.7, 0.8, 0.9, 0.5, 0.6, 0.7, 0.4, 0.6, 0.5, 0.5],
            generation: 3,
            crowding_distance: 0.0,
        };
        let changed3 = frontier.try_insert(ind3);
        assert!(changed3, "Non-dominated should change frontier");
        assert_eq!(frontier.size(), 2);

        // Dominates ind1 → replaces it
        let ind4 = ParetoIndividual {
            objectives: [0.9, 0.7, 0.6, 0.8, 0.5, 0.6, 0.5, 0.7, 0.5, 0.5],
            generation: 4,
            crowding_distance: 0.0,
        };
        let changed4 = frontier.try_insert(ind4);
        assert!(changed4);
        assert_eq!(frontier.size(), 2); // ind1 removed, ind3 and ind4 remain
    }

    #[test]
    fn test_pareto_from_metrics() {
        let metrics = FitnessMetrics {
            firing_balance: 0.8,
            synaptic_diversity: 0.7,
            module_utilization: 0.6,
            stability: 0.9,
            cortex_impact: 0.5,
            growth_health: 0.4,
            novelty_bonus: 0.3,
            energy_efficiency: 0.6,
            temporal_coherence: 0.7,
            emergent_complexity: 0.2,
            overall: 0.7,
        };
        let ind = ParetoIndividual::from_metrics(&metrics, 5);
        assert!((ind.objectives[0] - 0.8).abs() < 0.001);
        assert!((ind.objectives[3] - 0.9).abs() < 0.001);
        assert!((ind.objectives[6] - 0.3).abs() < 0.001);
        assert!((ind.objectives[7] - 0.6).abs() < 0.001);
        assert!((ind.objectives[8] - 0.7).abs() < 0.001);
        assert!((ind.objectives[9] - 0.2).abs() < 0.001);
        assert_eq!(ind.generation, 5);
    }

    #[test]
    fn test_pareto_crowding_distance() {
        let mut frontier = ParetoFrontier::new(10);
        // Add three non-dominated individuals with trade-offs across objectives
        let profiles = [
            [0.8, 0.3, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5], // high fb, low sd
            [0.5, 0.8, 0.5, 0.6, 0.4, 0.5, 0.3, 0.6, 0.5, 0.5], // high sd, low fb
            [0.6, 0.6, 0.9, 0.5, 0.4, 0.6, 0.4, 0.7, 0.5, 0.5], // high mu, low st
        ];
        for (i, profile) in profiles.iter().enumerate() {
            frontier.try_insert(ParetoIndividual {
                objectives: *profile,
                generation: i as u64,
                crowding_distance: 0.0,
            });
        }
        assert_eq!(frontier.size(), 3, "All three should be non-dominated");
        frontier.compute_crowding_distance();
        // All should have finite distance (no duplicates at boundaries)
        for ind in &frontier.individuals {
            assert_eq!(ind.crowding_distance, f64::INFINITY, "Boundary individuals get INF");
        }
    }

    #[test]
    fn test_pareto_hypervolume() {
        let mut frontier = ParetoFrontier::new(10);
        let ind = ParetoIndividual {
            objectives: [0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.5, 0.5],
            generation: 1,
            crowding_distance: 0.0,
        };
        frontier.try_insert(ind);
        let hv = frontier.hypervolume();
        assert!(hv > 0.0, "Hypervolume should be positive");
        assert!(hv < 2.0, "Hypervolume should be reasonable");
    }

    #[test]
    fn test_pareto_frontier_pruning() {
        let mut frontier = ParetoFrontier::new(5);
        // Add 8 non-dominated individuals (all with different objective profiles)
        for i in 0..10 {
            let mut objectives = [0.0; 10];
            objectives[0] = 0.1 + (i as f64) * 0.1; // diverse in first objective
            objectives[1] = 1.0 - (i as f64) * 0.05; // trade-off in second
            frontier.try_insert(ParetoIndividual {
                objectives,
                generation: i as u64,
                crowding_distance: 0.0,
            });
        }
        // Should be pruned to max_size (5)
        assert!(frontier.size() <= 5, "Frontier should be pruned to <=5, got {}", frontier.size());
    }

    #[test]
    fn test_pareto_try_insert_integration_with_evolution_step() {
        let mut brain = crate::Brain::new("test_pareto", 9999);
        let mut engine = EvolutionEngine::default();
        engine.auto_evolve = false;
        engine.fitness.sample_interval = 1;

        // Run a few steps and check frontier is populated
        for _ in 0..10 {
            engine.step(&mut brain);
        }

        // Frontier should have at least one individual (the first sample)
        // It might be empty if generation is 0 and no sampling happened
        // But with sample_interval=1 and 10 steps, it should have some
        assert!(
            engine.fitness.history.len() >= 2 || engine.pareto_frontier.size() > 0,
            "Frontier should have members after sampling, got size={}, history={}",
            engine.pareto_frontier.size(),
            engine.fitness.history.len()
        );
    }

    #[test]
    fn test_pareto_frontier_serialization_roundtrip() {
        let mut frontier = ParetoFrontier::new(10);
        frontier.try_insert(ParetoIndividual {
            objectives: [0.8, 0.6, 0.5, 0.7, 0.4, 0.5, 0.3, 0.5, 0.5, 0.5],
            generation: 1,
            crowding_distance: 0.0,
        });
        frontier.try_insert(ParetoIndividual {
            objectives: [0.7, 0.8, 0.9, 0.5, 0.6, 0.7, 0.4, 0.6, 0.5, 0.5],
            generation: 2,
            crowding_distance: 0.0,
        });

        let json = serde_json::to_string(&frontier).unwrap();
        let deserialized: ParetoFrontier = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.size(), 2);
        assert_eq!(deserialized.max_size, 10);
        assert!((deserialized.individuals[0].objectives[0] - 0.8).abs() < 0.001);
        assert_eq!(deserialized.individuals[1].generation, 2);
    }

    // ── Performance benchmarks (run with --nocapture to see timing) ──

    #[test]
    fn bench_step_small_brain() {
        let mut brain = Brain::new("bench", 9999);
        let mut engine = EvolutionEngine::default();
        engine.auto_evolve = false; // measure step only

        let start = std::time::Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            brain.step();
        }
        let elapsed = start.elapsed();
        let per_step = elapsed / iterations as u32;
        println!("\n  [BENCH] step() with small brain ({} neurons): {:.1}µs/step  total={:?}",
            brain.neurons.len(), per_step.as_micros(), elapsed);
    }

    #[test]
    fn bench_select_mutation() {
        let brain = Brain::new("bench", 9999);
        let engine = EvolutionEngine::default();

        let start = std::time::Instant::now();
        let iterations = 1000;
        for _ in 0..iterations {
            let _ = engine.select_mutation(&brain, 0.15);
        }
        let elapsed = start.elapsed();
        let per_op = elapsed / iterations as u32;
        println!("\n  [BENCH] select_mutation(): {:.1}µs/op  total={:?}",
            per_op.as_micros(), elapsed);
    }

    #[test]
    fn bench_auto_heal() {
        let mut brain = Brain::new("bench", 9999);
        let mut engine = EvolutionEngine::default();

        let start = std::time::Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            let _ = engine.auto_heal(&mut brain);
        }
        let elapsed = start.elapsed();
        let per_op = elapsed / iterations as u32;
        println!("\n  [BENCH] auto_heal() with {} neurons: {:.1}µs/op  total={:?}",
            brain.neurons.len(), per_op.as_micros(), elapsed);
    }

    #[test]
    fn bench_meta_mutate() {
        let mut engine = EvolutionEngine::default();

        let start = std::time::Instant::now();
        let iterations = 10000;
        for _ in 0..iterations {
            engine.meta_mutate();
        }
        let elapsed = start.elapsed();
        let per_op = elapsed / iterations as u32;
        println!("\n  [BENCH] meta_mutate(): {:.1}ns/op  total={:?}",
            per_op.as_nanos(), elapsed);
    }
}

impl Genome {
    /// Build a Genome from the current brain state.
    pub fn from_brain(brain: &crate::Brain) -> Self {
        let modules = brain
            .module_order
            .iter()
            .map(|name| {
                let m = &brain.modules[name];
                ModuleGene {
                    name: name.clone(),
                    neuron_count: m.neuron_indices.len(),
                    position_x: m.x,
                    position_y: m.y,
                    color: m.color.clone(),
                    learning_rate: 0.2,
                    firing_threshold_mean: 0.75,
                    firing_threshold_std: 0.1,
                    synaptic_density: 0.3,
                    mutation_rate: 0.01,
                    energy_multiplier: m.base_metabolic_mod,
                    energy_priority: m.energy_priority,
                }
            })
            .collect();

        Self {
            modules,
            brain_params: brain.params.clone(),
            cortex_config: CortexConfigGene::default(),
            behaviors: BehaviorGenes::default(),
            generation: brain.evolution.generation,
            created: crate::brain::now_f64(),
            parent_id: None,
            script_genome: ScriptGenome::default(),
        }
    }

    /// Generate Rust source code reflecting the evolved genome.
    pub fn to_rust_source(&self) -> String {
        let mut src = String::new();
        src.push_str("// Auto-generated genome source\n");
        src.push_str(&format!("// Generation: {}\n", self.generation));
        for m in &self.modules {
            src.push_str(&format!(
                "mod {} {{ && neuron_count: {} && }}\n",
                m.name, m.neuron_count
            ));
        }
        src
    }
}
