//! Reproduction — child brain spawning from evolved genome.
//! Roadmap Section 6: serialization, genetic variation, process spawning.
//!
//! # Architecture
//! - **Asexual** (`child_genome`): derives a child `Genome` from one parent
//!   via jittered point mutations on evolvable parameters.
//! - **Sexual** (`crossover_genomes`): recombines two parent genomes via
//!   BLX-alpha blend crossover on numeric fields + uniform crossover on
//!   discrete fields. Modules matched by name; unique modules inherited
//!   with jitter.
//! - A full `Brain` JSON state file is written to disk with a unique child
//!   name, then a new OS process is spawned using the same binary.
//! - The child inherits the parent(s)' genome but starts with a fresh
//!   network (no synaptic weights carried over — variation is genetic only).

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::brain::{Brain, now_f64};
use crate::evolution::{Genome, ModuleGene, CortexConfigGene, ScriptGenome};

// ── Child metadata ─────────────────────────────────────────────────────────

/// Information returned after successfully spawning a child brain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildInfo {
    pub child_name: String,
    pub child_port: u16,
    pub child_pid: u32,
    pub child_generation: u64,
    pub genome_json_path: String,
}

// ── Genome serialization (standalone, not tied to Brain) ───────────────────

/// Serialize just the genome to a JSON string (no neurons/synapses).
pub fn genome_to_json(genome: &Genome) -> Result<String, String> {
    serde_json::to_string_pretty(genome)
        .map_err(|e| format!("genome serialization failed: {e}"))
}

/// Deserialize a genome from a JSON string.
pub fn genome_from_json(json: &str) -> Result<Genome, String> {
    serde_json::from_str(json).map_err(|e| format!("genome deserialization failed: {e}"))
}

// ── Reproductive variation ─────────────────────────────────────────────────

/// Apply a small random perturbation to an f32 value.
fn jitter_f32(value: f32, range: f32, rng: &mut impl Rng) -> f32 {
    let delta = rng.random_range(-range..range);
    (value + delta).clamp(0.0001, 1000.0)
}

fn jitter_f64(value: f64, range: f64, rng: &mut impl Rng) -> f64 {
    let delta = rng.random_range(-range..range);
    (value + delta).clamp(0.0001, 1000.0)
}

/// Create a child module gene with reproductive mutation applied.
fn child_module_gene(parent: &ModuleGene, rng: &mut impl Rng) -> ModuleGene {
    ModuleGene {
        name: parent.name.clone(),
        neuron_count: parent.neuron_count,
        position_x: jitter_f32(parent.position_x, 20.0, rng),
        position_y: jitter_f32(parent.position_y, 20.0, rng),
        color: parent.color.clone(),
        learning_rate: jitter_f32(parent.learning_rate, 0.05, rng),
        firing_threshold_mean: jitter_f32(parent.firing_threshold_mean, 0.1, rng),
        firing_threshold_std: jitter_f32(parent.firing_threshold_std, 0.05, rng),
        synaptic_density: jitter_f32(parent.synaptic_density, 0.05, rng),
        mutation_rate: jitter_f32(parent.mutation_rate, 0.005, rng),
        energy_multiplier: jitter_f32(parent.energy_multiplier, 0.2, rng),
        energy_priority: jitter_f32(parent.energy_priority, 0.1, rng),
    }
}

/// Create a child cortex config gene with jitter.
fn child_cortex_config(parent: &CortexConfigGene, rng: &mut impl Rng) -> CortexConfigGene {
    CortexConfigGene {
        d_model: parent.d_model,
        state_dim: parent.state_dim,
        n_layers: parent.n_layers,
        modulation_strength: jitter_f64(parent.modulation_strength, 0.1, rng),
    }
}

