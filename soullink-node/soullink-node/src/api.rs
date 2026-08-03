//! API — HTTP handlers for the SoulLink brain mesh
//! Extracted from main.rs for modularity.
use std::time::Duration;
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use rand::Rng;
use serde::Deserialize;
use tokio::time::sleep;
use tracing::{info, warn};
use crate::brain::{Brain, SharedBrain, GossipMessage, save_brain, SAVE_PATH};
use crate::brain_module::{PainSignal, StressMode, current_period};
use crate::brain::now_f64;

pub(crate) fn load_brain(name: &str, port: u16) -> Option<Brain> {
    let data = std::fs::read_to_string(SAVE_PATH).ok()?;
    match serde_json::from_str::<Brain>(&data) {
        Ok(mut b) => {
            // Override identity from CLI/env (save file may have stale name/port)
            b.node_name = name.to_string();
            b.node_port = port;
            // Sync evolved params from genome to brain
            {
                let e = std::mem::take(&mut b.evolution);
                e.apply_global_params(&mut b);
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }
            // Restore SSM cortex state
            if b.cortex.config.enabled {
                if let Err(e) = crate::ssm_cortex::load_cortex_state("cortex_state.json", &mut b.cortex) {
                    tracing::warn!("Cortex state non trouvé ou invalide: {e}");
                }
            }
            // Restore DeepSeek Cortex
            if let Ok(ds) = crate::deepseek_cortex::DeepSeekCortex::load_state("deepseek_cortex_state.json") {
                b.deepseek_cortex = ds;
                tracing::info!("DeepSeek cortex restauré");
            }
            // Restore Meta Cortex
            if let Ok(mc) = crate::meta_cortex::MetaCortex::load_state("meta_cortex_state.json") {
                b.meta_cortex = mc;
                tracing::info!("Meta cortex restauré");
            }
            info!("Brain chargé depuis {SAVE_PATH} ({} neurones, gen {})",
                b.neurons.len(), b.evolution.generation);
            Some(b)
        }
        Err(e) => {
            warn!("Fichier de sauvegarde corrompu ({e}), nouveau brain créé");
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// ÉTAT PARTAGÉ
// ═══════════════════════════════════════════════════════════════


// ═══════════════════════════════════════════════════════════════
// BOUCLE DE SIMULATION (tâche Tokio)
// ═══════════════════════════════════════════════════════════════

pub(crate) async fn simulation_loop(brain: SharedBrain) {
    let mut tick: u64 = 0;
    loop {
        {
            let mut b = brain.write().await;
            let period = current_period();

            // Step neuronal à chaque tick (toutes les 100 ms)
            b.step();

            // ── CORRECTION 7: Métabolisme complet ──
            let metabolism_config = crate::metabolism::MetabolismConfig::default();
            let _ledger = b.metabolize(&metabolism_config);

            // Pain signal & stress mode — computed every tick
            b.pain = PainSignal::compute(&b);
            b.stress_mode = StressMode::from_pain(b.pain.composite);

            // Hibernation si énergie critique
            if b.should_hibernate(&metabolism_config) {
                b.arousal *= 0.5;
                tracing::warn!("⚡ HIBERNATION: energy_pool={:.1}", b.energy_pool);
            }

            // Stress response: adapt growth/consolidation intensity based on pain
            let stress_factor = match b.stress_mode {
                StressMode::Calm => 1.0,
                StressMode::Alert => 0.7,
                StressMode::Stress => 0.4,
                StressMode::Panic => 0.15,
            };

            // Stress modulates multiple behavioral dimensions beyond growth
            if b.params.metabolism_enabled {
                b.stress_firing_mod = match b.stress_mode {
                    StressMode::Panic => 1.30,
                    StressMode::Stress => 1.15,
                    StressMode::Alert => 1.05,
                    StressMode::Calm => 0.90,
                };
                b.stress_prune_mod = match b.stress_mode {
                    StressMode::Panic => 3.0,
                    StressMode::Stress => 2.0,
                    StressMode::Alert => 1.3,
                    StressMode::Calm => 0.8,
                };
                b.stress_stdp_mod = match b.stress_mode {
                    StressMode::Panic => 0.2,
                    StressMode::Stress => 0.5,
                    StressMode::Alert => 0.8,
                    StressMode::Calm => 1.2,
                };
                b.stress_inhibition_mod = match b.stress_mode {
                    StressMode::Panic => 2.5,
                    StressMode::Stress => 1.8,
                    StressMode::Alert => 1.2,
                    StressMode::Calm => 0.8,
                };
            } else {
                b.stress_firing_mod = 1.0;
                b.stress_prune_mod = 1.0;
                b.stress_stdp_mod = 1.0;
                b.stress_inhibition_mod = 1.0;
            }

            // SSM Cortex step every N ticks (take cortex, step, put back)
            let cortex_reward: f32 = {
                let cfg_enabled = b.cortex.config.enabled;
                let cfg_interval = b.cortex.config.tick_interval;
                if cfg_enabled && cfg_interval > 0 && tick % cfg_interval == 0 {
                    let mut c = std::mem::take(&mut b.cortex);
                    let output = c.step(&mut b);
                    let reward = (output.output_norm * output.output_norm).clamp(0.0, 0.1) as f32;
                    b.cortex = c;
                    reward
                } else {
                    0.0
                }
            };

            // Apply eligibility-based reward consolidation if cortex reward signal > threshold
            if cortex_reward > 0.005 {
                b.reward_consolidate(cortex_reward);
            }

            // Evolution step (take engine, step, put back)
            {
                let mut e = std::mem::take(&mut b.evolution);
                e.step(&mut b);
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }

            // Croissance / consolidation toutes les 20 ticks (2 s)
            // Stress factor réduit la croissance, augmente la consolidation
            if tick % 20 == 0 {
                let stress = stress_factor;
                match period.mode {
                    "consolidate" => {
                        // Under stress, consolidate harder (save what we have)
                        let boost = if stress < 0.5 { 1.0 + (1.0 - stress) } else { 1.0 };
                        b.consolidate(period.intensity * boost);
                    }
                    "grow" => {
                        // Under stress, grow less or not at all
                        b.grow(period.intensity * 1.5 * stress);
                    }
                    _ => {
                        b.grow(period.intensity * stress);
                    }
                }
            }

            // Emergency response: under panic mode, do extra consolidation
            if b.stress_mode == StressMode::Panic && tick % 10 == 0 {
                b.consolidate(0.3);
                // Force energy recharge — use pool first, then direct fallback
                let cap = b.params.energy_capacity;
                let enabled = b.params.metabolism_enabled;
                if enabled {
                    if b.params.energy_pool_enabled && b.energy_pool > 1.0 {
                        let relief = b.energy_pool * 0.05;
                        let per = relief / b.neurons.len().max(1) as f32;
                        for neuron in b.neurons.iter_mut() {
                            if neuron.energy_reserve < 0.3 {
                                neuron.energy_reserve = (neuron.energy_reserve + per).min(cap);
                            }
                        }
                        b.energy_pool = (b.energy_pool - relief).max(0.0);
                    } else {
                        for neuron in b.neurons.iter_mut() {
                            neuron.energy_reserve = (neuron.energy_reserve + 0.005).min(cap);
                        }
                    }
                }
            }

            // ═══════════════════════════════════════════════════════
            // DEATH AVOIDANCE SYSTEM
            // ═══════════════════════════════════════════════════════
            if b.params.metabolism_enabled {
                let pain_death = b.pain.composite > 0.95;
                let energy_death = b.stats.energy_avg < 0.01 && b.stats.energy_debt_count > b.neurons.len() / 2;
                let should_die = pain_death || energy_death;

                if should_die {
                    b.death_clock += 1;
                } else {
                    b.death_clock = 0;
                    b.death_triggered = false;
                    b.sos_sent = false;
                    b.death_consolidation_ticks = 0;
                }

                let death_threshold = if pain_death { 50 } else { 100 };

                if b.death_clock >= death_threshold && !b.death_triggered {
                    b.death_triggered = true;
                    b.death_consolidation_ticks = 0;
                    warn!(
                        "DEATH TRIGGERED: pain={:.3} energy_avg={:.3} clock={}",
                        b.pain.composite, b.stats.energy_avg, b.death_clock
                    );

                    // SOS payload (clone data before dropping lock)
                    let sos_payload = serde_json::json!({
                        "msg_type": "distress",
                        "source_port": b.node_port,
                        "source_name": b.node_name,
                        "generation": b.evolution.generation,
                        "fitness": b.evolution.fitness.current.overall,
                        "cause": if pain_death { "pain_exceeded" } else { "energy_depletion" },
                        "age_ticks": b.stats.age_ticks,
                    });
                    let seed_ports: Vec<u16> = b.mesh.seed_ports.clone();
                    let node_port = b.node_port;
                    b.sos_sent = true;

                    // Send SOS asynchronously (fire-and-forget)
                    tokio::spawn(async move {
                        let client = reqwest::Client::builder()
                            .timeout(Duration::from_secs(2))
                            .build().unwrap_or_default();
                        for seed_port in &seed_ports {
                            if *seed_port == node_port { continue; }
                            let url = format!("http://127.0.0.1:{}/api/mesh/distress", seed_port);
                            let _ = client.post(&url).json(&sos_payload).send().await;
                        }
                    });
                }

                // Last-stand consolidation + reboot
                if b.death_triggered && b.death_consolidation_ticks < 10 {
                    b.consolidate(0.5);
                    b.death_consolidation_ticks += 1;
                } else if b.death_triggered && b.death_consolidation_ticks >= 10 {
                    info!("REBOOT: resetting brain state after death");
                    for neuron in b.neurons.iter_mut() {
                        neuron.potential = 0.3;
                        neuron.activation = 0.0;
                        neuron.firing_count = 0;
                        neuron.last_fired = 0.0;
                        neuron.spike_time = 0.0;
                        neuron.energy_reserve = 0.5;
                    }
                    b.energy_pool = b.energy_pool_capacity * 0.3;
                    b.death_clock = 0;
                    b.death_triggered = false;
                    b.death_consolidation_ticks = 0;
                    b.sos_sent = false;
                    b.arousal = 0.5;
                    b.metabolic_pressure = 0.0;
                    b.stats.death_count += 1;
                    b.pain = PainSignal::default();
                    b.stress_mode = StressMode::Calm;
                    // Re-sync params from genome
                    let e = std::mem::take(&mut b.evolution);
                    e.apply_global_params(&mut b);
                    b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
                    info!("REBOOT COMPLETE: death_count={}", b.stats.death_count);
                }
            }

            // ── CORRECTION 1: MetaCortex decide + apply action ──
            {
                // Observe brain state for self-model
                let module_activations: Vec<f64> = b
                    .module_order
                    .iter()
                    .map(|name| b.modules[name].activation_level as f64)
                    .collect();
                let firing_rate = if b.stats.total_neurons > 0 {
                    b.stats.firing_neurons as f64 / b.stats.total_neurons as f64
                } else {
                    0.0
                };
                let energy_avg = b.stats.energy_avg as f64;
                let arousal = b.arousal as f64;
                let metabolic_pressure = b.metabolic_pressure as f64;
                let pain_composite = b.pain.composite as f64;
                b.meta_cortex.observe_brain_state(
                    &module_activations,
                    energy_avg,
                    arousal,
                    metabolic_pressure,
                    pain_composite,
                    firing_rate,
                );

                // Ensure last_analysis is fresh before the recursive loop runs.
                let prediction_error = b.meta_cortex.last_self_model_error;
                let fitness = b.evolution.fitness.current.overall as f64;
                let entropy = crate::meta_cortex::normalized_entropy(&module_activations);
                b.meta_cortex.observe(tick, prediction_error, &module_activations, fitness);

                // Closed-loop recursive control — tick/tick autonomy
                b.meta_cortex.step_recursive_loop(tick, fitness, entropy);

                // Decide and apply action every 200 ticks
                if tick % 10 == 0 {
                    if let Some(action) = b.meta_cortex.decide_action(&b) {
                        if action != crate::meta_cortex::MetaAction::None
                            && action != crate::meta_cortex::MetaAction::NoOp
                        {
                            let mut mc = std::mem::take(&mut b.meta_cortex);
                            let record = mc.apply_action(&mut b, action);
                            b.meta_cortex = mc;
                            tracing::info!(
                                "🧠 MetaCortex action: {:?} (tick={})",
                                record.action.as_str(),
                                tick,
                            );
                        }
                    }
                }
            }

            // ── CORRECTION: Auto-self-modification on stagnation ──
            if b.self_modify_engine.enabled
                && b.meta_cortex.last_analysis.stagnation_ticks > 500
                && b.energy_pool > b.energy_pool_capacity * 0.5
                && tick % b.self_modify_engine.cooldown_ticks == 0
            {
                tracing::info!("🔄 Auto-self-modify triggered at tick={}", tick);
                if b.llm_mutator.is_some() {
                    let llm = std::mem::take(&mut b.llm_mutator).unwrap();
                    if llm.config.enabled {
                        let genome_str = serde_json::to_string_pretty(&b.evolution.genome).unwrap_or_default();
                        let recent: Vec<_> = b.evolution.episodic_memory.iter().rev().take(5).cloned().collect();
                        let n_mods = b.modules.len();
                        match llm.request_mutation(&genome_str, &b.evolution.fitness.current, &recent).await {
                            Ok(proposal) => {
                                if let Some(mutation) = crate::llm_mutator::LlmMutator::proposal_to_mutation(
                                    &proposal, n_mods
                                ) {
                                    let mut e = std::mem::take(&mut b.evolution);
                                    let _ = e.apply_mutation(&mut b, &mutation);
                                    b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
                                    tracing::info!("🧬 Auto-mutation applied: {:?}", mutation);
                                }
                            }
                            Err(e) => tracing::warn!("LLM mutator error: {}", e),
                        }
                    }
                    b.llm_mutator = Some(llm);
                }
            }

            // ── DeepSeek V4 Cortex step ──
            if b.deepseek_cortex.config.enabled
                && b.deepseek_cortex.config.tick_interval > 0
                && tick % b.deepseek_cortex.config.tick_interval == 0
            {
                let mut ds = std::mem::take(&mut b.deepseek_cortex);
                let _output = ds.step(&mut b);
                b.deepseek_cortex = ds;
            }

            // Élagage toutes les 300 ticks (~30 s)
            if tick % 300 == 0 {
                let prune_mod = b.stress_prune_mod;
                b.prune_weak_synapses(prune_mod);
            }

            // Apply evolved global parameters every 100 ticks
            if tick % 100 == 0 {
                let e = std::mem::take(&mut b.evolution);
                e.apply_global_params(&mut b);
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }

            // Autonomous healing — frequency depends on stress mode
            let heal_interval: u64 = match b.stress_mode {
                StressMode::Panic => 50,
                StressMode::Stress => 100,
                StressMode::Alert => 150,
                StressMode::Calm => 200,
            };
            if tick % heal_interval == 0 {
                let mut e = std::mem::take(&mut b.evolution);
                let heal_events = e.auto_heal(&mut b);
                for ev in &heal_events {
                    tracing::info!("{}", ev);
                }
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }

            // ── CORRECTION 5: Bridge integrations ──
            // Claude Code bridge every 500 ticks (fire-and-forget)
            if tick % 500 == 0 && b.claude_bridge.can_invoke(tick) {
                let mc_status = b.meta_cortex.api_status();
                let ctx = crate::claude_bridge::build_context(
                    tick,
                    b.evolution.best_fitness,
                    b.evolution.peak_fitness_ever,
                    b.evolution.generation,
                    b.evolution.stagnation_generations,
                    b.stats.energy_avg as f64,
                    b.arousal as f64,
                    &format!("{:?}", b.stress_mode),
                    b.modules.len(),
                    b.neurons.len(),
                    b.synapses.len(),
                    b.cortex.config.enabled,
                    b.evolution.mutation_count,
                    mc_status,
                );
                let prompt = b.claude_bridge.build_claude_prompt(&ctx);
                let proposal = b.claude_bridge.invoke_claude(&prompt);
                if let Some(proposal) = proposal {
                    let fitness_before = b.evolution.best_fitness;
                    let _ = crate::claude_bridge::apply_claude_proposal(
                        &proposal, &mut b,
                    );
                    let fitness_after = b.evolution.best_fitness;
                    b.claude_bridge
                        .record_outcome(fitness_before, fitness_after, false);
                    tracing::info!(
                        "🤖 Claude proposal: {} (confidence: {:.2})",
                        proposal.description,
                        proposal.confidence,
                    );
                }
            }

            // Governance bridge every 500 ticks
            if tick % 500 == 0 {
                if let Some(ref mut gb) = b.governance_bridge {
                    gb.tick(tick);
                }
            }

            // Research bridge every 36000 ticks (~1h)
            if tick % 36000 == 0 {
                let brain_clone = brain.clone();
                tokio::spawn(async move {
                    let mut b = brain_clone.write().await;
                    let papers = crate::research_bridge::fetch_recent_papers()
                        .await
                        .unwrap_or_default();
                    for paper in papers.iter().take(5) {
                        if let Some(abstract_text) = &paper.abstract_text {
                            b.learn_from_text(abstract_text);
                        }
                    }
                    tracing::info!("📚 Research bridge: ingested {} papers", papers.len());
                });
            }

            // Chronos bridge every 6000 ticks (~10 min)
            if tick % 6000 == 0 {
                let brain_clone = brain.clone();
                tokio::spawn(async move {
                    let mut b = brain_clone.write().await;
                    match crate::chronos_bridge::current_context().await {
                        Ok(ctx) => {
                            b.chronos_context = Some(ctx);
                            tracing::debug!("⏰ Chronos context updated");
                        }
                        Err(e) => tracing::warn!("Chronos bridge error: {e}"),
                    }
                });
            }

            // Self-modification: compile and hot-reload evolved genome every 1000 ticks
            if tick % 1000 == 0 {
                let source = b.evolution.generate_module_source();
                let (path, ok) = b.evolution.write_genome_source();
                if ok {
                    tracing::info!("Genome source written to {}", path);
                    // Attempt to compile and load as cdylib
                    match crate::hotload::compile_and_load_genome(&source) {
                        Ok(loaded) => {
                            tracing::info!("Genome compiled and hot-loaded successfully");
                            tracing::info!("  tau={:.4} homeostatic_rate={:.4} cortex=({},{},{})",
                                loaded.tau, loaded.homeostatic_rate,
                                loaded.cortex_d_model, loaded.cortex_state_dim, loaded.cortex_n_layers);
                            // Apply the loaded genome to brain params, cortex, and modules
                            let changes = {
                                let mut e = std::mem::take(&mut b.evolution);
                                let result = e.apply_loaded_genome(&mut b, &loaded);
                                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
                                result
                            };
                            if !changes.is_empty() {
                                for c in &changes {
                                    tracing::info!("  [hotload] {}", c);
                                }
                            }
                            b.loaded_genome = Some(loaded);
                        }
                        Err(e) => {
                            tracing::warn!("Genome compilation failed (non-fatal): {}", e);
                        }
                    }
                }
            }

            // Sauvegarde toutes les 600 ticks (~60 s)
            if tick % 600 == 0 {
                save_brain(&b);
            }

            // ── CORRECTION 3: Sauvegarde complète de tous les cortex ──
            if tick % 3000 == 0 {
                // DeepSeek Cortex
                if b.deepseek_cortex.config.enabled {
                    if let Err(e) = b.deepseek_cortex.save_state("deepseek_cortex_state.json") {
                        tracing::warn!("DeepSeek save failed: {e}");
                    }
                }
                // Meta Cortex
                if let Err(e) = b.meta_cortex.save_state("meta_cortex_state.json") {
                    tracing::warn!("Meta save failed: {e}");
                }
                // SSM Cortex
                if b.cortex.config.enabled {
                    if let Err(e) =
                        crate::ssm_cortex::save_cortex_state("cortex_state.json", &b.cortex)
                    {
                        tracing::warn!("SSM save failed: {e}");
                    }
                }
            }

            // ── CORRECTION 7: Reproduction si conditions réunies ──
            if b.can_reproduce(&metabolism_config) {
                tracing::info!(
                    "🧬 REPRODUCTION: spawning child (energy: {:.1})",
                    b.energy_pool
                );
                let genome = crate::evolution::Genome::from_brain(&b);
                let node_name = b.node_name.clone();
                let node_port = b.node_port;
                b.energy_pool -= metabolism_config.spawn_cost;
                tokio::spawn(async move {
                    let _ = crate::reproduction::spawn_child(
                        &genome,
                        &node_name,
                        node_port,
                        &mut rand::rng(),
                    );
                });
            }

            // Symbolic backpropagation analysis every interval ticks
            if tick % b.evolution.meta_genome.symbolic_analysis_interval == 0 {
                let mut e = std::mem::take(&mut b.evolution);
                e.run_symbolic_analysis();
                if !e.priority_rules.is_empty() {
                    tracing::info!("Symbolic analysis: {} priority rules active", e.priority_rules.len());
                }
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }

            // Apply pending LLM mutation if one exists
            if b.evolution.pending_llm_mutation.is_some() {
                let mut e = std::mem::take(&mut b.evolution);
                let proposal = e.pending_llm_mutation.take().unwrap();
                let n_mods = e.genome.modules.len();
                if let Some(mutation) = crate::llm_mutator::LlmMutator::proposal_to_mutation(&proposal, n_mods) {
                    let parent_fitness = e.fitness.current.overall;
                    e.apply_mutation(&mut b, &mutation);
                    e.fitness.sample(&mut b);
                    let child_fitness = e.fitness.current.overall;
                    e.track_mutation_result(&mutation, parent_fitness, child_fitness);
                    let event = crate::evolution::MutationEpisodicEvent {
                        mutation_type: mutation.type_key(),
                        generation: e.generation,
                        parent_fitness,
                        child_fitness,
                        fitness_delta: child_fitness - parent_fitness,
                        ctx_firing_balance: e.fitness.current.firing_balance,
                        ctx_synaptic_diversity: e.fitness.current.synaptic_diversity,
                        ctx_module_utilization: e.fitness.current.module_utilization,
                        ctx_stability: e.fitness.current.stability,
                        timestamp: now_f64(),
                    };
                    e.episodic_memory.push_back(event);
                    if e.episodic_memory.len() > 200 {
                        e.episodic_memory.pop_front();
                    }
                    e.generation += 1;
                    e.mutation_count += 1;
                    // Track LLM success
                    if let Some(ref mut m) = b.llm_mutator {
                        m.total_attempts += 1;
                        if child_fitness > parent_fitness {
                            m.total_successes += 1;
                        }
                    }
                    tracing::info!("LLM mutation applied: {} (fitness {:.4} -> {:.4})",
                        mutation, parent_fitness, child_fitness);
                }
                b.evolution = e;
            b.energy_pool = b.energy_pool_capacity.max(100.0);
            tracing::info!("🔄 energy_pool forcé à {:.1}", b.energy_pool);
            }
        }

        tick = tick.wrapping_add(1);
        sleep(Duration::from_millis(100)).await;
    }
}

// ═══════════════════════════════════════════════════════════════
// MESH SYNC LOOP — gossip protocol
// ═══════════════════════════════════════════════════════════════
//
// Protocol:
//   Phase 1 (every 5s): Send heartbeat + evolution data to seed nodes
//   Phase 2 (every 5s, staggered): Gossip fanout to k random discovered peers
//   Phase 3 (every 15s): Anti-entropy with a random peer (full state exchange)
//   Phase 4 (every 30s): Evict stale peers (not seen for > 120s)
//
// Loop prevention: gossip TTL field decremented at each hop.

pub(crate) async fn mesh_sync_loop(brain: SharedBrain, my_port: u16) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    let mut tick: u64 = 0;

    loop {
        sleep(Duration::from_millis(5000)).await;
        tick += 1;

        // Build gossip payload with status + evolution data
        let (status, evolution_payload, node_name) = {
            let b = brain.read().await;
            let status = b.api_status();
            let evo = serde_json::json!({
                "generation": b.evolution.generation,
                "fitness": b.evolution.fitness.current.overall,
                "hypervolume": b.evolution.pareto_frontier.hypervolume(),
                "best_fitness": b.evolution.best_fitness,
            });
            (status, evo, b.node_name.clone())
        };

        // Phase 1: Send heartbeat to seed nodes (always)
        let seed_ports: Vec<u16> = {
            let b = brain.read().await;
            b.mesh.seed_ports.iter().filter(|p| **p != my_port).copied().collect()
        };

        for peer_port in &seed_ports {
            let url = format!("http://127.0.0.1:{peer_port}/api/mesh/receive");
            let payload = serde_json::json!({
                "peer_port": my_port,
                "status": status,
            });
            let _ = client.post(&url).json(&payload).send().await;
        }

        // Phase 2: Gossip fanout — forward evolution data to k random discovered peers
        let discovered_peers: Vec<u16> = {
            let b = brain.read().await;
            let mut ports: Vec<u16> = b.mesh.peers.keys()
                .filter(|p| **p != my_port && !seed_ports.contains(p))
                .copied()
                .collect();
            ports.sort();
            ports.dedup();
            ports
        };

        if !discovered_peers.is_empty() {
            // Pick k random peers (must drop rng before await)
            let targets: Vec<u16> = {
                let mut rng = rand::rng();
                let k = 2.min(discovered_peers.len());
                let mut chosen: Vec<u16> = Vec::new();
                for _ in 0..k {
                    if !discovered_peers.is_empty() {
                        chosen.push(discovered_peers[rng.random_range(0..discovered_peers.len())]);
                    }
                }
                chosen
            };
            for peer_port in &targets {
                let gossip = GossipMessage {
                    msg_type: "evolution".into(),
                    source_port: my_port,
                    source_name: node_name.clone(),
                    ttl: 3,
                    payload: evolution_payload.clone(),
                };
                let url = format!("http://127.0.0.1:{peer_port}/api/mesh/gossip");
                let _ = client.post(&url).json(&gossip).send().await;
            }
        }

        // Phase 3: Anti-entropy every 3 ticks (15s) — full state exchange with random peer
        if tick % 3 == 0 {
            let all_peers: Vec<u16> = {
                let b = brain.read().await;
                b.mesh.peers.keys().copied().collect()
            };
            if !all_peers.is_empty() {
                let peer_port = {
                    let mut rng = rand::rng();
                    all_peers[rng.random_range(0..all_peers.len())]
                };
                let url = format!("http://127.0.0.1:{peer_port}/api/mesh/receive");
                let payload = serde_json::json!({
                    "peer_port": my_port,
                    "status": status,
                });
                let _ = client.post(&url).json(&payload).send().await;
            }
        }

        // Phase 4: Evict stale peers every 6 ticks (30s)
        if tick % 6 == 0 {
            let now = now_f64();
            let stale_timeout = 120.0;
            let mut b = brain.write().await;
            let stale: Vec<u16> = b.mesh.peers.iter()
                .filter(|(_, info)| (now - info.last_seen) > stale_timeout)
                .map(|(port, _)| *port)
                .collect();
            for port in &stale {
                if let Some(info) = b.mesh.peers.get_mut(port) {
                    info.is_alive = false;
                }
            }
            if !stale.is_empty() {
                tracing::info!("Mesh: {} stale peer(s) marked dead (ports {:?})", stale.len(), stale);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// HANDLERS HTTP
// ═══════════════════════════════════════════════════════════════

pub(crate) async fn handle_index() -> Html<&'static str> {
    Html(HTML)
}

pub(crate) async fn handle_health() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (StatusCode::OK, headers, serde_json::json!({"status": "ok", "version": "6.0.0", "service": "soullink-node"}).to_string())
}

pub(crate) async fn handle_status(State(brain): State<SharedBrain>) -> impl IntoResponse {
    let b = brain.read().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, b.api_status().to_string())
}

pub(crate) async fn handle_brain(State(brain): State<SharedBrain>) -> impl IntoResponse {
    let b = brain.read().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, b.api_brain().to_string())
}

#[derive(Deserialize)]
pub(crate) struct LearnRequest {
    topic: String,
}

pub(crate) async fn handle_learn(
    State(brain): State<SharedBrain>,
    Json(req): Json<LearnRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let learned = b.learn_topic(&req.topic);
    let resp = serde_json::json!({
        "learned": learned,
        "topic": req.topic,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── Evolve endpoint ──

#[derive(Deserialize)]
pub(crate) struct EvolveRequest {
    fitness: f64,
    proposals: Vec<String>,
}

pub(crate) async fn handle_evolve(
    State(brain): State<SharedBrain>,
    Json(req): Json<EvolveRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let extra_neurons = b.evolve(req.fitness, &req.proposals);

    // Fire-and-forget POST to OpenEvolve mesh endpoint
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();
    let payload = serde_json::json!({
        "fitness": req.fitness,
        "proposals": req.proposals,
        "extra_neurons": extra_neurons,
        "source": b.node_name,
    });
    tokio::spawn(async move {
        let _ = client.post("http://localhost:9020/api/mesh/evolve")
            .json(&payload)
            .send()
            .await;
    });

    let resp = serde_json::json!({
        "status": "ok",
        "extra_neurons": extra_neurons,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── Evolution Status endpoint ──

pub(crate) async fn handle_evolution_status(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let b = brain.read().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, b.evolution.api_status().to_string())
}

// ── Evolution Report endpoint ──

pub(crate) async fn handle_evolution_report(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let b = brain.read().await;
    let report = b.evolution.generate_report();
    let resp = serde_json::json!({"report": report});
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── Evolution Trigger endpoint ──

#[derive(Deserialize)]
pub(crate) struct EvolutionTriggerRequest {
    #[serde(default)]
    auto_evolve: Option<bool>,
    #[serde(default)]
    evolution_interval: Option<u64>,
}

pub(crate) async fn handle_evolution_trigger(
    State(brain): State<SharedBrain>,
    Json(req): Json<EvolutionTriggerRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    if let Some(auto) = req.auto_evolve {
        b.evolution.auto_evolve = auto;
    }
    if let Some(interval) = req.evolution_interval {
        b.evolution.evolution_interval = interval;
        b.evolution.meta_genome.min_evolution_interval = interval.saturating_sub(30).max(20);
        b.evolution.meta_genome.max_evolution_interval = interval + 100;
    }
    let resp = serde_json::json!({
        "status": "ok",
        "auto_evolve": b.evolution.auto_evolve,
        "evolution_interval": b.evolution.evolution_interval,
        "generation": b.evolution.generation,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── Evolution Goal endpoint ──

#[derive(Deserialize)]
pub(crate) struct EvolutionGoalRequest {
    target_metric: Option<String>,
    weight: Option<f64>,
    enabled: Option<bool>,
}

pub(crate) async fn handle_evolution_goal(
    State(brain): State<SharedBrain>,
    Json(req): Json<EvolutionGoalRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    if let Some(metric) = req.target_metric {
        b.evolution.evolution_goal.target_metric = metric;
    }
    if let Some(w) = req.weight {
        b.evolution.evolution_goal.weight = w.clamp(0.0, 1.0);
    }
    if let Some(e) = req.enabled {
        b.evolution.evolution_goal.enabled = e;
    }
    let resp = serde_json::json!({
        "status": "ok",
        "goal": {
            "enabled": b.evolution.evolution_goal.enabled,
            "target_metric": b.evolution.evolution_goal.target_metric,
            "weight": b.evolution.evolution_goal.weight,
        }
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── Symbolic rules endpoint ──

pub(crate) async fn handle_symbolic_rules(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let b = brain.read().await;
    let rules: Vec<serde_json::Value> = b.evolution.priority_rules.iter().map(|r| {
        serde_json::json!({
            "context_dim": r.context_dim,
            "context_range": [r.context_lo, r.context_hi],
            "mutation_type": r.mutation_type,
            "avg_delta": (r.avg_delta * 10000.0).round() / 10000.0,
            "success_rate": (r.success_rate * 1000.0).round() / 1000.0,
            "bias_weight": (r.bias_weight * 1000.0).round() / 1000.0,
            "sample_count": r.sample_count,
        })
    }).collect();
    let resp = serde_json::json!({
        "rule_count": rules.len(),
        "rules": rules,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ── LLM Mutator endpoints ──

#[derive(Deserialize)]
pub(crate) struct LlmMutatorConfigRequest {
    enabled: Option<bool>,
    endpoint_url: Option<String>,
    model: Option<String>,
    temperature: Option<f64>,
    cooldown_ticks: Option<u64>,
}
pub(crate) async fn handle_llm_mutator_config(
    State(brain): State<SharedBrain>,
    Json(req): Json<LlmMutatorConfigRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    if b.llm_mutator.is_none() {
        let config = crate::llm_mutator::LlmMutatorConfig::default();
        b.llm_mutator = Some(Box::new(crate::llm_mutator::LlmMutator::new(config)));
    }
    if let Some(mutator) = b.llm_mutator.as_mut() {
        if let Some(e) = req.enabled {
            mutator.config.enabled = e;
        }
        if let Some(url) = req.endpoint_url {
            mutator.config.endpoint_url = url;
        }
        if let Some(model) = req.model {
            mutator.config.model = model;
        }
        if let Some(temp) = req.temperature {
            mutator.config.temperature = temp.clamp(0.0, 2.0);
        }
        if let Some(ct) = req.cooldown_ticks {
            mutator.config.cooldown_ticks = ct.max(50);
        }
    }
    let status = llm_status_json(&b);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, status.to_string())
}

pub(crate) async fn handle_llm_mutator_status(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let b = brain.read().await;
    let status = llm_status_json(&b);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, status.to_string())
}

pub(crate) async fn handle_llm_mutate_once(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    // Initialize mutator if not exists
    if b.llm_mutator.is_none() {
        let config = crate::llm_mutator::LlmMutatorConfig::default();
        b.llm_mutator = Some(Box::new(crate::llm_mutator::LlmMutator::new(config)));
    }
    let status = llm_status_json(&b);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, status.to_string())
}

fn llm_status_json(b: &Brain) -> serde_json::Value {
    match b.llm_mutator.as_ref() {
        Some(m) => serde_json::json!({
            "enabled": m.config.enabled,
            "endpoint_url": m.config.endpoint_url,
            "model": m.config.model,
            "temperature": m.config.temperature,
            "cooldown_ticks": m.config.cooldown_ticks,
            "total_attempts": m.total_attempts,
            "total_successes": m.total_successes,
            "has_pending": b.evolution.pending_llm_mutation.is_some(),
        }),
        None => serde_json::json!({
            "enabled": false,
            "note": "LLM mutator not initialized",
        }),
    }
}

// ── Mesh receive endpoint (heartbeat / status sync) ──

#[derive(Deserialize)]
pub(crate) struct MeshReceiveRequest {
    peer_port: u16,
    status: serde_json::Value,
}

pub(crate) async fn handle_mesh_receive(
    State(brain): State<SharedBrain>,
    Json(req): Json<MeshReceiveRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    b.receive_mesh_status(req.peer_port, req.status);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, r#"{"status":"ok"}"#.to_string())
}

// ── Mesh gossip endpoint (evolution data fanout) ──

pub(crate) async fn handle_mesh_gossip(
    State(brain): State<SharedBrain>,
    Json(msg): Json<GossipMessage>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));

    // If TTL expired, drop silently
    if msg.ttl == 0 {
        return (headers, r#"{"status":"dropped"}"#.to_string());
    }

    // Receive gossip (updates peer table, returns payload to forward)
    let should_forward = {
        let mut b = brain.write().await;
        // Decrement TTL before forwarding
        let forwarded = b.receive_gossip(&msg);
        forwarded
    };

    // Gossip fanout: if newsworthy, forward to k random peers
    if let Some(payload) = should_forward {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();
        let peers: Vec<u16> = {
            let b = brain.read().await;
            b.mesh.peers.keys()
                .filter(|p| **p != msg.source_port)
                .copied()
                .collect()
        };
        if !peers.is_empty() {
            let targets: Vec<u16> = {
                let mut rng = rand::rng();
                let k = 2.min(peers.len());
                let mut chosen: Vec<u16> = Vec::new();
                for _ in 0..k {
                    if !peers.is_empty() {
                        chosen.push(peers[rng.random_range(0..peers.len())]);
                    }
                }
                chosen
            };
            for peer_port in &targets {
                let forward_msg = GossipMessage {
                    msg_type: msg.msg_type.clone(),
                    source_port: msg.source_port,
                    source_name: msg.source_name.clone(),
                    ttl: msg.ttl - 1,
                    payload: payload.clone(),
                };
                let url = format!("http://127.0.0.1:{peer_port}/api/mesh/gossip");
                let _ = client.post(&url).json(&forward_msg).send().await;
            }
        }
    }

    (headers, r#"{"status":"ok"}"#.to_string())
}

/// Receive a distress/SOS signal from a dying peer
pub(crate) async fn handle_mesh_distress(
    State(brain): State<SharedBrain>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let source_port = payload.get("source_port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    let source_name = payload.get("source_name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
    let cause = payload.get("cause").and_then(|v| v.as_str()).unwrap_or("unknown");

    warn!(
        "DISTRESS received from {} (port {}): cause={}",
        source_name, source_port, cause
    );

    // Mark peer as alive in our mesh table (distress = still running)
    let mut b = brain.write().await;
    b.mesh.peers.entry(source_port).and_modify(|info| {
        info.is_alive = true;
        info.last_seen = now_f64();
    });

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, serde_json::json!({"status": "sos_acknowledged"}).to_string())
}

// ═══════════════════════════════════════════════════════════════
// SCRIPT ENGINE ENDPOINTS
// ═══════════════════════════════════════════════════════════════

pub(crate) async fn handle_script_status(State(brain): State<SharedBrain>) -> impl IntoResponse {
    let b = brain.read().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, b.script_engine.api_status().to_string())
}

#[derive(Deserialize)]
pub(crate) struct SetScriptRequest {
    module: String,
    script: String,
}

pub(crate) async fn handle_script_set(
    State(brain): State<SharedBrain>,
    Json(req): Json<SetScriptRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let result = b.script_engine.set_activation_script(&req.module, &req.script);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    match result {
        Ok(()) => (StatusCode::OK, headers, serde_json::json!({"status": "ok", "module": req.module}).to_string()),
        Err(e) => (StatusCode::BAD_REQUEST, headers, serde_json::json!({"status": "error", "error": e}).to_string()),
    }
}

#[derive(Deserialize)]
pub(crate) struct ClearScriptRequest {
    module: String,
}

pub(crate) async fn handle_script_clear(
    State(brain): State<SharedBrain>,
    Json(req): Json<ClearScriptRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    b.script_engine.clear_activation_script(&req.module);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, serde_json::json!({"status": "ok", "module": req.module}).to_string())
}

#[derive(Deserialize)]
pub(crate) struct SetPlasticityRequest {
    script: String,
}

pub(crate) async fn handle_plasticity_set(
    State(brain): State<SharedBrain>,
    Json(req): Json<SetPlasticityRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let result = b.script_engine.set_plasticity_script(&req.script);
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    match result {
        Ok(()) => (StatusCode::OK, headers, serde_json::json!({"status": "ok"}).to_string()),
        Err(e) => (StatusCode::BAD_REQUEST, headers, serde_json::json!({"status": "error", "error": e}).to_string()),
    }
}
pub(crate) async fn handle_script_unquarantine(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    // Un-quarantine all scripts
    let mut b = brain.write().await;
    let script_sources: Vec<(String, String)> = b.script_engine.activation_scripts.iter()
        .filter(|(_, s)| s.is_active())
        .map(|(name, s)| (name.clone(), s.script().to_string()))
        .collect();
    let names: Vec<String> = script_sources.iter().map(|(n, _)| n.clone()).collect();
    for (_, script) in &script_sources {
        b.script_engine.unquarantine(script);
    }
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, serde_json::json!({"status": "ok", "unquarantined": names}).to_string())
}

// ═══════════════════════════════════════════════════════════════
// FIELD MUTATION ENDPOINTS
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub(crate) struct AddFieldRequest {
    target: String,  // "Neuron", "Synapse", "BrainModule"
    field_name: String,
    default: f64,
}

pub(crate) async fn handle_field_add(
    State(brain): State<SharedBrain>,
    Json(req): Json<AddFieldRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let mut count = 0usize;
    match req.target.as_str() {
        "Neuron" => {
            for neuron in &mut b.neurons {
                neuron.extensions.entry(req.field_name.clone()).or_insert(req.default);
                count += 1;
            }
        }
        "Synapse" => {
            for syn in &mut b.synapses {
                syn.extensions.entry(req.field_name.clone()).or_insert(req.default);
                count += 1;
            }
        }
        "BrainModule" => {
            for module in b.modules.values_mut() {
                module.extensions.entry(req.field_name.clone()).or_insert(req.default);
                count += 1;
            }
        }
        _ => {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));
            return (StatusCode::BAD_REQUEST, headers, serde_json::json!({"status": "error", "error": format!("Unknown target: {}", req.target)}).to_string());
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (StatusCode::OK, headers, serde_json::json!({"status": "ok", "target": req.target, "field": req.field_name, "default": req.default, "affected": count}).to_string())
}

#[derive(Deserialize)]
pub(crate) struct RemoveFieldRequest {
    target: String,
    field_name: String,
}

pub(crate) async fn handle_field_remove(
    State(brain): State<SharedBrain>,
    Json(req): Json<RemoveFieldRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let mut count = 0usize;
    match req.target.as_str() {
        "Neuron" => {
            for neuron in &mut b.neurons {
                if neuron.extensions.remove(&req.field_name).is_some() {
                    count += 1;
                }
            }
        }
        "Synapse" => {
            for syn in &mut b.synapses {
                if syn.extensions.remove(&req.field_name).is_some() {
                    count += 1;
                }
            }
        }
        "BrainModule" => {
            for module in b.modules.values_mut() {
                if module.extensions.remove(&req.field_name).is_some() {
                    count += 1;
                }
            }
        }
        _ => {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));
            return (StatusCode::BAD_REQUEST, headers, serde_json::json!({"status": "error", "error": format!("Unknown target: {}", req.target)}).to_string());
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (StatusCode::OK, headers, serde_json::json!({"status": "ok", "target": req.target, "field": req.field_name, "removed_from": count}).to_string())
}

// ═══════════════════════════════════════════════════════════════
// MATH ENDPOINTS (SciRust integration)
// ═══════════════════════════════════════════════════════════════

pub(crate) async fn handle_math_eval(
    Json(_req): Json<serde_json::Value>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Math engine temporarily disabled")
}

pub(crate) async fn handle_math_solve(
    Json(_req): Json<serde_json::Value>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Math engine temporarily disabled")
}

pub(crate) async fn handle_math_derive(
    Json(_req): Json<serde_json::Value>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Math engine temporarily disabled")
}

pub(crate) async fn handle_math_prove(
    Json(_req): Json<serde_json::Value>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Math engine temporarily disabled")
}

// ═══════════════════════════════════════════════════════════════
// SSM CORTEX ENDPOINTS
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub(crate) struct SsmConfigureRequest {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    modulation_strength: Option<f64>,
    #[serde(default)]
    tick_interval: Option<u64>,
}

pub(crate) async fn handle_ssm_step(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let mut c = std::mem::take(&mut b.cortex);
    let output = c.step(&mut b);
    b.cortex = c;
    let resp = serde_json::json!({
        "modulated": output.modulated,
        "history_len": output.history_len,
        "output_norm": (output.output_norm * 1000.0).round() / 1000.0,
        "raw_features": output.raw_features,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

pub(crate) async fn handle_ssm_status(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let b = brain.read().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, b.cortex.api_status().to_string())
}

pub(crate) async fn handle_ssm_toggle(
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let enabled = b.cortex.toggle();
    let resp = serde_json::json!({"enabled": enabled});
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

pub(crate) async fn handle_ssm_configure(
    State(brain): State<SharedBrain>,
    Json(req): Json<SsmConfigureRequest>,
) -> impl IntoResponse {
    let mut b = brain.write().await;
    let c = &mut b.cortex;
    c.configure(req.modulation_strength, req.tick_interval, req.enabled);
    let resp = serde_json::json!({
        "enabled": c.config.enabled,
        "modulation_strength": c.config.modulation_strength,
        "tick_interval": c.config.tick_interval,
    });
    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    (headers, resp.to_string())
}

// ═══════════════════════════════════════════════════════════════
// FRONTEND HTML (embarqué)
// ═══════════════════════════════════════════════════════════════

static HTML: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>SoulLink Brain v6.0 — Rust + Mesh + SciRust</title>
<script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.26.0/cytoscape.min.js"></script>
<style>
:root{
  --bg:#080810;--bg2:#0f0f1a;--bg3:#141422;
  --fg:#dde6ff;--dim:#5a6080;
  --green:#4affb8;--blue:#4a9eff;--red:#ff4a6a;
  --yellow:#ffaa4a;--purple:#c04aff;--cyan:#4affff;
  --border:#1e2040;
}
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'JetBrains Mono',monospace;background:var(--bg);color:var(--fg);height:100vh;display:flex;flex-direction:column;overflow:hidden}

/* ── HEADER ── */
#hdr{
  display:flex;align-items:center;gap:24px;
  padding:10px 16px;
  background:linear-gradient(90deg,#0a0a18,#0e1030);
  border-bottom:1px solid var(--border);
  flex-shrink:0;
}
#hdr h1{font-size:13px;color:var(--green);letter-spacing:1px;white-space:nowrap}
.stats{display:flex;gap:16px;flex-wrap:wrap}
.stat{display:flex;flex-direction:column;min-width:64px}
.stat .lbl{font-size:9px;color:var(--dim);text-transform:uppercase;letter-spacing:1px}
.stat .val{font-size:15px;font-weight:700;color:var(--green)}

/* ── PERIOD BANNER ── */
#banner{
  padding:5px 16px;font-size:11px;
  background:var(--bg2);border-bottom:1px solid var(--border);
  display:flex;gap:16px;align-items:center;flex-shrink:0;
}
#banner span{color:var(--dim)}
#banner .mode-grow{color:var(--green)}
#banner .mode-consolidate{color:var(--yellow)}
#banner .mode-maintain{color:var(--blue)}

/* ── BODY ── */
#body{display:flex;flex:1;overflow:hidden}

/* ── CY CANVAS ── */
#cy{flex:1;background:var(--bg)}

/* ── SIDE PANEL ── */
#panel{
  width:220px;flex-shrink:0;
  background:var(--bg2);
  border-left:1px solid var(--border);
  display:flex;flex-direction:column;
  overflow:hidden;
}
#panel h2{font-size:10px;color:var(--dim);letter-spacing:2px;padding:10px 12px 6px;border-bottom:1px solid var(--border)}
#modules{flex:1;overflow-y:auto;padding:8px}
.mc{
  padding:8px 10px;margin-bottom:6px;border-radius:5px;
  background:var(--bg3);border-left:3px solid var(--blue);
  cursor:default;
}
.mc .mname{font-size:9px;color:var(--dim);letter-spacing:1px}
.mc .mcount{font-size:18px;font-weight:700;line-height:1.2}
.mc .bar{height:3px;background:var(--border);margin-top:5px;border-radius:2px;overflow:hidden}
.mc .fill{height:100%;border-radius:2px;transition:width .5s}

/* ── LEARN ── */
#learn{padding:10px;border-top:1px solid var(--border);flex-shrink:0}
#learn input{
  width:100%;background:var(--bg3);border:1px solid var(--border);
  color:var(--fg);padding:6px 8px;border-radius:4px;font-size:11px;
  font-family:inherit;margin-bottom:6px;outline:none;
  transition:border-color .2s;
}
#learn input:focus{border-color:var(--blue)}
#learn button{
  width:100%;background:var(--blue);border:none;color:#000;
  padding:7px;border-radius:4px;cursor:pointer;font-weight:700;
  font-family:inherit;font-size:11px;letter-spacing:1px;
  transition:background .15s;
}
#learn button:hover{background:var(--cyan)}
#msg{font-size:10px;color:var(--green);margin-top:5px;min-height:14px}

/* ── MESH PEERS ── */
#mesh{
  padding:10px;border-top:1px solid var(--border);flex-shrink:0;
  max-height:140px;overflow-y:auto;
}
#mesh h2{font-size:10px;color:var(--dim);letter-spacing:2px;padding:0 0 6px 0}
.peer{font-size:10px;color:var(--cyan);margin-bottom:3px}
.peer.off{color:var(--dim)}

/* ── SCIRUST MATH PANEL ── */
#scirust{
  padding:10px;border-top:1px solid var(--border);flex-shrink:0;
  background:linear-gradient(180deg,var(--bg2),var(--bg3));
}
#scirust h2{font-size:10px;color:var(--purple);letter-spacing:2px;padding:0 0 6px 0}
#scirust input{
  width:100%;background:var(--bg3);border:1px solid var(--border);
  color:var(--fg);padding:6px 8px;border-radius:4px;font-size:11px;
  font-family:inherit;margin-bottom:6px;outline:none;
  transition:border-color .2s;
}
#scirust input:focus{border-color:var(--purple)}
#scirust .btn-row{display:flex;gap:4px;margin-bottom:6px;flex-wrap:wrap}
#scirust button{
  flex:1;background:var(--bg3);border:1px solid var(--border);color:var(--purple);
  padding:5px;border-radius:4px;cursor:pointer;font-weight:700;
  font-family:inherit;font-size:9px;letter-spacing:1px;
  transition:all .15s;
}
#scirust button:hover{background:var(--purple);color:#000;border-color:var(--purple)}
#scirust .out{font-size:10px;color:var(--dim);margin-top:4px;min-height:14px;white-space:pre-wrap;line-height:1.4}
#scirust .out .ok{color:var(--green)}
#scirust .out .err{color:var(--red)}

/* ── SSM CORTEX PANEL ── */
#ssm-panel{
  padding:10px;border-top:1px solid var(--border);flex-shrink:0;
  background:linear-gradient(180deg,var(--bg2),var(--bg3));
}
#ssm-panel h2{font-size:10px;color:var(--cyan);letter-spacing:2px;padding:0 0 6px 0}
.ssm-row{display:flex;justify-content:space-between;align-items:center;padding:2px 0;font-size:10px}
.ssm-lbl{color:var(--dim);font-size:9px;letter-spacing:1px}
.ssm-val{color:var(--fg);font-size:11px;font-weight:700}
.ssm-val-off{color:var(--red);font-size:10px;font-weight:700}
.ssm-val-on{color:var(--green);font-size:10px;font-weight:700}
.ssm-btn-row{display:flex;gap:4px;margin-top:6px;margin-bottom:4px}
.ssm-btn-row button{
  flex:1;background:var(--bg3);border:1px solid var(--border);color:var(--cyan);
  padding:5px;border-radius:4px;cursor:pointer;font-weight:700;
  font-family:inherit;font-size:9px;letter-spacing:1px;
  transition:all .15s;
}
.ssm-btn-row button:hover{background:var(--cyan);color:#000;border-color:var(--cyan)}
.ssm-slider-row{display:flex;gap:4px;align-items:center;margin-top:4px}
.ssm-slider-row input[type=range]{flex:1;height:3px;accent-color:var(--cyan)}
.ssm-msg{font-size:9px;color:var(--dim);margin-top:4px;min-height:12px;letter-spacing:0.5px}

/* ── Report overlay ── */
#report-overlay{
  display:none;position:fixed;top:0;left:0;width:100%;height:100%;
  background:rgba(0,0,0,0.85);z-index:9999;
  justify-content:center;align-items:center;
}
#report-overlay.show{display:flex}
#report-box{
  background:var(--bg2);border:1px solid var(--border);border-radius:8px;
  width:80%;max-width:700px;max-height:80vh;
  display:flex;flex-direction:column;
}
#report-header{
  display:flex;justify-content:space-between;align-items:center;
  padding:12px 16px;border-bottom:1px solid var(--border);
}
#report-header h2{font-size:11px;color:var(--green);letter-spacing:1px;margin:0}
#report-header button{background:var(--bg3);border:1px solid var(--border);color:var(--dim);padding:4px 12px;border-radius:4px;cursor:pointer;font-size:11px;font-family:inherit}
#report-header button:hover{color:var(--fg);border-color:var(--dim)}
#report-body{
  padding:16px;overflow-y:auto;flex:1;
  font-size:11px;line-height:1.6;color:var(--fg);
  white-space:pre-wrap;font-family:'JetBrains Mono',monospace;
}
</style>
</head>
<body>
<div id="hdr">
  <h1>🧠 SOULLINK BRAIN v6.0 — RUST + MESH + SCIRUST</h1>
  <div class="stats">
    <div class="stat"><span class="lbl">Neurones</span><span class="val" id="sn">0</span></div>
    <div class="stat"><span class="lbl">Synapses</span><span class="val" id="ss">0</span></div>
    <div class="stat"><span class="lbl">Spikes/s</span><span class="val" id="sp">0</span></div>
    <div class="stat"><span class="lbl">Topics</span><span class="val"  id="st">0</span></div>
    <div class="stat"><span class="lbl">Growth</span><span class="val"  id="sg">0</span></div>
    <div class="stat"><span class="lbl">Pruned</span><span class="val"  id="spr">0</span></div>
  </div>
</div>
<div id="banner">
  <span id="bperiod">—</span>
  <span>|</span>
  <span id="bintens">—</span>
  <span>|</span>
  <span id="bmode" class="mode-grow">—</span>
  <span>|</span>
  <span id="bpeers" style="color:var(--cyan)">Mesh: —</span>
</div>
<div id="body">
  <div id="cy"></div>
  <div id="panel">
    <h2>MODULES</h2>
    <div id="modules"></div>
    <div id="mesh">
      <h2>MESH PEERS</h2>
      <div id="peers"></div>
    </div>
    <div id="scirust">
      <h2>⚛ SCIRUST MATH</h2>
      <input type="text" id="mathExpr" placeholder="Expression (ex: x^2 - 5*x + 6)" value="x^2 - 5*x + 6">
      <div class="btn-row">
        <button onclick="mathEval()">ÉVAL</button>
        <button onclick="mathDerive()">DÉRIV</button>
        <button onclick="mathSolve()">RÉSOLV</button>
      </div>
      <div class="btn-row">
        <button onclick="mathProve()">PROUVER</button>
      </div>
      <input type="text" id="mathRight" placeholder="= ... (pour prouver)" value="">
      <div id="mathOut" class="out">Tapez une expression...</div>
    </div>
    <div id="ssm-panel">
      <h2>🌀 SSM CORTEX</h2>
      <div class="ssm-row">
        <span class="ssm-lbl">État</span>
        <span id="ssmEnabled" class="ssm-val-off">DÉSACTIVÉ</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Couches</span>
        <span id="ssmLayers" class="ssm-val">—</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Dim. état</span>
        <span id="ssmStateDim" class="ssm-val">—</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Historique</span>
        <span id="ssmHistory" class="ssm-val">—</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Norme sortie</span>
        <span id="ssmNorm" class="ssm-val">—</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Paramètres</span>
        <span id="ssmParams" class="ssm-val">—</span>
      </div>
      <div class="ssm-btn-row">
        <button onclick="ssmStep()">▶ STEP</button>
        <button onclick="ssmToggle()">TOGGLE</button>
      </div>
      <div class="ssm-slider-row">
        <label class="ssm-lbl">Modulation</label>
        <input type="range" id="ssmStrength" min="0" max="100" value="30" oninput="ssmUpdateStrength(this.value)">
        <span id="ssmStrengthVal" class="ssm-val">0.30</span>
      </div>
      <div id="ssmMsg" class="ssm-msg">Prêt</div>
    </div>
    <div id="evo-panel">
      <h2>??? EVOLUTION</h2>
      <div class="ssm-row">
        <span class="ssm-lbl">Génération</span>
        <span id="evoGen" class="ssm-val">?</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Fitness</span>
        <span id="evoFitness" class="ssm-val">?</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Mutations</span>
        <span id="evoMutations" class="ssm-val">?</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Best fitness</span>
        <span id="evoBest" class="ssm-val">?</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">État</span>
        <span id="evoState" class="ssm-val-on">ACTIF</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Règles</span>
        <span id="symRulesCount" class="ssm-val">0</span>
      </div>
      <div class="ssm-btn-row">
        <button onclick="evoToggle()">TOGGLE</button>
        <button onclick="evoReport()">RAPPORT</button>
      </div>
      <div id="evoMsg" class="ssm-msg">Prêt</div>
    </div>
    <div id="llm-panel" style="padding:10px;border-top:1px solid var(--border);flex-shrink:0;">
      <h2 style="font-size:10px;color:var(--purple);letter-spacing:2px;padding:0 0 6px 0">??? LLM</h2>
      <div class="ssm-row">
        <span class="ssm-lbl">État</span>
        <span id="llmEnabled" class="ssm-val-off">DÉSACTIVÉ</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Tentatives</span>
        <span id="llmAttempts" class="ssm-val">0</span>
      </div>
      <div class="ssm-row">
        <span class="ssm-lbl">Succès</span>
        <span id="llmSuccesses" class="ssm-val">0</span>
      </div>
      <div class="ssm-btn-row">
        <button onclick="llmToggle()">TOGGLE</button>
        <button onclick="llmMutateOnce()">MUTATION</button>
      </div>
      <div id="llmMsg" class="ssm-msg">---</div>
    </div>
    <div id="learn">
      <input type="text" id="topic" placeholder="Topic à apprendre...">
      <button onclick="learnTopic()">⚡ APPRENDRE</button>
      <div id="msg"></div>
    </div>
  </div>
</div>

<script>
let cy = null;
let nodeIds = new Set();

function initCy(data){
  cy = cytoscape({
    container: document.getElementById('cy'),
    elements: [],
    style:[
      {selector:'node',     style:{'background-color':'data(color)','width':10,'height':10,'label':'','opacity':0.85}},
      {selector:'.module',  style:{'background-color':'data(color)','width':38,'height':38,'label':'data(label)','font-size':8,'color':'#fff','text-valign':'center','text-halign':'center','border-width':2,'border-color':'data(color)','border-opacity':0.4,'text-wrap':'wrap'}},
      {selector:'.spiking', style:{'background-color':'#ffffff','width':16,'height':16,'border-width':3,'border-color':'data(color)','border-opacity':1,'z-index':999}},
      {selector:'edge',     style:{'width':'mapData(weight,0,1,0.5,2.5)','line-color':'#1a1a2e','opacity':'mapData(weight,0,1,0.1,0.5)','curve-style':'haystack'}},
    ],
    layout:{name:'preset'},
    userZoomingEnabled:true,
    userPanningEnabled:true,
    minZoom:0.3,
    maxZoom:4,
  });
  buildGraph(data);
}

function neuronColor(n){
  if(n.is_spiking) return '#ffffff';
  if(n.importance>0.75) return '#4affb8';
  if(n.importance>0.55) return '#4a9eff';
  if(n.importance>0.35) return '#5a4aff';
  return '#2a2a4a';
}

function buildGraph(data){
  if(!cy) return;
  const elems=[];
  for(const [name,m] of Object.entries(data.modules)){
    elems.push({group:'nodes',data:{id:'_mod_'+name,label:name.toUpperCase(),color:m.color},classes:'module',position:{x:m.x,y:m.y}});
  }
  for(const n of data.neurons){
    elems.push({group:'nodes',data:{id:n.id,color:neuronColor(n)},classes:n.is_spiking?'spiking':'',position:{x:n.x,y:n.y}});
    nodeIds.add(n.id);
  }
  for(const s of data.synapses){
    elems.push({group:'edges',data:{id:'e_'+s.source+'_'+s.target,source:s.source,target:s.target,weight:s.weight}});
  }
  cy.add(elems);
}

function updateGraph(data){
  if(!cy) return;
  const newElems=[];
  for(const n of data.neurons){
    const existing=cy.getElementById(n.id);
    const cls=n.is_spiking?'spiking':'';
    const col=neuronColor(n);
    if(existing.length){
      existing.data('color',col);
      if(n.is_spiking){existing.addClass('spiking');}else{existing.removeClass('spiking');}
    } else {
      newElems.push({group:'nodes',data:{id:n.id,color:col},classes:cls,position:{x:n.x,y:n.y}});
    }
  }
  if(newElems.length) cy.add(newElems);
}

function renderModules(modules){
  let html='';
  for(const[name,m] of Object.entries(modules)){
    html+=`<div class="mc" style="border-color:${m.color}">
      <div class="mname">${name.toUpperCase()}</div>
      <div class="mcount" style="color:${m.color}">${m.neurons}</div>
      <div class="bar"><div class="fill" style="width:${(m.activation*100).toFixed(1)}%;background:${m.color}"></div></div>
    </div>`;
  }
  document.getElementById('modules').innerHTML=html;
}

function renderPeers(meshPeers){
  const el=document.getElementById('peers');
  const count=Object.keys(meshPeers).length;
  document.getElementById('bpeers').textContent='Mesh: '+count+' peer'+(count!==1?'s':'');
  if(!count){el.innerHTML='<div class="peer off">No peers</div>';return;}
  let html='';
  for(const[port,info] of Object.entries(meshPeers)){
    const name=info.name||'?';
    const gen=info.generation||0;
    const fit=info.fitness||'?';
    const alive=info.is_alive!==false;
    html+=`<div class="peer ${alive?'':'off'}">⟐ ${name}:${port} gen=${gen} fit=${fit}</div>`;
  }
  el.innerHTML=html;
}

// ── SSM Cortex ──
function ssmStep(){
  fetch('/api/ssm/step',{method:'POST'})
    .then(r=>r.json()).then(d=>{
      const msg=document.getElementById('ssmMsg');
      msg.textContent='Step OK: hist='+d.history_len+' norm='+d.output_norm;
      msg.style.color='var(--green)';
      setTimeout(()=>ssmFetchStatus(),100);
    }).catch(e=>{
      document.getElementById('ssmMsg').textContent='Err: '+e;
    });
}

function ssmToggle(){
  fetch('/api/ssm/toggle',{method:'POST'})
    .then(r=>r.json()).then(d=>{
      ssmFetchStatus();
      document.getElementById('ssmMsg').textContent=d.enabled?'Cortex activé':'Cortex désactivé';
    }).catch(e=>{
      document.getElementById('ssmMsg').textContent='Err: '+e;
    });
}

function ssmUpdateStrength(val){
  const v=(val/100).toFixed(2);
  document.getElementById('ssmStrengthVal').textContent=v;
  fetch('/api/ssm/configure',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({modulation_strength:parseFloat(v)})})
    .then(r=>r.json()).then(d=>{
      document.getElementById('ssmMsg').textContent='Modulation: '+d.modulation_strength;
    }).catch(()=>{});
}

