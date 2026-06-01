//! Soullink Brain v6.0 - Merged with SoulSystem v13.5
//!
//! This module contains:
//! - Soullink-SSM: Mamba-style State Space Models in Rust
//! - Soullink Core: Brain simulation, neurons, synapses, metabolism
//! - Soullink Evolution: Genome, fitness, Pareto, checkpoint/rollback
//! - Soullink Self-modify: Auto-code-modify engine
//! - Soullink Script Engine: Rhai sandbox for activation plasticity
//!
//! # Architecture
//!
//! ## SSM Components (from soullink-ssm)
//! - **HiPPO:** Optimal polynomial projection initialization for A matrix
//! - **SSM:** Core continuous-time state space model with ZOH discretization
//! - **SelectiveScan:** Input-dependent parameterization (Δ, B, C)
//! - **ParallelScan:** Associative scan for O(L log L) training
//! - **MambaBlock:** Full gated SSM layer with conv + SiLU + residual
//! - **MambaModel:** Stacked Mamba blocks with embedding/projection heads
//!
//! ## Core Components (from soullink-node)
//! - **Brain:** Core brain (33K neurons, modules, metabolism)
//! - **Neuron:** Neuron struct + dynamics
//! - **Synapse:** Synapse struct + STDP
//! - **Evolution:** Genome, fitness, Pareto, checkpoint/rollback
//! - **SSM Cortex:** Mamba SSM encode/forward/decode
//! - **Meta Cortex:** Self-model MLP + closed-loop control
//! - **Script Engine:** Rhai sandbox (activation + plasticity)
//! - **Self-modify:** Auto-code-modify engine (patch→build→respawn)

// SSM modules (from soullink-ssm)
pub mod cuda;
pub mod hippo;
pub mod layer;
pub mod model;
pub mod parallel;
pub mod quantization;
pub mod selective;
pub mod ssm;
pub mod tiled_scan;

// Core modules (from soullink-node)
pub mod brain;
pub mod neuron;
pub mod synapse;
pub mod brain_module;
pub mod ssm_cortex;
pub mod meta_cortex;
pub mod evolution;
pub mod self_modify;
pub mod script_engine;
pub mod deepseek_cortex;
pub mod config;
pub mod learning_journal;
pub mod llm_mutator;
pub mod migration;
pub mod hotload;
pub mod math_engine;
pub mod metabolism;
pub mod reproduction;
pub mod research_bridge;
pub mod distill;
pub mod python_bridge;
pub mod governance_bridge;
pub mod chronos_bridge;
pub mod openclaw_shim;
pub mod health;
pub mod autonomy;
pub mod claude_bridge;

// Re-export the public API
pub use hippo::HiPPOInitializer;
pub use layer::{GateType, MambaBlock, MambaConfig};
pub use model::MambaModel;
pub use parallel::{MultiDimScan, ParallelScan};
pub use quantization::QuantizedMatrix;
pub use selective::{MultiDimSelectiveSSM, SelectiveSSM};
pub use ssm::{discretize_zoh, matrix_exp, SSMConfig, SSMKernel};
pub use tiled_scan::TiledScan;

// Core re-exports
pub use brain::{Brain, BrainParams, NeuronIdx, SynapseIdx, SharedBrain, GossipMessage, save_brain};
pub use brain_module::{BrainModule, PainSignal, StressMode, Stats, Period};
pub use neuron::Neuron;
pub use synapse::Synapse;
pub use evolution::EvolutionEngine;
pub use llm_mutator::LlmMutator;
pub use brain::now_f64;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub brain: SharedBrain,
    pub http_client: reqwest::Client,
}

impl axum::extract::FromRef<AppState> for SharedBrain {
    fn from_ref(state: &AppState) -> Self {
        state.brain.clone()
    }
}