/// Apply reproductive jitter to BrainParams (only numerical fields).
fn child_brain_params(parent: &crate::brain::BrainParams, rng: &mut impl Rng) -> crate::brain::BrainParams {
    crate::brain::BrainParams {
        tau: jitter_f32(parent.tau, 0.01, rng),
        homeostatic_rate: jitter_f32(parent.homeostatic_rate, 0.002, rng),
        prune_threshold: jitter_f32(parent.prune_threshold, 0.01, rng),
        stdp_learning_rate: jitter_f32(parent.stdp_learning_rate, 0.002, rng),
        energy_capacity: jitter_f32(parent.energy_capacity, 0.05, rng),
        base_metabolic_rate: jitter_f32(parent.base_metabolic_rate, 0.00001, rng),
        firing_energy_cost: jitter_f32(parent.firing_energy_cost, 0.005, rng),
        synapse_energy_cost: jitter_f32(parent.synapse_energy_cost, 0.000005, rng),
        energy_recharge_rate: jitter_f32(parent.energy_recharge_rate, 0.0005, rng),
        metabolism_enabled: parent.metabolism_enabled,
        module_budget_enabled: parent.module_budget_enabled,
        budget_share: jitter_f32(parent.budget_share, 0.05, rng),
        energy_pool_enabled: parent.energy_pool_enabled,
        energy_pool_recharge_rate: jitter_f32(parent.energy_pool_recharge_rate, 0.0005, rng),
        pool_contribution: jitter_f32(parent.pool_contribution, 0.05, rng),
        neuron_creation_cost: jitter_f32(parent.neuron_creation_cost, 0.01, rng),
        synapse_creation_cost: jitter_f32(parent.synapse_creation_cost, 0.001, rng),
        growth_energy_threshold: jitter_f32(parent.growth_energy_threshold, 0.05, rng),
        arousal_decay: jitter_f32(parent.arousal_decay, 0.01, rng),
        arousal_circadian_sensitivity: jitter_f32(parent.arousal_circadian_sensitivity, 0.05, rng),
        arousal_energy_sensitivity: jitter_f32(parent.arousal_energy_sensitivity, 0.05, rng),
        set_point_drift_rate: jitter_f32(parent.set_point_drift_rate, 0.0005, rng),
        metabolic_pressure_rate: jitter_f32(parent.metabolic_pressure_rate, 0.005, rng),
        metabolic_pressure_decay: jitter_f32(parent.metabolic_pressure_decay, 0.01, rng),
        arousal_threshold_mod: jitter_f32(parent.arousal_threshold_mod, 0.05, rng),
    }
}

/// Build a child genome from a parent genome with reproductive variation.
///
/// The child inherits the parent's module layout but with jittered parameters.
pub fn child_genome(parent: &Genome, rng: &mut impl Rng) -> Genome {
    let modules: Vec<ModuleGene> = parent
        .modules
        .iter()
        .map(|m| child_module_gene(m, rng))
        .collect();

    let brain_params = child_brain_params(&parent.brain_params, rng);

    Genome {
        modules,
        brain_params,
        cortex_config: child_cortex_config(&parent.cortex_config, rng),
        behaviors: parent.behaviors.clone(),
        generation: parent.generation + 1,
        created: now_f64(),
        parent_id: Some(parent.generation),
        script_genome: ScriptGenome::default(),
    }
}

// ── Child brain state builder ──────────────────────────────────────────────

/// Build a full child `Brain` instance from a child genome.
///
/// Creates a fresh brain (no synaptic weights from parent), applies the child
/// genome, and returns the `Brain` ready to be saved and spawned.
pub fn build_child_brain(child_name: &str, child_port: u16, child_genome: &Genome) -> Brain {
    let mut brain = Brain::new(child_name, child_port);

    // Apply the child genome to the evolution engine
    brain.evolution.genome = child_genome.clone();
    brain.evolution.generation = child_genome.generation;

    // Re-apply genome params to the brain
    // Use same pattern as Brain::new()
    let e = std::mem::take(&mut brain.evolution);
    e.apply_global_params(&mut brain);
    brain.evolution = e;

    brain
}

// ── Port discovery ─────────────────────────────────────────────────────────