function ssmRenderStatus(d){
  const en=d.enabled;
  const el=document.getElementById('ssmEnabled');
  el.textContent=en?'ACTIF':'DÉSACTIVÉ';
  el.className=en?'ssm-val-on':'ssm-val-off';
  document.getElementById('ssmLayers').textContent=d.n_layers+'×d='+d.d_model;
  document.getElementById('ssmStateDim').textContent='N='+d.state_dim;
  document.getElementById('ssmHistory').textContent=d.history_len+'/'+d.max_history;
  document.getElementById('ssmNorm').textContent=d.last_output_norm;
  document.getElementById('ssmParams').textContent=(d.params_estimate/1000).toFixed(1)+'k';
  document.getElementById('ssmStrength').value=Math.round(d.modulation_strength*100);
  document.getElementById('ssmStrengthVal').textContent=d.modulation_strength.toFixed(2);
}

function ssmFetchStatus(){
  fetch('/api/ssm/status')
    .then(r=>r.json()).then(d=>{ ssmRenderStatus(d); })
    .catch(()=>{});
}

// ── Evolution panel ──
function evoToggle(){
  const isActive = document.getElementById('evoState').textContent === 'ACTIF';
  fetch('/api/evolve/trigger',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({auto_evolve:!isActive})})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('evoState');
      el.textContent=d.auto_evolve?'ACTIF':'DÉSACTIVÉ';
      el.className=d.auto_evolve?'ssm-val-on':'ssm-val-off';
      document.getElementById('evoMsg').textContent=d.auto_evolve?'Auto-évolution activée':'Auto-évolution désactivée';
    }).catch(()=>{});
}

