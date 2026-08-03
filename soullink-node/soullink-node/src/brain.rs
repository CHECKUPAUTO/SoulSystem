//! Brain — central simulation engine
//! Extracted from main.rs for modularity.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::prelude::*;
use crate::soulsystem_shim::MHCBlock;
use rayon::prelude::*;
use tracing::{info, warn};
use crate::neuron::Neuron;
use crate::synapse::Synapse;
use crate::brain_module::{BrainModule, Stats, SpikeInfo, MeshState, MeshPeerInfo, PainSignal, StressMode, current_period};
use crate::ssm_cortex::{SsmCortex, CortexConfig};
use crate::deepseek_cortex::{DeepSeekCortex, DeepSeekCortexConfig};
use crate::script_engine::ScriptEngine;
use crate::evolution;
use crate::evolution::EvolutionEngine;
use crate::claude_bridge::ClaudeBridge;
use crate::learning_journal::LearningJournal;
use crate::meta_cortex::MetaCortex;
use crate::self_modify::SelfModifyEngine;
/// Type alias for neuron index.
pub type NeuronIdx = usize;
/// Type alias for synapse index.
pub type SynapseIdx = usize;

#[inline]
pub fn now_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrainParams {
    /// Membrane time constant decay (applied as neuron.potential * tau + input * (1-tau))
    pub tau: f32,
    /// Homeostatic boost rate for silent neurons
    pub homeostatic_rate: f32,
    /// Synapse weight threshold for pruning
    pub prune_threshold: f32,
    /// Hebbian STDP strengthen delta
    pub stdp_learning_rate: f32,
    /// Energy capacity per neuron (max energy_reserve)
    pub energy_capacity: f32,
    /// Base metabolic cost per neuron per tick
    pub base_metabolic_rate: f32,
    /// Extra metabolic cost per firing event
    pub firing_energy_cost: f32,
    /// Cost per active synapse per tick
    pub synapse_energy_cost: f32,
    /// Energy recharge rate during rest/consolidation
    pub energy_recharge_rate: f32,
    /// Whether metabolism is enabled
    pub metabolism_enabled: bool,
    /// Whether module-level energy budget is active
    pub module_budget_enabled: bool,
    /// Fraction of neuron cost also charged to module (0.0-1.0)
    pub budget_share: f32,
    /// Whether global energy pool is active
    pub energy_pool_enabled: bool,
    /// Global pool recharge rate per tick during rest
    pub energy_pool_recharge_rate: f32,
    /// Fraction of neuron surplus collected into pool (0.0-1.0)
    pub pool_contribution: f32,
    /// Energy cost deducted per new neuron created
    pub neuron_creation_cost: f32,
    /// Energy cost deducted per new synapse created
    pub synapse_creation_cost: f32,
    /// Minimum average energy to allow growth (below this = reduced/blocked)
    pub growth_energy_threshold: f32,
    // ── Homeostasis / Arousal parameters ──
    /// Arousal decay rate per tick (0.0-1.0, default 0.995)
    pub arousal_decay: f32,
    /// Arousal sensitivity to circadian intensity (0.0-1.0, default 0.5)
    pub arousal_circadian_sensitivity: f32,
    /// Arousal sensitivity to energy depletion (0.0-1.0, default 0.4)
    pub arousal_energy_sensitivity: f32,
    /// How fast set points drift toward target (0-1, default 0.001)
    pub set_point_drift_rate: f32,
    /// Metabolic pressure integration rate (default 0.01)
    pub metabolic_pressure_rate: f32,
    /// Metabolic pressure decay rate (default 0.95)
    pub metabolic_pressure_decay: f32,
    /// Arousal effect on firing threshold (0.0-1.0, default 0.3)
    pub arousal_threshold_mod: f32,
}

impl BrainParams {
    /// Apply a delta to a named parameter.
    pub fn apply_delta(&mut self, param_name: &str, delta: f64) {
        match param_name {
            "tau" => self.tau = (self.tau as f64 + delta).clamp(0.5, 1.0) as f32,
            "homeostatic_rate" => self.homeostatic_rate = (self.homeostatic_rate as f64 + delta).clamp(0.0, 0.1) as f32,
            "prune_threshold" => self.prune_threshold = (self.prune_threshold as f64 + delta).clamp(0.0, 0.5) as f32,
            "stdp_learning_rate" => self.stdp_learning_rate = (self.stdp_learning_rate as f64 + delta).clamp(0.0, 0.1) as f32,
            "global_learning_rate" => {
                // Affects multiple params
                let lr = (self.stdp_learning_rate as f64 + delta).clamp(0.0, 0.1) as f32;
                self.stdp_learning_rate = lr;
            }
            "base_metabolic_rate" => self.base_metabolic_rate = (self.base_metabolic_rate as f64 + delta).clamp(0.0, 0.01) as f32,
            "energy_recharge_rate" => self.energy_recharge_rate = (self.energy_recharge_rate as f64 + delta).clamp(0.0, 0.1) as f32,
            "arousal_decay" => self.arousal_decay = (self.arousal_decay as f64 + delta).clamp(0.9, 1.0) as f32,
            _ => tracing::warn!("Unknown param: {param_name}"),
        }
    }
}