/// Find an available TCP port by attempting to bind with retry on failure.
/// Up to `max_retries` attempts with `base_port..base_port+100` each time.
pub fn find_available_port_with_retry(base_port: u16, max_retries: u32) -> Option<u16> {
    for retry in 0..max_retries {
        if let Some(port) = find_available_port(base_port) {
            return Some(port);
        }
        if retry < max_retries - 1 {
            std::thread::sleep(std::time::Duration::from_millis(50 * (retry as u64 + 1)));
        }
    }
    None
}

/// Find an available TCP port by attempting to bind.
/// Tries ports starting from `base_port`, incrementing up to `base_port + 100`.
pub fn find_available_port(base_port: u16) -> Option<u16> {
    for offset in 0..100u16 {
        let candidate = base_port.saturating_add(offset);
        if candidate == 0 {
            continue;
        }
        if std::net::TcpListener::bind(("127.0.0.1", candidate)).is_ok() {
            return Some(candidate);
        }
    }
    None
}

// ── Process spawning ───────────────────────────────────────────────────────

/// Spawn a child SoulLink process from a parent brain.
///
/// # Steps
/// 1. Derive a child genome from the parent genome with reproductive variation.
/// 2. Find an available port.
/// 3. Write a child brain state JSON file to disk.
/// 4. Launch the same binary as a child process (background).
///
/// # Returns
/// `ChildInfo` with the child's name, port, PID, and genome file path.
pub fn spawn_child(
    parent_genome: &Genome,
    parent_name: &str,
    parent_port: u16,
    rng: &mut impl Rng,
) -> Result<ChildInfo, String> {
    // 1. Derive child genome
    let child_g = child_genome(parent_genome, rng);

    // 2. Find available port with retry
    let child_port = find_available_port_with_retry(parent_port.saturating_add(1), 3)
        .ok_or_else(|| "No available port found for child process".to_string())?;

    // 3. Generate child identity (random hex suffix)
    let short_id: String = (0..6)
        .map(|_| {
            let idx = rng.random_range(0..16u8);
            "0123456789abcdef".as_bytes()[idx as usize] as char
        })
        .collect();
    let child_name = format!("child_of_{parent_name}_{short_id}");

    // 4. Build child brain and write state file
    let child_brain = build_child_brain(&child_name, child_port, &child_g);
    let state_json = serde_json::to_string_pretty(&child_brain)
        .map_err(|e| format!("child brain serialization failed: {e}"))?;

    let state_path = format!("/tmp/soullink-child-{child_name}.json");
    std::fs::write(&state_path, &state_json)
        .map_err(|e| format!("failed to write child state file: {e}"))?;

    // Also write just the genome for inspection
    let genome_path = format!("/tmp/soullink-genome-{child_name}.json");
    std::fs::write(&genome_path, &genome_to_json(&child_g)?)
        .map_err(|e| format!("failed to write child genome file: {e}"))?;

    // 5. Locate the current binary
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot find current executable: {e}"))?;

    // 6. Spawn child process
    let child = std::process::Command::new(&exe)
        .arg(&child_name)
        .arg(child_port.to_string())
        .env("SOULLINK_CHILD_OF", parent_name)
        .env("SOULLINK_CHILD_PORT", child_port.to_string())
        .env("PORT", child_port.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn child process: {e}"))?;

    let child_pid = child.id();

    Ok(ChildInfo {
        child_name,
        child_port,
        child_pid,
        child_generation: child_g.generation,
        genome_json_path: genome_path,
    })
}

// ── Crossover (sexual reproduction) ────────────────────────────────────────

/// Maximum allowed blend range for any f32 crossover field.
/// Prevents divergence explosion when parents are far apart.
const MAX_BLEND_RANGE_F32: f32 = 0.5;

/// Blend crossover for f32: child = blend(a,b) + uniform(-alpha*diff, +alpha*diff).
/// When alpha=0 or parents are identical, returns the exact midpoint.
/// `max_range` caps the blend window to prevent genetic drift explosion.
fn crossover_f32(a: f32, b: f32, alpha: f32, max_range: f32, rng: &mut impl Rng) -> f32 {
    let diff = (a - b).abs();
    let mid = (a + b) * 0.5;
    let range = (alpha * diff).min(max_range);
    if range <= f32::EPSILON {
        return mid.clamp(0.0001, 1000.0);
    }
    let delta = rng.random_range(-range..range);
    (mid + delta).clamp(0.0001, 1000.0)
}

/// Maximum allowed blend range for any f64 crossover field.
const MAX_BLEND_RANGE_F64: f64 = 0.5;

fn crossover_f64(a: f64, b: f64, alpha: f64, max_range: f64, rng: &mut impl Rng) -> f64 {
    let diff = (a - b).abs();
    let mid = (a + b) * 0.5;
    let range = (alpha * diff).min(max_range);
    if range <= f64::EPSILON {
        return mid.clamp(0.0001, 1000.0);
    }
    let delta = rng.random_range(-range..range);
    (mid + delta).clamp(0.0001, 1000.0)
}



/// Blend two module genes matched by name.
fn crossover_module_gene(a: &ModuleGene, b: &ModuleGene, alpha: f32, rng: &mut impl Rng) -> ModuleGene {
    ModuleGene {
        name: a.name.clone(),
        neuron_count: if rng.random_bool(0.5) { a.neuron_count } else { b.neuron_count },
        position_x: crossover_f32(a.position_x, b.position_x, alpha, MAX_BLEND_RANGE_F32, rng),
        position_y: crossover_f32(a.position_y, b.position_y, alpha, MAX_BLEND_RANGE_F32, rng),
        color: if rng.random_bool(0.5) { a.color.clone() } else { b.color.clone() },
        learning_rate: crossover_f32(a.learning_rate, b.learning_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        firing_threshold_mean: crossover_f32(a.firing_threshold_mean, b.firing_threshold_mean, alpha, MAX_BLEND_RANGE_F32, rng),
        firing_threshold_std: crossover_f32(a.firing_threshold_std, b.firing_threshold_std, alpha, MAX_BLEND_RANGE_F32, rng),
        synaptic_density: crossover_f32(a.synaptic_density, b.synaptic_density, alpha, MAX_BLEND_RANGE_F32, rng),
        mutation_rate: crossover_f32(a.mutation_rate, b.mutation_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        energy_multiplier: crossover_f32(a.energy_multiplier, b.energy_multiplier, alpha, MAX_BLEND_RANGE_F32, rng),
        energy_priority: crossover_f32(a.energy_priority, b.energy_priority, alpha, MAX_BLEND_RANGE_F32, rng),
    }
}

/// Blend two cortex config genes.
fn crossover_cortex_config(a: &CortexConfigGene, b: &CortexConfigGene, alpha: f64, rng: &mut impl Rng) -> CortexConfigGene {
    CortexConfigGene {
        d_model: if rng.random_bool(0.5) { a.d_model } else { b.d_model },
        state_dim: if rng.random_bool(0.5) { a.state_dim } else { b.state_dim },
        n_layers: if rng.random_bool(0.5) { a.n_layers } else { b.n_layers },
        modulation_strength: crossover_f64(a.modulation_strength, b.modulation_strength, alpha, MAX_BLEND_RANGE_F64, rng),
    }
}

/// Blend two brain params via crossover on every numeric field.
fn crossover_brain_params(a: &crate::brain::BrainParams, b: &crate::brain::BrainParams, alpha: f32, rng: &mut impl Rng) -> crate::brain::BrainParams {
    crate::brain::BrainParams {
        metabolism_enabled: if rng.random_bool(0.5) { a.metabolism_enabled } else { b.metabolism_enabled },
        module_budget_enabled: if rng.random_bool(0.5) { a.module_budget_enabled } else { b.module_budget_enabled },
        energy_pool_enabled: if rng.random_bool(0.5) { a.energy_pool_enabled } else { b.energy_pool_enabled },
        tau: crossover_f32(a.tau, b.tau, alpha, MAX_BLEND_RANGE_F32, rng),
        homeostatic_rate: crossover_f32(a.homeostatic_rate, b.homeostatic_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        prune_threshold: crossover_f32(a.prune_threshold, b.prune_threshold, alpha, MAX_BLEND_RANGE_F32, rng),
        stdp_learning_rate: crossover_f32(a.stdp_learning_rate, b.stdp_learning_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        energy_capacity: crossover_f32(a.energy_capacity, b.energy_capacity, alpha, MAX_BLEND_RANGE_F32, rng),
        base_metabolic_rate: crossover_f32(a.base_metabolic_rate, b.base_metabolic_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        firing_energy_cost: crossover_f32(a.firing_energy_cost, b.firing_energy_cost, alpha, MAX_BLEND_RANGE_F32, rng),
        synapse_energy_cost: crossover_f32(a.synapse_energy_cost, b.synapse_energy_cost, alpha, MAX_BLEND_RANGE_F32, rng),
        energy_recharge_rate: crossover_f32(a.energy_recharge_rate, b.energy_recharge_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        budget_share: crossover_f32(a.budget_share, b.budget_share, alpha, MAX_BLEND_RANGE_F32, rng),
        energy_pool_recharge_rate: crossover_f32(a.energy_pool_recharge_rate, b.energy_pool_recharge_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        pool_contribution: crossover_f32(a.pool_contribution, b.pool_contribution, alpha, MAX_BLEND_RANGE_F32, rng),
        neuron_creation_cost: crossover_f32(a.neuron_creation_cost, b.neuron_creation_cost, alpha, MAX_BLEND_RANGE_F32, rng),
        synapse_creation_cost: crossover_f32(a.synapse_creation_cost, b.synapse_creation_cost, alpha, MAX_BLEND_RANGE_F32, rng),
        growth_energy_threshold: crossover_f32(a.growth_energy_threshold, b.growth_energy_threshold, alpha, MAX_BLEND_RANGE_F32, rng),
        arousal_decay: crossover_f32(a.arousal_decay, b.arousal_decay, alpha, MAX_BLEND_RANGE_F32, rng),
        arousal_circadian_sensitivity: crossover_f32(a.arousal_circadian_sensitivity, b.arousal_circadian_sensitivity, alpha, MAX_BLEND_RANGE_F32, rng),
        arousal_energy_sensitivity: crossover_f32(a.arousal_energy_sensitivity, b.arousal_energy_sensitivity, alpha, MAX_BLEND_RANGE_F32, rng),
        set_point_drift_rate: crossover_f32(a.set_point_drift_rate, b.set_point_drift_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        metabolic_pressure_rate: crossover_f32(a.metabolic_pressure_rate, b.metabolic_pressure_rate, alpha, MAX_BLEND_RANGE_F32, rng),
        metabolic_pressure_decay: crossover_f32(a.metabolic_pressure_decay, b.metabolic_pressure_decay, alpha, MAX_BLEND_RANGE_F32, rng),
        arousal_threshold_mod: crossover_f32(a.arousal_threshold_mod, b.arousal_threshold_mod, alpha, MAX_BLEND_RANGE_F32, rng),
    }
}

/// Uniform crossover for behavior genes — enum fields pick randomly from either parent.
fn crossover_behaviors(a: &crate::evolution::BehaviorGenes, b: &crate::evolution::BehaviorGenes, alpha: f32, rng: &mut impl Rng) -> crate::evolution::BehaviorGenes {
    crate::evolution::BehaviorGenes {
        activation_fn: if rng.random_bool(0.5) { a.activation_fn.clone() } else { b.activation_fn.clone() },
        plasticity_rule: if rng.random_bool(0.5) { a.plasticity_rule.clone() } else { b.plasticity_rule.clone() },
        lateral_inhibition: crossover_f32(a.lateral_inhibition, b.lateral_inhibition, alpha, MAX_BLEND_RANGE_F32, rng),
        refractory_ticks: if rng.random_bool(0.5) { a.refractory_ticks } else { b.refractory_ticks },
        noise_level: crossover_f32(a.noise_level, b.noise_level, alpha, MAX_BLEND_RANGE_F32, rng),
    }
}

/// Merge two script genomes — take union of scripts and field definitions.
fn crossover_script_genome(a: &ScriptGenome, b: &ScriptGenome, rng: &mut impl Rng) -> ScriptGenome {
    let mut activation_scripts = a.activation_scripts.clone();
    for (k, v) in &b.activation_scripts {
        activation_scripts.entry(k.clone()).or_insert_with(|| v.clone());
    }
    let plasticity_script = if rng.random_bool(0.5) {
        a.plasticity_script.clone().or_else(|| b.plasticity_script.clone())
    } else {
        b.plasticity_script.clone().or_else(|| a.plasticity_script.clone())
    };
    let mut field_definitions = a.field_definitions.clone();
    for (target, fields) in &b.field_definitions {
        let entry = field_definitions.entry(target.clone()).or_default();
        for (name, val) in fields {
            entry.entry(name.clone()).or_insert_with(|| *val);
        }
    }
    ScriptGenome { activation_scripts, plasticity_script, field_definitions }
}

/// Build a child genome via sexual reproduction (crossover + mutation).
///
/// Modules present in both parents are blended via BLX-alpha crossover.
/// Modules unique to one parent are inherited with jitter.
/// Global params, cortex config, and behaviors are all blended.
///
/// `alpha` controls exploration breadth: 0.0 = exact midpoint, 0.5 = default,
/// 1.0 = wide exploration beyond parent range.
pub fn crossover_genomes(
    parent_a: &Genome,
    parent_b: &Genome,
    alpha: f32,
    rng: &mut impl Rng,
) -> Genome {
    // Build name→gene map for parent B only (for fast lookup).
    // Parent A is iterated in order to preserve module ordering.
    let map_b: std::collections::HashMap<&str, &ModuleGene> =
        parent_b.modules.iter().map(|m| (m.name.as_str(), m)).collect();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

    let mut child_modules: Vec<ModuleGene> = Vec::new();
    let alpha_f64 = alpha as f64;

    // Iterate parent A in order — crossover if both have the module
    for ma in &parent_a.modules {
        seen.insert(ma.name.as_str());
        if let Some(mb) = map_b.get(ma.name.as_str()) {
            child_modules.push(crossover_module_gene(ma, mb, alpha, rng));
        } else {
            child_modules.push(child_module_gene(ma, rng));
        }
    }
    // Modules unique to parent B → inherit with jitter (appended at end)
    for mb in &parent_b.modules {
        if !seen.contains(mb.name.as_str()) {
            child_modules.push(child_module_gene(mb, rng));
        }
    }

    let brain_params = crossover_brain_params(&parent_a.brain_params, &parent_b.brain_params, alpha, rng);
    let cortex_config = crossover_cortex_config(&parent_a.cortex_config, &parent_b.cortex_config, alpha_f64, rng);
    let behaviors = crossover_behaviors(&parent_a.behaviors, &parent_b.behaviors, alpha, rng);
    let script_genome = crossover_script_genome(&parent_a.script_genome, &parent_b.script_genome, rng);

    let max_gen = parent_a.generation.max(parent_b.generation);
    let parent_ids = if parent_a.generation == parent_b.generation {
        Some(parent_a.generation)
    } else {
        Some(max_gen)
    };

    Genome {
        modules: child_modules,
        brain_params,
        cortex_config,
        behaviors,
        generation: max_gen + 1,
        created: now_f64(),
        parent_id: parent_ids,
        script_genome,
    }
}

/// Spawn a child from two parent genomes via sexual reproduction.
///
/// Combines both parents via `crossover_genomes()` then spawns like `spawn_child()`.
pub fn spawn_sexual_child(
    parent_a_genome: &Genome,
    parent_a_name: &str,
    parent_b_genome: &Genome,
    parent_b_name: &str,
    parent_port: u16,
    crossover_alpha: f32,
    rng: &mut impl Rng,
) -> Result<ChildInfo, String> {
    let child_g = crossover_genomes(parent_a_genome, parent_b_genome, crossover_alpha, rng);

    let child_port = find_available_port_with_retry(parent_port.saturating_add(1), 3)
        .ok_or_else(|| "No available port found for child process".to_string())?;

    let short_id: String = (0..6)
        .map(|_| {
            let idx = rng.random_range(0..16u8);
            "0123456789abcdef".as_bytes()[idx as usize] as char
        })
        .collect();
    let child_name = format!("hybrid_of_{parent_a_name}_and_{parent_b_name}_{short_id}");

    let child_brain = build_child_brain(&child_name, child_port, &child_g);
    let state_json = serde_json::to_string_pretty(&child_brain)
        .map_err(|e| format!("child brain serialization failed: {e}"))?;

    let state_path = format!("/tmp/soullink-child-{child_name}.json");
    std::fs::write(&state_path, &state_json)
        .map_err(|e| format!("failed to write child state file: {e}"))?;

    let genome_path = format!("/tmp/soullink-genome-{child_name}.json");
    std::fs::write(&genome_path, &genome_to_json(&child_g)?)
        .map_err(|e| format!("failed to write child genome file: {e}"))?;

    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot find current executable: {e}"))?;

    let child = std::process::Command::new(&exe)
        .arg(&child_name)
        .arg(child_port.to_string())
        .env("SOULLINK_CHILD_OF", parent_a_name)
        .env("SOULLINK_CHILD_SECOND_PARENT", parent_b_name)
        .env("SOULLINK_CHILD_PORT", child_port.to_string())
        .env("PORT", child_port.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn child process: {e}"))?;

    let child_pid = child.id();

    Ok(ChildInfo {
        child_name,
        child_port,
        child_pid,
        child_generation: child_g.generation,
        genome_json_path: genome_path,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_genome_roundtrip() {
        let genome = Genome::base();
        let json = genome_to_json(&genome).expect("serialize");
        let decoded = genome_from_json(&json).expect("deserialize");
        assert_eq!(decoded.generation, genome.generation);
        assert_eq!(decoded.modules.len(), genome.modules.len());
    }

    #[test]
    fn test_child_genome_variation() {
        let parent = Genome::base();
        let mut rng = StdRng::seed_from_u64(42);
        let child = child_genome(&parent, &mut rng);

        // Child should have incremented generation
        assert_eq!(child.generation, parent.generation + 1);
        assert_eq!(child.parent_id, Some(parent.generation));

        // Modules should have jittered values (not identical)
        let first_module_differs = parent.modules[0].learning_rate
            != child.modules[0].learning_rate
            || parent.modules[0].energy_multiplier
                != child.modules[0].energy_multiplier;
        assert!(
            first_module_differs,
            "child modules should be jittered from parent"
        );

        // Module names should be preserved
        for (pm, cm) in parent.modules.iter().zip(child.modules.iter()) {
            assert_eq!(pm.name, cm.name);
        }
    }

    #[test]
    fn test_build_child_brain() {
        let genome = Genome::base();
        let brain = build_child_brain("test_child", 19999, &genome);
        assert_eq!(brain.node_name, "test_child");
        assert_eq!(brain.node_port, 19999);
        assert_eq!(brain.evolution.genome.generation, genome.generation);
    }

    #[test]
    fn test_find_available_port_basic() {
        // Only verify the function doesn't panic with reasonable input.
        // Actual port availability depends on the environment.
        let result = find_available_port(22000);
        if let Some(port) = result {
            assert!(port >= 22000);
            assert!(port < 22100);
        }
        // If None, that's OK — environment may restrict port binding.
    }

    // ── Crossover tests ──────────────────────────────────────────────

    #[test]
    fn test_crossover_genome_variation() {
        let mut rng = StdRng::seed_from_u64(123);
        let a = Genome::base();
        // Create parent B with clearly different values
        let mut b = Genome::base();
        for m in &mut b.modules {
            m.learning_rate *= 0.5;
            m.energy_multiplier *= 2.0;
        }
        b.brain_params.energy_capacity = 2.0;
        b.brain_params.tau = 0.5;

        let child = crossover_genomes(&a, &b, 0.5, &mut rng);

        // Generation incremented
        assert_eq!(child.generation, b.generation + 1);
        assert!(child.parent_id.is_some());

        // Child values should be between or near parents (BLX-alpha may extend)
        for (cm, ma) in child.modules.iter().zip(a.modules.iter()) {
            assert_eq!(cm.name, ma.name, "module names preserved");
        }

        // Brain params blended
        assert!(child.brain_params.energy_capacity > 0.0);
        assert!(child.brain_params.tau > 0.0);
    }

    #[test]
    fn test_crossover_with_extra_module() {
        let mut rng = StdRng::seed_from_u64(456);
        let a = Genome::base();
        let mut b = Genome::base();
        // Parent B has an extra module
        b.modules.push(ModuleGene {
            name: "extra_special".into(),
            neuron_count: 1000,
            position_x: 100.0,
            position_y: 100.0,
            color: "#00ff00".into(),
            learning_rate: 0.15,
            firing_threshold_mean: 0.8,
            firing_threshold_std: 0.2,
            synaptic_density: 0.5,
            mutation_rate: 0.01,
            energy_multiplier: 0.5,
            energy_priority: 0.3,
        });

        let child = crossover_genomes(&a, &b, 0.5, &mut rng);

        // Child should have at least max(len_a, len_b) modules
        assert!(child.modules.len() >= b.modules.len());

        // The extra_special module should be present
        assert!(child.modules.iter().any(|m| m.name == "extra_special"));
    }

    #[test]
    fn test_crossover_deterministic_at_zero_alpha() {
        let mut rng = StdRng::seed_from_u64(789);
        let a = Genome::base();
        let mut b = Genome::base();
        for m in &mut b.modules {
            m.learning_rate = 0.5;
        }

        let child = crossover_genomes(&a, &b, 0.0, &mut rng);

        // alpha=0 means exact midpoint, no noise
        for cm in child.modules.iter() {
            let pa = a.modules.iter().find(|m| m.name == cm.name).unwrap();
            let pb = b.modules.iter().find(|m| m.name == cm.name).unwrap();
            // learning_rate should be exactly mid
            let expected = (pa.learning_rate + pb.learning_rate) * 0.5;
            assert!(
                (cm.learning_rate - expected).abs() < 0.0001,
                "alpha=0 should give midpoint, got {} vs expected {}",
                cm.learning_rate,
                expected
            );
        }
    }

    #[test]
    fn test_crossover_novelty() {
        let mut rng = StdRng::seed_from_u64(999);
        let mut a = Genome::base();
        let mut b = Genome::base();
        // Make parents different so BLX-alpha has room to vary
        for m in &mut a.modules {
            m.learning_rate *= 0.8;
        }
        for m in &mut b.modules {
            m.learning_rate *= 1.3;
        }

        // Run multiple crossovers — results should vary due to randomness in BLX-alpha
        let child1 = crossover_genomes(&a, &b, 0.5, &mut rng);
        let child2 = crossover_genomes(&a, &b, 0.5, &mut rng);

        // At least one module field should differ between siblings
        let any_diff = child1.modules.iter().zip(child2.modules.iter()).any(|(c1, c2)| {
            (c1.learning_rate - c2.learning_rate).abs() > 0.0001
                || (c1.energy_multiplier - c2.energy_multiplier).abs() > 0.0001
        });
        assert!(any_diff, "siblings from crossover should vary due to randomness");
    }

    #[test]
    fn test_crossover_behaviors_preserved() {
        let mut rng = StdRng::seed_from_u64(42);
        let a = Genome::base();
        let b = Genome::base();

        let child = crossover_genomes(&a, &b, 0.5, &mut rng);

        // Behavior fields should be valid
        assert!(child.behaviors.lateral_inhibition >= 0.0 && child.behaviors.lateral_inhibition <= 1.0);
        assert!(child.behaviors.noise_level >= 0.0 && child.behaviors.noise_level <= 1.0);
        assert!(child.behaviors.refractory_ticks > 0);
    }
}