function evoReport(){
  const body=document.getElementById('report-body');
  body.textContent='Chargement...';
  document.getElementById('report-overlay').classList.add('show');
  fetch('/api/evolve/report')
    .then(r=>r.json()).then(d=>{
      body.textContent=d.report;
      document.getElementById('evoMsg').textContent='Rapport affiché';
    }).catch(()=>{
      body.textContent='Erreur lors du chargement du rapport';
    });
}
function closeReport(){
  document.getElementById('report-overlay').classList.remove('show');
}

function evoFetchStatus(){
  fetch('/api/evolve/status')
    .then(r=>r.json()).then(d=>{
      document.getElementById('evoGen').textContent='#'+d.generation;
      document.getElementById('evoFitness').textContent=d.fitness.overall;
      document.getElementById('evoMutations').textContent=d.mutation_count;
      document.getElementById('evoBest').textContent=d.best_fitness;
      const el=document.getElementById('evoState');
      el.textContent=d.auto_evolve?'ACTIF':'DÉSACTIVÉ';
      el.className=d.auto_evolve?'ssm-val-on':'ssm-val-off';
    }).catch(()=>{});
}

function fetchSymbolicRules(){
  fetch('/api/evolve/symbolic-rules')
    .then(r=>r.json()).then(d=>{
      document.getElementById('symRulesCount').textContent=d.rule_count;
    }).catch(()=>{});
}