impl Default for BrainParams {
    fn default() -> Self {
        Self {
            tau: 0.98,
            homeostatic_rate: 0.005,
            prune_threshold: 0.05,
            stdp_learning_rate: 0.005,
            energy_capacity: 1.0,
            base_metabolic_rate: 0.0001,
            firing_energy_cost: 0.02,
            synapse_energy_cost: 0.000_01,
            energy_recharge_rate: 0.001,
            metabolism_enabled: true,
            module_budget_enabled: true,
            budget_share: 0.3,
            energy_pool_enabled: true,
            energy_pool_recharge_rate: 0.1,
            pool_contribution: 0.1,
            neuron_creation_cost: 0.05,
            synapse_creation_cost: 0.01,
            growth_energy_threshold: 0.3,
            arousal_decay: 0.995,
            arousal_circadian_sensitivity: 0.5,
            arousal_energy_sensitivity: 0.4,
            set_point_drift_rate: 0.001,
            metabolic_pressure_rate: 0.01,
            metabolic_pressure_decay: 0.95,
            arousal_threshold_mod: 0.3,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Brain {
    pub neurons: Vec<Neuron>,
    pub synapses: Vec<Synapse>,
    pub modules: HashMap<String, BrainModule>,
    pub module_order: Vec<String>,
    pub stats: Stats,
    pub learned_topics: std::collections::HashSet<String>,
    pub recent_spikes: Vec<SpikeInfo>,
    // Mesh state
    pub mesh: MeshState,
    // Node identity
    pub node_name: String,
    pub node_port: u16,
    // Evolvable brain parameters (synced from genome)
    pub params: BrainParams,
    // mHC block for Sinkhorn-constrained residual gating (not serialized)
    #[serde(skip)]
    pub mhc_block: MHCBlock,
    // Hot-loaded genome from compiled cdylib (not serialized)
    #[serde(skip)]
    pub loaded_genome: Option<crate::hotload::LoadedGenome>,
    // SSM Cortex (not serialized — rebuilt on load)
    // DeepSeek V4 Cortex — hybrid attention + mHC + Muon (not serialized)
    #[serde(skip)]
    pub deepseek_cortex: DeepSeekCortex,
    #[serde(skip)]
    pub cortex: SsmCortex,
    // Script engine for Rhai-based self-modification (not serialized)
    #[serde(skip)]
    pub script_engine: ScriptEngine,
    /// Meta-cortex (not serialized)
    #[serde(skip)]
    pub meta_cortex: MetaCortex,
    #[serde(skip)]
    pub self_modify_engine: SelfModifyEngine,
    #[serde(skip)]
    pub llm_mutator: Option<Box<crate::llm_mutator::LlmMutator>>,
    /// Claude Code bridge (not serialized)
    #[serde(skip)]
    pub claude_bridge: ClaudeBridge,
    /// Persistent learning journal (not serialized)
    #[serde(skip)]
    pub journal: Option<LearningJournal>,
    /// Governance bridge (not serialized)
    #[serde(skip)]
    pub governance_bridge: Option<crate::governance_bridge::GovernanceBridge>,
    /// Chronos temporal context (not serialized)
    #[serde(skip)]
    pub chronos_context: Option<serde_json::Value>,
    // Evolution Engine — serialized for persistence across restarts
    pub evolution: EvolutionEngine,
    // Behavioral genes — synced from genome, NOT serialized separately
    #[serde(skip)]
    pub behavior: evolution::BehaviorGenes,
    // Pain signal — composite vital sign (computed each tick, not serialized)
    #[serde(skip)]
    pub pain: PainSignal,
    // Stress mode — derived from pain (computed each tick, not serialized)
    #[serde(skip)]
    pub stress_mode: StressMode,
    // Global energy pool — system-level reserve shared across all neurons
    #[serde(skip)]
    pub energy_pool: f32,
    #[serde(skip)]
    pub energy_pool_capacity: f32,
    // ── Homeostasis runtime state (not serialized) ──
    #[serde(skip)]
    pub arousal: f32,
    #[serde(skip)]
    pub metabolic_pressure: f32,
    #[serde(skip)]
    pub death_clock: u64,
    #[serde(skip)]
    pub death_triggered: bool,
    #[serde(skip)]
    pub death_consolidation_ticks: u64,
    #[serde(skip)]
    pub sos_sent: bool,
    // ── Pre-allocated scratch buffers (performance, not serialized) ──
    #[serde(skip)]
    pub(crate) scratch_activations: Vec<f32>,
    #[serde(skip)]
    pub(crate) scratch_synaptic_inputs: Vec<f32>,
    #[serde(skip)]
    pub(crate) scratch_ext_inputs: Vec<f32>,
    #[serde(skip)]
    pub(crate) scratch_fired_flags: Vec<bool>,
    #[serde(skip)]
    pub(crate) scratch_fired: Vec<NeuronIdx>,
    /// Activation history for intrinsic curiosity (not serialized)
    #[serde(skip)]
    pub activation_history: std::collections::VecDeque<Vec<f64>>,
    // ── Stress modulation scalars (recomputed each tick) ──
    #[serde(skip)]
    pub(crate) stress_firing_mod: f32,
    #[serde(skip)]
    pub(crate) stress_prune_mod: f32,
    #[serde(skip)]
    pub(crate) stress_stdp_mod: f32,
    #[serde(skip)]
    pub(crate) stress_inhibition_mod: f32,
}

/// Thread-safe shared brain reference for API handlers.
pub type SharedBrain = std::sync::Arc<tokio::sync::RwLock<Brain>>;

/// Gossip message for mesh peer-to-peer communication.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GossipMessage {
    /// "heartbeat", "evolution", "peer_discovery"
    pub msg_type: String,
    pub source_port: u16,
    pub source_name: String,
    /// Time-to-live — decremented at each hop, dropped at 0
    pub ttl: u8,
    pub payload: serde_json::Value,
}

impl Brain {
    pub fn new(name: &str, port: u16) -> Self {
        let mut brain = Self {
            neurons: Vec::with_capacity(32_768),
            synapses: Vec::with_capacity(131_072),
            modules: HashMap::new(),
            module_order: Vec::new(),
            stats: Stats::default(),
            learned_topics: std::collections::HashSet::new(),
            recent_spikes: Vec::new(),
            mesh: MeshState::default(),
            node_name: name.to_string(),
            node_port: port,
            params: BrainParams::default(),
            mhc_block: MHCBlock::default(),
            loaded_genome: None,
            cortex: SsmCortex::new(CortexConfig::default()),
            deepseek_cortex: DeepSeekCortex::new(DeepSeekCortexConfig::default()),
            script_engine: ScriptEngine::new(),
            meta_cortex: MetaCortex::new(),
            self_modify_engine: SelfModifyEngine::default(),
            llm_mutator: Some(Box::new(crate::llm_mutator::LlmMutator::new(crate::llm_mutator::LlmMutatorConfig::default()))),
            claude_bridge: ClaudeBridge::new(),
            journal: LearningJournal::open("learning_journal.jsonl").ok(),
            governance_bridge: Some(crate::governance_bridge::GovernanceBridge::new()),
            chronos_context: None,
            evolution: EvolutionEngine::default(),
            behavior: evolution::BehaviorGenes::default(),
            pain: PainSignal::default(),
            stress_mode: StressMode::Calm,
            energy_pool: 100.0,
            energy_pool_capacity: 100.0,
            arousal: 0.5,
            metabolic_pressure: 0.0,
            death_clock: 0,
            death_triggered: false,
            death_consolidation_ticks: 0,
            sos_sent: false,
            scratch_activations: Vec::with_capacity(32_768),
            scratch_synaptic_inputs: Vec::with_capacity(32_768),
            scratch_ext_inputs: Vec::with_capacity(16),
            scratch_fired_flags: Vec::with_capacity(32_768),
            scratch_fired: Vec::with_capacity(32_768),
            activation_history: std::collections::VecDeque::with_capacity(100),
            stress_firing_mod: 1.0,
            stress_prune_mod: 1.0,
            stress_stdp_mod: 1.0,
            stress_inhibition_mod: 1.0,
        };
        brain.init_modules();
        {
            let e = std::mem::take(&mut brain.evolution);
            e.apply_global_params(&mut brain);
            brain.evolution = e;
        }
        brain
    }

    /// Learn from arbitrary text — ingest knowledge into learned_topics.
    pub fn learn_from_text(&mut self, text: &str) {
        for word in text.split_whitespace() {
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();
            if cleaned.len() > 2 {
                self.learned_topics.insert(cleaned.to_lowercase());
            }
        }
    }

    /// Accessor for the persistent learning journal.
    pub fn journal(&mut self) -> Option<&mut LearningJournal> {
        self.journal.as_mut()
    }

    // ─────────────────────────────────────────────────────────
    // Initialisation — 30,000 neurons (10 modules × 3000) + extra
    // ─────────────────────────────────────────────────────────

    fn init_modules(&mut self) {
        let configs: &[(&str, usize, f32, f32, &str)] = &[
            ("perception",  3000, 150.0,  100.0, "#4affb8"),
            ("memory",      3000, 350.0,   80.0, "#4a9eff"),
            ("reasoning",   3000, 550.0,  100.0, "#ff4a6a"),
            ("learning",    3000, 200.0,  220.0, "#ffaa4a"),
            ("output",      3000, 400.0,  220.0, "#9bff4a"),
            ("attention",   3000, 600.0,  220.0, "#ff4aff"),
            ("language",    3000, 150.0,  350.0, "#4affff"),
            ("vision",      3000, 350.0,  350.0, "#ff4a4a"),
            ("audio",       3000, 550.0,  350.0, "#4a4aff"),
            ("motor",       3000, 400.0,  420.0, "#ffff4a"),
            // 7th Brain — OpenEvolve
            ("extra",       3000, 250.0,  500.0, "#ff00ff"),
        ];

        for (name, n, x, y, color) in configs {
            let module = BrainModule::new(name.to_string(), *x, *y, color.to_string());
            self.modules.insert(name.to_string(), module);
            self.module_order.push(name.to_string());
            for _ in 0..*n {
                self.add_neuron_to(name);
            }
        }

        self.create_initial_connections();
        self.refresh_stats();
    }

    pub(crate) fn add_neuron_to(&mut self, module_name: &str) -> Option<NeuronIdx> {
        let (mx, my) = self.modules.get(module_name).map(|m| (m.x, m.y))?;
        let count = self.modules[module_name].neuron_indices.len();
        let mut rng = rand::rng();

        let x = mx + rng.random_range(-60.0..60.0_f32);
        let y = my + rng.random_range(-40.0..40.0_f32);
        let pfx = &module_name[..module_name.len().min(4)];
        let id = format!("{pfx}__{count:05}");

        let neuron = Neuron::new(id, module_name.to_string(), x, y);
        let idx = self.neurons.len();
        self.neurons.push(neuron);
        self.modules.get_mut(module_name)?.neuron_indices.push(idx);
        Some(idx)
    }

    fn create_initial_connections(&mut self) {
        let names = self.module_order.clone();
        let n = names.len();
        let mut rng = rand::rng();

        for i in 0..n {
            for offset in [0usize, 1, 2] {
                let j = (i + offset) % n;
                let src_neurons = self.modules[&names[i]].neuron_indices.clone();
                let tgt_neurons = self.modules[&names[j]].neuron_indices.clone();
                if src_neurons.is_empty() || tgt_neurons.is_empty() { continue; }

                // Scale initial connections for 30k neurons
                let conn_count = (3.min(src_neurons.len())).min(30);
                for _ in 0..conn_count {
                    let src = *src_neurons.choose(&mut rng).unwrap();
                    let tgt = *tgt_neurons.choose(&mut rng).unwrap();
                    self.add_synapse(src, tgt);
                }
            }
        }
    }

    pub(crate) fn add_synapse(&mut self, source: NeuronIdx, target: NeuronIdx) -> SynapseIdx {
        let idx = self.synapses.len();
        self.synapses.push(Synapse::new(source, target));
        self.neurons[source].connections_out.push(idx);
        self.neurons[target].connections_in.push(idx);
        idx
    }

    // ─────────────────────────────────────────────────────────
    // Step de simulation — parallel neuron updates with Rayon
    // ─────────────────────────────────────────────────────────

    pub fn step(&mut self) {
        let period = current_period();
        let mut rng = rand::rng();
        let tau = self.params.tau;
        let homeostatic_rate = self.params.homeostatic_rate;
        let stdp_lr = self.params.stdp_learning_rate;

        // 1. Snapshot des activations (double-buffer pattern) — reuse scratch buffer
        let n = self.neurons.len();
        self.scratch_activations.clear();
        self.scratch_activations.extend(self.neurons.iter().map(|n| n.activation));
        // Ensure capacity for the fast path
        if self.scratch_activations.len() < n {
            self.scratch_activations.resize(n, 0.0);
        }

        // 2. Pre-compute synaptic inputs per neuron — reuse scratch buffer
        self.scratch_synaptic_inputs.clear();
        self.scratch_synaptic_inputs.resize(n, 0.0);
        for syn in &self.synapses {
            if syn.active {
                self.scratch_synaptic_inputs[syn.target] += syn.weight * self.scratch_activations[syn.source];
            }
        }

        // 3. Pre-compute external inputs per module — use Vec indexed by module_order
        self.scratch_ext_inputs.clear();
        self.scratch_ext_inputs.reserve(self.module_order.len());
        for _name in &self.module_order {
            let ext = rng.random_range(0.05..0.25_f32) * period.intensity;
            self.scratch_ext_inputs.push(ext);
        }

        // 4. Parallel neuron updates with Rayon — reuse scratch buffers
        self.scratch_fired_flags.clear();
        self.scratch_fired_flags.resize(n, false);
        let _activations = &self.scratch_activations;
        let synaptic_inputs = &self.scratch_synaptic_inputs;
        let ext_inputs = &self.scratch_ext_inputs;
        let module_order = &self.module_order;

        // Build module→index lookup once (reused across all neurons)
        let mut module_to_idx: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::with_capacity(module_order.len());
        for (idx, name) in module_order.iter().enumerate() {
            module_to_idx.insert(name.as_str(), idx);
        }

        let _fired_count = 0usize;
        // Extract stress firing mod before parallel section (borrow checker)
        // Compose with arousal: low arousal raises threshold, high arousal lowers it
        let arousal_effect = (self.arousal - 0.5) * 2.0 * self.params.arousal_threshold_mod;
        let firing_mod = self.stress_firing_mod * (1.0 - arousal_effect);
        // Use a local Vec for the parallel part, then copy back to scratch_fired
        let local_fired: Vec<bool> = self.neurons
            .par_iter_mut()
            .enumerate()
            .map(|(i, neuron)| {
                let syn_in = synaptic_inputs[i];
                let ext_idx = module_to_idx.get(neuron.module.as_str()).copied().unwrap_or(0);
                let ext = ext_inputs.get(ext_idx).copied().unwrap_or(0.05);
                neuron.update(syn_in, ext, tau);
                let effective_threshold = neuron.firing_threshold * firing_mod;
                if neuron.potential > effective_threshold {
                    neuron.fire();
                    true
                } else {
                    false
                }
            })
            .collect();

        // Copy parallel results into scratch_fired_flags and build fired list
        for (i, fired) in local_fired.into_iter().enumerate() {
            if fired {
                self.scratch_fired_flags[i] = true;
                self.scratch_fired.push(i);
            }
        }

        let fired: &[NeuronIdx] = &self.scratch_fired;

        // 5b. Lateral inhibition — active neurons suppress peers in same module
        if self.behavior.lateral_inhibition > 0.01 {
            let inhibition_strength = self.behavior.lateral_inhibition * self.stress_inhibition_mod;
            for module in self.modules.values() {
                let fired_in_module: Vec<usize> = module.neuron_indices.iter()
                    .filter(|&&i| i < self.neurons.len() && self.scratch_fired_flags[i])
                    .copied()
                    .collect();
                if fired_in_module.is_empty() { continue; }
                let inhib = inhibition_strength / fired_in_module.len() as f32;
                for &ni in &module.neuron_indices {
                    if ni < self.neurons.len() && !self.scratch_fired_flags[ni] {
                        self.neurons[ni].potential = (self.neurons[ni].potential - inhib * 0.1).max(0.0);
                    }
                }
            }
        }

        // 6.0 mHC-gated learning rate: Sinkhorn-constrained residual modulates STDP
        let mhc_lr_mod: f32 = {
            let state_vec: Vec<f32> = self.module_order.iter()
                .filter_map(|name| self.modules.get(name))
                .map(|m| m.activation_level)
                .collect();
            let _resid: Vec<f32> = state_vec.iter().map(|&x| (x * 0.1 - 0.05).tanh()).collect();
            // Use MHC alpha/beta/gate for gated learning rate modulation
            let gate = self.mhc_block.gate.clamp(0.5, 1.5);
            let alpha_factor = self.mhc_block.alpha * 0.1;
            let mean_gate: f32 = state_vec.iter().sum::<f32>() / state_vec.len().max(1) as f32;
            // Combine MHC gate with residuals to produce a learning rate modifier
            (gate * alpha_factor * mean_gate * 0.1 + 0.85).clamp(0.5, 1.5) // scale to 0.5-1.5x
        };

        // 6a. STDP eligibility — decay all traces, then bump for fired pre-synaptic neurons
        // Extract stress STDP mod before the loop
        let stdp_stress_mod = self.stress_stdp_mod;
        for syn in self.synapses.iter_mut() {
            syn.decay_eligibility();
        }
        for &fi in fired {
            let outs: &[SynapseIdx] = &self.neurons[fi].connections_out;
            for &si in outs {
                let syn = &mut self.synapses[si];
                syn.bump_eligibility();
                // Also apply classical Hebbian strengthening
                let tgt = syn.target;
                if self.neurons[tgt].firing_count > 0 {
                    syn.strengthen(stdp_lr * period.intensity * stdp_stress_mod * mhc_lr_mod);
                }
            }
        }

        // 7. Plasticité homéostatique : booster les neurones trop silencieux
        for neuron in self.neurons.iter_mut() {
            if neuron.firing_count == 0 && rng.random_bool(homeostatic_rate as f64) {
                neuron.potential = (neuron.potential + 0.05).min(0.95);
            }
        }


        // 7b. Métabolisme énergétique
        if self.params.metabolism_enabled {
            let base_cost = self.params.base_metabolic_rate;
            let fire_cost = self.params.firing_energy_cost;
            let synapse_cost = self.params.synapse_energy_cost;
            let recharge_rate = self.params.energy_recharge_rate;
            let capacity = self.params.energy_capacity;
            let is_rest = period.mode == "consolidate";

            let active_syn_count = self.synapses.iter().filter(|s| s.active).count() as f32;
            let neuron_count = self.neurons.len() as f32;

            for neuron in self.neurons.iter_mut() {
                // Cost: base metabolism + firing cost (if fired this tick)
                let cost = base_cost
                    + if neuron.last_fired > 0.0 && (now_f64() - neuron.last_fired) < 0.1 {
                        fire_cost
                    } else {
                        0.0
                    };
                // Synapse cost: average per neuron
                let total_cost = cost + synapse_cost * (active_syn_count / neuron_count);
                neuron.energy_reserve = (neuron.energy_reserve - total_cost).clamp(0.0, capacity);

                // Recharge during rest/consolidation
                if is_rest {
                    neuron.energy_reserve = (neuron.energy_reserve + recharge_rate).min(capacity);
                }

                // Starvation mode: low energy → reduced firing threshold
                if neuron.energy_reserve < 0.1 {
                    neuron.potential *= 0.98; // metabolic depression
                }
            }
        }

        // 7c. Module-level energy budget (second pass)
        if self.params.metabolism_enabled && self.params.module_budget_enabled {
            let budget_share = self.params.budget_share;
            let stress_threshold = 0.7;
            let stress_decay = 0.95;

            // Decay local_stress for all modules, reset per-tick tracking
            for module in self.modules.values_mut() {
                module.energy_reserve = 0.0;
                module.energy_budget = 1.0;
                module.local_stress *= stress_decay;
            }

            // Aggregate per-module energy cost from neurons
            let mod_base_cost = self.params.base_metabolic_rate;
            let mod_fire_cost = self.params.firing_energy_cost;
            for neuron in self.neurons.iter() {
                let cost = mod_base_cost
                    + if neuron.last_fired > 0.0 && (now_f64() - neuron.last_fired) < 0.1 {
                        mod_fire_cost
                    } else {
                        0.0
                    };
                let module_cost = cost * budget_share;
                if let Some(module) = self.modules.get_mut(&neuron.module) {
                    module.energy_reserve += module_cost;
                    if module.energy_reserve > module.energy_budget {
                        module.local_stress = (module.local_stress + 0.01).min(1.0);
                    }
                }
            }

            // Apply module-level depression where local stress is high
            let stress_indices: Vec<NeuronIdx> = self.modules.values()
                .filter(|m| m.local_stress > stress_threshold)
                .flat_map(|m| m.neuron_indices.iter().copied())
                .collect();
            for &ni in &stress_indices {
                if let Some(n) = self.neurons.get_mut(ni) {
                    n.potential *= 0.98;
                }
            }
        }

        // 7d. Global energy pool — reserve sharing across all neurons
        if self.params.metabolism_enabled && self.params.energy_pool_enabled {
            let pool_recharge = self.params.energy_pool_recharge_rate;
            let contribution = self.params.pool_contribution;
            let capacity = self.energy_pool_capacity;

            // Recharge pool during rest/consolidation
            if period.mode == "consolidate" {
                self.energy_pool = (self.energy_pool + pool_recharge).min(capacity);
            }

            // Collect surplus from well-fed neurons
            let mut collected = 0.0;
            for neuron in self.neurons.iter_mut() {
                if neuron.energy_reserve > 0.3 {
                    let take = (neuron.energy_reserve - 0.3) * contribution;
                    neuron.energy_reserve -= take;
                    collected += take;
                }
            }
            self.energy_pool = (self.energy_pool + collected).min(capacity);

            // Emergency relief: if pool > 50% and neurons are starving
            if self.energy_pool > capacity * 0.5 {
                let debtor_count = self.neurons.iter().filter(|n| n.energy_reserve < 0.1).count();
                if debtor_count > 0 {
                    let relief = (self.energy_pool * 0.01).min(capacity * 0.001);
                    let per_neuron = relief / debtor_count.max(1) as f32;
                    for neuron in self.neurons.iter_mut() {
                        if neuron.energy_reserve < 0.1 {
                            neuron.energy_reserve = (neuron.energy_reserve + per_neuron).min(1.0);
                        }
                    }
                    self.energy_pool -= relief.min(self.energy_pool);
                }
            }

            // Update stats
            self.stats.energy_pool_level = (self.energy_pool / capacity).clamp(0.0, 1.0);
        }

        // 7e. Active homeostasis: arousal, metabolic pressure, set-point drift
        if self.params.metabolism_enabled {
            let energy_avg = if !self.neurons.is_empty() {
                self.neurons.iter().map(|n| n.energy_reserve).sum::<f32>() / self.neurons.len() as f32
            } else { 0.5 };

            // Arousal from circadian drive + energy level
            let circadian = period.intensity * self.params.arousal_circadian_sensitivity;
            let energy_drive = energy_avg * self.params.arousal_energy_sensitivity;
            let target_arousal = (circadian + energy_drive).clamp(0.1, 0.9);
            self.arousal = self.arousal * self.params.arousal_decay
                + target_arousal * (1.0 - self.params.arousal_decay);
            self.arousal = self.arousal.clamp(0.0, 1.0);

            // Metabolic pressure: builds when energy is negative, decays otherwise
            let total_cost: f32 = self.neurons.iter()
                .map(|n| self.params.base_metabolic_rate
                    + if n.last_fired > 0.0 && (now_f64() - n.last_fired) < 0.1 { self.params.firing_energy_cost } else { 0.0 })
                .sum();
            let total_income: f32 = self.neurons.iter()
                .map(|n| n.energy_reserve.min(self.params.energy_recharge_rate))
                .sum();
            let net_flow = (total_income - total_cost) / self.neurons.len().max(1) as f32;
            if net_flow < -0.1 {
                self.metabolic_pressure = (self.metabolic_pressure
                    + self.params.metabolic_pressure_rate * (-net_flow)).min(1.0);
            } else {
                self.metabolic_pressure *= self.params.metabolic_pressure_decay;
            }

            // Set-point drift: when not stressed, params drift toward evolved targets
            if self.stress_mode == StressMode::Calm || self.stress_mode == StressMode::Alert {
                let drift = self.params.set_point_drift_rate;
                let gp = &self.evolution.genome.brain_params;
                self.params.tau += (gp.tau - self.params.tau) * drift;
                self.params.homeostatic_rate += (gp.homeostatic_rate - self.params.homeostatic_rate) * drift;
                self.params.prune_threshold += (gp.prune_threshold - self.params.prune_threshold) * drift;
                self.params.stdp_learning_rate += (gp.stdp_learning_rate - self.params.stdp_learning_rate) * drift;
            }

            // Update stats
            self.stats.arousal_display = (self.arousal * 1000.0) as u32;
            self.stats.metabolic_pressure_display = (self.metabolic_pressure * 1000.0) as u32;
        }

        // 8. Mise à jour activation_level des modules
        for module in self.modules.values_mut() {
            if module.neuron_indices.is_empty() { continue; }
            module.activation_level = module.neuron_indices.iter()
                .map(|&i| self.scratch_activations[i])
                .sum::<f32>() / module.neuron_indices.len() as f32;
        }

        // 9. Spikes récents (fenêtre 300 ms)
        let now = now_f64();
        self.recent_spikes.clear();
        self.recent_spikes.reserve(fired.len());
        for &i in fired {
            self.recent_spikes.push(SpikeInfo {
                id:   self.neurons[i].id.clone(),
                x:    self.neurons[i].x,
                y:    self.neurons[i].y,
                time: self.neurons[i].spike_time,
            });
        }
        self.recent_spikes.retain(|s| now - s.time < 0.3);

        self.stats.learning_cycles += 1;
        self.stats.firing_neurons = fired.len();
        self.stats.spikes_count = self.recent_spikes.len();
        self.refresh_stats();
        // Apply Rhai scripted activations (sequential, after parallel update)
        self.apply_scripted_activations();
        // Apply Rhai scripted plasticity to active synapses
        if self.script_engine.plasticity_script.is_active() {
            self.apply_scripted_plasticity(period.intensity);
        }
        // Record activation history for intrinsic curiosity
        let activation_vec: Vec<f64> = self.neurons.iter().map(|n| n.activation as f64).collect();
        self.activation_history.push_back(activation_vec);
        if self.activation_history.len() > 100 {
            self.activation_history.pop_front();
        }
        self.meta_cortex.set_novelty(self.compute_novelty_score());

        // Clear scratch buffers for reuse
        self.scratch_fired.clear();
        self.scratch_fired_flags.clear();
        // Reset firing counts for next tick
        for neuron in self.neurons.iter_mut() {
            neuron.firing_count = 0;
        }
    }


    /// Compute intrinsic curiosity novelty score based on activation history.
    /// Returns average cosine distance to the 10 previous activation vectors.
    pub fn compute_novelty_score(&self) -> f64 {
        if self.activation_history.len() < 2 {
            return 1.0;
        }
        let last = self.activation_history.back().unwrap();
        let start = self.activation_history.len().saturating_sub(11);
        let mut total_dist = 0.0;
        let mut count = 0;
        for i in start..self.activation_history.len() - 1 {
            let prev = &self.activation_history[i];
            let dot = last.iter().zip(prev.iter()).map(|(a, b)| a * b).sum::<f64>();
            let norm_a = last.iter().map(|a| a * a).sum::<f64>().sqrt();
            let norm_b = prev.iter().map(|b| b * b).sum::<f64>().sqrt();
            if norm_a > 1e-10 && norm_b > 1e-10 {
                let cos_dist = 1.0 - dot / (norm_a * norm_b);
                total_dist += cos_dist.clamp(0.0, 1.0);
                count += 1;
            }
        }
        if count == 0 {
            return 1.0;
        }
        (total_dist / count as f64).clamp(0.0, 1.0)
    }
    /// Apply scripted activation overrides for modules with active Rhai scripts.
    /// Runs sequentially after the parallel neuron update.
    fn apply_scripted_activations(&mut self) {
        // Collect active script module names first to avoid double borrow
        let active_modules: Vec<String> = self.script_engine.activation_scripts.iter()
            .filter(|(_, s)| s.is_active())
            .map(|(name, _)| name.clone())
            .collect();

        if active_modules.is_empty() {
            return;
        }

        let tau = self.params.tau;
        let noise = self.behavior.noise_level;

        for module_name in &active_modules {
            let neuron_indices: Vec<usize> = self.modules.get(module_name.as_str())
                .map(|m| m.neuron_indices.clone())
                .unwrap_or_default();

            for &ni in &neuron_indices {
                if ni >= self.neurons.len() {
                    continue;
                }
                let (potential, threshold, reserve, activation, mut ext) = {
                    let n = &self.neurons[ni];
                    (n.potential, n.firing_threshold, n.energy_reserve, n.activation, n.extensions.clone())
                };
                let new_activation = self.script_engine.eval_activation(
                    module_name,
                    potential,
                    tau,
                    threshold,
                    reserve,
                    activation,
                    noise,
                    &mut ext,
                );
                if let Some(n) = self.neurons.get_mut(ni) {
                    n.activation = new_activation;
                    n.extensions = ext;
                }
            }
        }
    }

    /// Apply scripted plasticity to synapses connected to recently fired neurons.
    fn apply_scripted_plasticity(&mut self, intensity: f32) {
        let lr = self.params.stdp_learning_rate * intensity;
        let activations = &self.scratch_activations;

        for &fi in &self.scratch_fired {
            let outs: &[SynapseIdx] = &self.neurons[fi].connections_out;
            for &si in outs {
                let syn = &self.synapses[si];
                if !syn.active {
                    continue;
                }
                let pre = activations.get(syn.source).copied().unwrap_or(0.0);
                let post = activations.get(syn.target).copied().unwrap_or(0.0);
                let new_w = self.script_engine.eval_plasticity(
                    syn.weight, pre, post, syn.eligibility, lr,
                );
                if new_w.is_finite() {
                    self.synapses[si].weight = new_w.clamp(0.0, 1.0);
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────
    // Consolidation (renforcement, pas destruction)
    // ─────────────────────────────────────────────────────────

    pub fn consolidate(&mut self, intensity: f32) {
        let mut events = 0u64;
        let n = self.neurons.len();

        for i in 0..n {
            if self.neurons[i].importance > 0.4 {
                self.neurons[i].consolidate(intensity);
                events += 1;
            }
        }

        for si in 0..self.synapses.len() {
            if !self.synapses[si].active { continue; }
            let (src, tgt) = (self.synapses[si].source, self.synapses[si].target);
            if self.neurons[src].firing_count > 0 && self.neurons[tgt].firing_count > 0 {
                self.synapses[si].strengthen(0.02 * intensity);
            }
        }

        // Croissance même pendant consolidation
        if rand::rng().random_bool(0.3 * intensity as f64) {
            self.add_neuron_to_important_module();
        }

        self.stats.consolidation_events += events;
    }

    /// Reward-based consolidation using STDP eligibility traces.
    ///
    /// Applies a global reward signal (typically from the SSM cortex) to all
    /// synapses proportionally to their eligibility trace. This implements
    /// a three-factor learning rule: pre-synaptic spike × post-synaptic
    /// activation × reward signal.
    ///
    /// `signal` is a global reward value (negative = punishment, positive = reward).
    /// Returns the total weight change applied.
    pub fn reward_consolidate(&mut self, signal: f32) -> f32 {
        if signal.abs() < 0.001 {
            return 0.0;
        }
        let mut total_delta = 0.0;
        for syn in self.synapses.iter_mut() {
            if !syn.active { continue; }
            total_delta += syn.reward(signal);
        }
        self.stats.consolidation_events += 1;
        total_delta
    }

    // ─────────────────────────────────────────────────────────
    // Croissance
    // ─────────────────────────────────────────────────────────

    pub fn grow(&mut self, intensity: f32) {
        // Energy gate: don't grow if energy is too low
        if self.params.metabolism_enabled {
            let avg_energy = if !self.neurons.is_empty() {
                self.neurons.iter().map(|n| n.energy_reserve).sum::<f32>() / self.neurons.len() as f32
            } else { 1.0 };
            let threshold = self.params.growth_energy_threshold;
            if avg_energy < threshold {
                let penalty = (avg_energy / threshold).max(0.05);
                let effective = intensity * penalty;
                if effective < 0.02 { return; }
                let new_n = (effective * 2.0) as usize + 1;
                let new_s = (effective * 3.0) as usize;
                return self.grow_core(new_n, new_s);
            }
        }

        let mut rng = rand::rng();
        let new_n = (intensity * 2.0) as usize + rng.random_range(0..=2);
        let new_s = (intensity * 3.0) as usize;
        self.grow_core(new_n, new_s);
    }

    /// Core growth logic with energy deduction.
    fn grow_core(&mut self, new_n: usize, new_s: usize) {
        // Deduct creation cost from energy pool first, then from neurons
        if self.params.metabolism_enabled {
            let neuron_cost = self.params.neuron_creation_cost;
            let synapse_cost = self.params.synapse_creation_cost;
            let total_cost = new_n as f32 * neuron_cost + new_s as f32 * synapse_cost;

            if self.params.energy_pool_enabled && self.energy_pool >= total_cost {
                self.energy_pool -= total_cost;
            } else if total_cost > 0.0 {
                let per_neuron = total_cost / self.neurons.len().max(1) as f32;
                for neuron in self.neurons.iter_mut() {
                    neuron.energy_reserve = (neuron.energy_reserve - per_neuron).max(0.0);
                }
            }
        }

        for _ in 0..new_n { self.add_neuron_to_important_module(); }
        for _ in 0..new_s { self.create_random_synapse(); }
        self.stats.growth_events += new_n as u64;
    }

    fn add_neuron_to_important_module(&mut self) {
        let mut rng = rand::rng();
        let total: f32 = self.modules.values().map(|m| m.importance).sum();
        let names = self.module_order.clone();

        if total <= 0.0 {
            if let Some(name) = names.first() { self.add_neuron_to(name); }
            return;
        }

        let r: f32 = rng.random_range(0.0..total);
        let mut cum = 0.0f32;
        for name in &names {
            cum += self.modules[name].importance;
            if r <= cum {
                self.add_neuron_to(name);
                if let Some(m) = self.modules.get_mut(name) {
                    m.importance = (m.importance + 0.01).min(1.0);
                }
                return;
            }
        }
    }

    fn create_random_synapse(&mut self) {
        let mut rng = rand::rng();
        if self.neurons.len() < 2 { return; }
        let src = rng.random_range(0..self.neurons.len());
        let tgt = rng.random_range(0..self.neurons.len());
        if src != tgt { self.add_synapse(src, tgt); }
    }

    // ─────────────────────────────────────────────────────────
    // Élagage des synapses faibles
    // ─────────────────────────────────────────────────────────

    pub fn prune_weak_synapses(&mut self, intensity_mod: f32) {
        let mut pruned = 0u64;
        let threshold = self.params.prune_threshold * intensity_mod;
        // Build set of high-importance neurons (never prune their synapses)
        let protected: std::collections::HashSet<usize> = self.modules.values()
            .flat_map(|m| m.neuron_indices.iter())
            .copied()
            .filter(|&i| i < self.neurons.len() && self.neurons[i].importance > 0.8)
            .collect();
        for syn in self.synapses.iter_mut() {
            if syn.active && syn.weight < threshold {
                if protected.contains(&syn.source) || protected.contains(&syn.target) {
                    continue;
                }
                syn.active = false;
                pruned += 1;
            }
        }
        self.stats.pruned_synapses += pruned;
    }

    // ─────────────────────────────────────────────────────────
    // Apprentissage d'un topic
    // ─────────────────────────────────────────────────────────

    pub fn learn_topic(&mut self, topic: &str) -> bool {
        if self.learned_topics.contains(topic) { return false; }
        let mut rng = rand::rng();

        for _ in 0..rng.random_range(2..=5) { self.add_neuron_to_important_module(); }
        for _ in 0..rng.random_range(3..=8) { self.create_random_synapse(); }

        self.learned_topics.insert(topic.to_string());
        self.stats.topics_learned = self.learned_topics.len();
        self.refresh_stats();
        true
    }

    // ─────────────────────────────────────────────────────────
    // Evolve endpoint — reinforce extra module neurons
    // ─────────────────────────────────────────────────────────

    pub fn evolve(&mut self, fitness: f64, proposals: &[String]) -> usize {
        let extra_module = match self.modules.get("extra") {
            Some(m) => m,
            None => return 0,
        };

        let extra_indices = extra_module.neuron_indices.clone();
        let boost = (fitness as f32).clamp(0.0, 1.0) * 0.1;

        for &idx in &extra_indices {
            self.neurons[idx].importance = (self.neurons[idx].importance + boost).min(1.0);
            self.neurons[idx].strength = (self.neurons[idx].strength + boost * 0.5).min(1.0);
        }

        // Add new neurons and synapses for each proposal
        for proposal in proposals {
            if !proposal.is_empty() {
                self.add_neuron_to("extra");
                // Create a few connections for the new neuron
                let new_idx = self.neurons.len() - 1;
                let mut rng = rand::rng();
                for _ in 0..3 {
                    let tgt = rng.random_range(0..self.neurons.len());
                    if new_idx != tgt {
                        self.add_synapse(new_idx, tgt);
                    }
                }
            }
        }

        extra_indices.len()
    }

    // ─────────────────────────────────────────────────────────
    // Sérialisation pour l'API
    // ─────────────────────────────────────────────────────────

    pub(crate) fn refresh_stats(&mut self) {
        self.stats.total_neurons  = self.neurons.len();
        self.stats.total_synapses = self.synapses.len();
        self.stats.active_synapses = self.synapses.iter().filter(|s| s.active).count();

        // Energy stats
        if !self.neurons.is_empty() {
            let sum: f32 = self.neurons.iter().map(|n| n.energy_reserve).sum();
            self.stats.energy_avg = sum / self.neurons.len() as f32;
            self.stats.energy_debt_count = self.neurons.iter().filter(|n| n.energy_reserve < 0.1).count();
            if self.stats.energy_debt_count > self.neurons.len() / 2 {
                self.stats.energy_deficit_ticks += 1;
            }
        }
    }

    pub fn api_status(&self) -> serde_json::Value {
        let period = current_period();
        let modules: serde_json::Map<_, _> = self.modules.iter().map(|(name, m)| {
            let v = serde_json::json!({
                "neurons":    m.neuron_indices.len(),
                "activation": (m.activation_level * 1000.0).round() / 1000.0,
                "importance": (m.importance * 1000.0).round() / 1000.0,
                "x":          m.x,
                "y":          m.y,
                "color":      m.color,
            });
            (name.clone(), v)
        }).collect();

        let mesh_peers: serde_json::Map<_, _> = self.mesh.peers.iter().map(|(port, info)| {
            (port.to_string(), serde_json::json!({
                "name": info.name,
                "last_seen": info.last_seen,
                "generation": info.generation,
                "fitness": (info.fitness * 1000.0).round() / 1000.0,
                "hypervolume": (info.frontier_hypervolume * 1000.0).round() / 1000.0,
                "is_alive": info.is_alive,
            }))
        }).collect();

        serde_json::json!({
            "name":       self.node_name,
            "port":       self.node_port,
            "stats":      self.stats,
            "period":     period.name,
            "intensity":  period.intensity,
            "mode":       period.mode,
            "metabolism": {
                "enabled":      self.params.metabolism_enabled,
                "energy_avg":   (self.stats.energy_avg * 1000.0).round() / 1000.0,
                "debt_count":   self.stats.energy_debt_count,
                "debt_ticks":   self.stats.energy_deficit_ticks,
                "capacity":     self.params.energy_capacity,
                "recharge":     self.params.energy_recharge_rate,
            },
            "homeostasis": {
                "arousal":              (self.arousal * 1000.0).round() / 1000.0,
                "metabolic_pressure":   (self.metabolic_pressure * 1000.0).round() / 1000.0,
                "death_count":          self.stats.death_count,
                "death_clock":          self.death_clock,
                "death_triggered":      self.death_triggered,
            },
            "pain": {
                "composite":    (self.pain.composite * 1000.0).round() / 1000.0,
                "energy":       (self.pain.energy_pain * 1000.0).round() / 1000.0,
                "atrophy":      (self.pain.atrophy_pain * 1000.0).round() / 1000.0,
                "fitness":      (self.pain.fitness_pain * 1000.0).round() / 1000.0,
                "isolation":    (self.pain.isolation_pain * 1000.0).round() / 1000.0,
                "pressure":     (self.pain.pressure_pain * 1000.0).round() / 1000.0,
                "arousal":      (self.pain.arousal_pain * 1000.0).round() / 1000.0,
            },
            "stress_mode": format!("{:?}", self.stress_mode).to_lowercase(),
            "modules":    modules,
            "mesh_peers": mesh_peers,
            "timestamp":  chrono::Local::now().to_rfc3339(),
        })
    }

    pub fn api_brain(&self) -> serde_json::Value {
        let now = now_f64();

        let neurons: Vec<_> = self.neurons.iter().map(|n| {
            let spiking = n.spike_time > 0.0 && (now - n.spike_time) < 0.5;
            serde_json::json!({
                "id":          n.id,
                "layer":       n.module,
                "x":           n.x,
                "y":           n.y,
                "potential":   (n.potential   * 1000.0).round() / 1000.0,
                "activation":  (n.activation  * 1000.0).round() / 1000.0,
                "importance":  (n.importance  * 1000.0).round() / 1000.0,
                "firing_count": n.firing_count,
                "is_spiking":  spiking,
            })
        }).collect();

        let synapses: Vec<_> = self.synapses.iter()
            .filter(|s| s.active)
            .take(800)
            .map(|s| serde_json::json!({
                "source": self.neurons[s.source].id,
                "target": self.neurons[s.target].id,
                "weight": (s.weight * 1000.0).round() / 1000.0,
            }))
            .collect();

        let modules: serde_json::Map<_, _> = self.modules.iter().map(|(name, m)| {
            let v = serde_json::json!({
                "x":      m.x,
                "y":      m.y,
                "color":  m.color,
                "neurons": m.neuron_indices.len(),
            });
            (name.clone(), v)
        }).collect();

        serde_json::json!({
            "neurons":  neurons,
            "synapses": synapses,
            "spikes":   self.recent_spikes,
            "modules":  modules,
        })
    }

    pub fn receive_mesh_status(&mut self, peer_port: u16, status: serde_json::Value) {
        let now = now_f64();
        let peer_info = MeshPeerInfo {
            name: status.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
            last_seen: now,
            generation: status.get("evolution")
                .and_then(|e| e.get("generation"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            fitness: status.get("evolution")
                .and_then(|e| e.get("fitness"))
                .and_then(|f| f.get("overall"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            frontier_hypervolume: status.get("evolution")
                .and_then(|e| e.get("pareto_frontier"))
                .and_then(|p| p.get("hypervolume"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            is_alive: true,
        };
        self.mesh.peers.insert(peer_port, peer_info);
    }

    /// Receive a gossip message and return whether it should be forwarded.
    /// Returns `Some(outgoing_payload)` if the message is worth propagating.
    pub fn receive_gossip(&mut self, msg: &GossipMessage) -> Option<serde_json::Value> {
        // Track/update the source peer
        let now = now_f64();
        let peer_info = MeshPeerInfo {
            name: msg.source_name.clone(),
            last_seen: now,
            generation: msg.payload.get("generation").and_then(|v| v.as_u64()).unwrap_or(0),
            fitness: msg.payload.get("fitness").and_then(|v| v.as_f64()).unwrap_or(0.0),
            frontier_hypervolume: msg.payload.get("hypervolume").and_then(|v| v.as_f64()).unwrap_or(0.0),
            is_alive: true,
        };
        self.mesh.peers.insert(msg.source_port, peer_info);

        // Only forward evolution data that's newsworthy
        match msg.msg_type.as_str() {
            "evolution" => {
                let their_gen = msg.payload.get("generation").and_then(|v| v.as_u64()).unwrap_or(0);
                let our_gen = self.evolution.generation;
                if their_gen > our_gen {
                    // Their evolution is ahead — pull their data
                    Some(msg.payload.clone())
                } else {
                    None // nothing new to propagate
                }
            }
            "peer_discovery" => {
                // Forward discovery to spread new peers
                if msg.ttl > 1 {
                    Some(msg.payload.clone())
                } else {
                    None
                }
            }
            "distress" => {
                // Always propagate distress signals for mesh-wide awareness
                if msg.ttl > 1 {
                    Some(msg.payload.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PERSISTENCE JSON
// ═══════════════════════════════════════════════════════════════

pub(crate) const SAVE_PATH: &str = "brain_state.json";

pub fn save_brain(brain: &Brain) {
    match serde_json::to_string(brain) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SAVE_PATH, json) {
                warn!("Échec sauvegarde brain: {e}");
            } else {
                info!("Brain sauvegardé → {SAVE_PATH}");
            }
        }
        Err(e) => warn!("Sérialisation échouée: {e}"),
    }
}