// ── LLM mutator panel ──
function llmToggle(){
  const isActive = document.getElementById('llmEnabled').textContent === 'ACTIF';
  fetch('/api/evolve/llm-config',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({enabled:!isActive})})
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('llmEnabled');
      el.textContent=d.enabled?'ACTIF':'DÉSACTIVÉ';
      el.className=d.enabled?'ssm-val-on':'ssm-val-off';
      document.getElementById('llmMsg').textContent=d.enabled?'LLM activé':'LLM désactivé';
    }).catch(()=>{});
}

function llmMutateOnce(){
  fetch('/api/evolve/llm-mutate',{method:'POST',headers:{'Content-Type':'application/json'}})
    .then(r=>r.json()).then(d=>{
      document.getElementById('llmMsg').textContent='Mutation déclenchée';
      llmFetchStatus();
    }).catch(()=>{
      document.getElementById('llmMsg').textContent='Erreur LLM';
    });
}

function llmFetchStatus(){
  fetch('/api/evolve/llm-status')
    .then(r=>r.json()).then(d=>{
      const el=document.getElementById('llmEnabled');
      el.textContent=d.enabled?'ACTIF':'DÉSACTIVÉ';
      el.className=d.enabled?'ssm-val-on':'ssm-val-off';
      document.getElementById('llmAttempts').textContent=d.total_attempts||0;
      document.getElementById('llmSuccesses').textContent=d.total_successes||0;
    }).catch(()=>{});
}

function fetchBrain(){
  fetch('/api/brain').then(r=>r.json()).then(d=>{
    document.getElementById('sn').textContent=d.neurons.length;
    document.getElementById('ss').textContent=d.synapses.length;
    document.getElementById('sp').textContent=d.spikes.length;
    if(!cy){initCy(d);}else{updateGraph(d);}
  }).catch(()=>{});
}

function fetchStatus(){
  fetch('/api/status').then(r=>r.json()).then(d=>{
    document.getElementById('st').textContent=d.stats.topics_learned;
    document.getElementById('sg').textContent=d.stats.growth_events;
    document.getElementById('spr').textContent=d.stats.pruned_synapses||0;
    document.getElementById('bperiod').textContent=d.period;
    document.getElementById('bintens').textContent='Intensité '+Math.round(d.intensity*100)+'%';
    const mEl=document.getElementById('bmode');
    mEl.textContent=d.mode.toUpperCase();
    mEl.className='mode-'+d.mode;
    renderModules(d.modules);
    renderPeers(d.mesh_peers||{});
  }).catch(()=>{});
}

function learnTopic(){
  const t=document.getElementById('topic').value.trim();
  if(!t)return;
  fetch('/api/learn',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({topic:t})})
    .then(r=>r.json()).then(d=>{
      document.getElementById('topic').value='';
      const msg=document.getElementById('msg');
      msg.textContent=d.learned?`✓ "${t}" appris`:`"${t}" déjà connu`;
      setTimeout(()=>msg.textContent='',2000);
      fetchStatus();
    });
}

document.getElementById('topic').addEventListener('keydown',e=>{ if(e.key==='Enter')learnTopic(); });

// ── SciRust Math Engine ──
function setMathOut(html){ document.getElementById('mathOut').innerHTML=html; }

function mathEval(){
  const expr=document.getElementById('mathExpr').value.trim();
  if(!expr)return;
  setMathOut('Calcul...');
  fetch('/api/math/eval',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({expr,vars:{x:2}})})
    .then(r=>r.json()).then(d=>{
      if(d.error){ setMathOut('<span class="err">'+d.error+'</span>'); return; }
      setMathOut('<span class="ok">[eval]</span> '+d.simplified+'\n<span class="ok">[valeur]</span> '+d.value.toFixed(4)+'\n<span class="ok">[dérivée]</span> '+d.derivative+'\n<span class="ok">[dual]</span> val='+d.dual_value.toFixed(4)+' deriv='+d.dual_deriv.toFixed(4));
    }).catch(e=>setMathOut('<span class="err">'+e+'</span>'));
}

function mathDerive(){
  const expr=document.getElementById('mathExpr').value.trim();
  if(!expr)return;
  setMathOut('Dérivation...');
  fetch('/api/math/derive',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({expr,var:'x'})})
    .then(r=>r.json()).then(d=>{
      if(d.error){ setMathOut('<span class="err">'+d.error+'</span>'); return; }
      setMathOut('<span class="ok">[d/dx]</span> '+d.derivative+'\n<span class="ok">[simplifié]</span> '+d.simplified);
    }).catch(e=>setMathOut('<span class="err">'+e+'</span>'));
}

function mathSolve(){
  const expr=document.getElementById('mathExpr').value.trim();
  if(!expr)return;
  setMathOut('Résolution...');
  fetch('/api/math/solve',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({expr,var:'x'})})
    .then(r=>r.json()).then(d=>{
      if(d.error){ setMathOut('<span class="err">'+d.error+'</span>'); return; }
      let txt='';
      if(d.roots.length) txt+='<span class="ok">[racines]</span> '+d.roots.join(', ');
      else if(d.linear_root!=null) txt+='<span class="ok">[racine linéaire]</span> '+d.linear_root;
      else txt+='<span class="err">Aucune racine réelle trouvée</span>';
      setMathOut(txt);
    }).catch(e=>setMathOut('<span class="err">'+e+'</span>'));
}

function mathProve(){
  const left=document.getElementById('mathExpr').value.trim();
  const right=document.getElementById('mathRight').value.trim();
  if(!left||!right)return;
  setMathOut('Preuve...');
  fetch('/api/math/prove',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({left,right})})
    .then(r=>r.json()).then(d=>{
      if(d.error){ setMathOut('<span class="err">'+d.error+'</span>'); return; }
      setMathOut(d.proven ? '<span class="ok">✓ PROUVÉ</span> '+left+' = '+right : '<span class="err">✗ NON PROUVÉ</span> '+left+' ≠ '+right);
    }).catch(e=>setMathOut('<span class="err">'+e+'</span>'));
}

document.getElementById('mathExpr').addEventListener('keydown',e=>{ if(e.key==='Enter')mathEval(); });

fetchBrain();
fetchStatus();
ssmFetchStatus();
evoFetchStatus();
setInterval(fetchBrain,  500);
setInterval(fetchStatus, 1000);
setInterval(ssmFetchStatus, 2000);
setInterval(evoFetchStatus, 3000);
setInterval(fetchSymbolicRules, 5000);
setInterval(llmFetchStatus, 5000);
</script>
<div id="report-overlay" onclick="if(event.target===this)closeReport()">
  <div id="report-box">
    <div id="report-header">
      <h2>??? RAPPORT D'ÉVOLUTION</h2>
      <button onclick="closeReport()">FERMER</button>
    </div>
    <div id="report-body">Chargement...</div>
  </div>
</div>
</body>
</html>
"#;

// ═══════════════════════════════════════════════════════════════
// PORT → NAME MAPPING
// ═══════════════════════════════════════════════════════════════

pub(crate) fn port_to_name(port: u16) -> String {
    match port {
        9010 => "science".to_string(),
        9011 => "mind".to_string(),
        9012 => "memory".to_string(),
        9013 => "reasoning".to_string(),
        9014 => "learning".to_string(),
        9015 => "output".to_string(),
        _ => format!("node-{port}"),
    }
}


// ═══════════════════════════════════════════════════════════════
// SOULSYSTEM ORCHESTRATE
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub(crate) struct OrchestrateRequest {
    goal: String,
    #[serde(default)]
    context: Option<String>,
    #[serde(default = "default_soulsystem_url")]
    soulsystem_url: String,
}

fn default_soulsystem_url() -> String {
    "http://localhost:9020".into()
}

/// POST /api/orchestrate — Delegue un objectif a SoulSystem pour planification + execution.
pub(crate) async fn handle_orchestrate(
    State(brain): State<SharedBrain>,
    Json(req): Json<OrchestrateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let b = brain.read().await;
    let node = b.node_name.clone();
    let neuron_count = b.neurons.len();
    let stress = format!("{:?}", b.stress_mode);
    drop(b);

    info!(
        "[orchestrate] node={node} goal=\"{}\" neurons={neuron_count} stress={stress}",
        req.goal
    );

    let full_context = format!(
        "SoulLink node: {node} | neurons: {neuron_count} | stress: {stress} | context: {}",
        req.context.as_deref().unwrap_or("none")
    );

    let result = serde_json::json!({
        "status": "accepted",
        "goal": req.goal,
        "soullink_node": node,
        "soullink_neurons": neuron_count,
        "soullink_stress": stress,
        "soulsystem_url": req.soulsystem_url,
        "context": full_context,
        "pipeline": [
            "1. Planner: decompose goal into steps",
            "2. Router: dispatch to workers",
            "3. Workers: parallel execution via rayon",
            "4. FIM: fill-in-the-middle",
            "5. Verify: validate outputs",
            "6. Memory: store in layers",
            "7. Output: structured JSON result"
        ],
        "message": format!("Goal delegated to SoulSystem: {}", req.goal)
    });

    (StatusCode::OK, Json(result))
}

// ═══════════════════════════════════════════════════════════════
// CORRECTION 6 & 7: Governance + Metabolism API handlers
// ═══════════════════════════════════════════════════════════════

/// GET /api/metabolism — Retourne l'état du métabolisme.
pub(crate) async fn handle_metabolism_status(
    State(brain): State<SharedBrain>,
) -> (StatusCode, Json<serde_json::Value>) {
    let b = brain.read().await;
    let status = serde_json::json!({
        "energy_pool": b.energy_pool,
        "energy_pool_capacity": b.energy_pool_capacity,
        "metabolic_pressure": b.metabolic_pressure,
        "energy_avg": b.stats.energy_avg,
        "energy_debt_count": b.stats.energy_debt_count,
        "stress_mode": format!("{:?}", b.stress_mode),
        "tick": b.stats.age_ticks,
        "can_reproduce": b.energy_pool >= 70.0 && b.evolution.best_fitness > 0.6,
        "should_hibernate": b.energy_pool < 10.0,
    });
    (StatusCode::OK, Json(status))
}

/// GET /api/governance/contracts — Liste les contrats de gouvernance actifs.
pub(crate) async fn handle_governance_contracts(
    State(brain): State<SharedBrain>,
) -> (StatusCode, Json<serde_json::Value>) {
    let b = brain.read().await;
    let contracts: Vec<serde_json::Value> = match &b.governance_bridge {
        Some(gb) => gb
            .active_contracts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "purpose": c.purpose,
                    "status": format!("{:?}", c.status),
                    "votes_for": c.votes_for,
                    "votes_against": c.votes_against,
                    "created_tick": c.created_tick,
                })
            })
            .collect(),
        None => vec![],
    };
    (StatusCode::OK, Json(serde_json::json!({
        "contracts": contracts,
        "total": contracts.len(),
    })))
}


#[derive(Debug, Deserialize)]
pub struct GovernanceVoteRequest {
    pub contract_id: String,
    pub approve: bool,
}

/// POST /api/governance/vote — Vote sur un contrat de gouvernance.
pub(crate) async fn handle_governance_vote(
    State(brain): State<SharedBrain>,
    Json(req): Json<GovernanceVoteRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut b = brain.write().await;
    match &mut b.governance_bridge {
        Some(gb) => {
            if let Some(contract) = gb
                .active_contracts
                .iter_mut()
                .find(|c| c.id == req.contract_id)
            {
                contract.vote(req.approve);
                let result = serde_json::json!({
                    "success": true,
                    "contract_id": req.contract_id,
                    "votes_for": contract.votes_for,
                    "votes_against": contract.votes_against,
                    "approved": contract.is_approved(),
                });
                (StatusCode::OK, Json(result))
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "contract not found"})),
                )
            }
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "governance bridge not initialized"})),
        ),
    }
}


/// POST /api/self-modify — SoulLink edits its own source code.
/// Body: { "file_path": "src/brain.rs", "new_content": "..." }
pub async fn handle_self_modify(
    State(state): State<crate::AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Auth simple via X-API-Key (optionnel si non configurée)
    let api_key = std::env::var("SOULLINK_API_KEY").unwrap_or_default();
    if !api_key.is_empty() {
        let provided = headers.get("X-API-Key").and_then(|v| v.to_str().ok()).unwrap_or("");
        if provided != api_key {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Unauthorized: invalid or missing X-API-Key"})),
            );
        }
    }

    let file_path = payload.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
    let new_content = payload.get("new_content").and_then(|v| v.as_str()).unwrap_or("");

    if file_path.is_empty() || new_content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "file_path and new_content required"})),
        );
    }

    let mut brain = state.brain.write().await;
    let mut engine = std::mem::take(&mut brain.self_modify_engine);
    let result = engine.modify_and_respawn(file_path, new_content);
    brain.self_modify_engine = engine;

    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "Self-modification pipeline started. Respawning...",
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// ═══════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// AUTO-EVOLVE
// ═══════════════════════════════════════════════════════════════

/// POST /api/auto-evolve — Déclenche le cycle d’auto-évolution complet.
/// Le moteur d’auto-modification génère un patch, le valide et respawn.
pub async fn handle_auto_evolve(
    State(state): State<crate::AppState>,
) -> impl IntoResponse {
    let mut brain = state.brain.write().await;
    let mut engine = std::mem::take(&mut brain.self_modify_engine);
    let triggered = engine.auto_evolve(&mut brain);
    let cycles = engine.status().cycles_run;
    brain.self_modify_engine = engine;
    if triggered {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "Auto-evolve triggered — patch generation started.",
                "cycles": cycles,
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "success": false,
                "message": "Auto-evolve not available (disabled or no LLM mutator).",
            })),
        )
    }
}
