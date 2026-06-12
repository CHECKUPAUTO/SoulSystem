pub mod neuron;
pub mod synapse;
pub mod brain_module;
pub mod brain;
pub mod ssm_cortex;
pub mod deepseek_cortex;
pub mod script_engine;
pub mod migration;
pub mod evolution;
pub mod hotload;
pub mod llm_mutator;
pub mod api;
pub mod reproduction;
pub mod config;
pub(crate) mod research_bridge;
pub mod distill;
pub mod meta_cortex;
pub mod claude_bridge;
pub mod learning_journal;
pub mod metabolism;
pub mod python_bridge;
pub mod governance_bridge;
pub mod chronos_bridge;
pub mod openclaw_shim;
pub mod health;
pub mod autonomy;
pub mod self_modify;

// ── Re-exports for convenience (used as `crate::Type` across modules) ──
pub use brain::{Brain, BrainParams, NeuronIdx, SynapseIdx, SharedBrain, GossipMessage, save_brain};
pub use brain_module::{BrainModule, PainSignal, StressMode, Stats, Period};
pub use neuron::Neuron;
pub use synapse::Synapse;
pub use evolution::EvolutionEngine;
pub use llm_mutator::LlmMutator;
pub use brain::now_f64;

// ── Shared application state (used by both library and binary) ──
use axum::extract::FromRef;

#[derive(Clone)]
pub struct AppState {
    pub brain: brain::SharedBrain,
    pub http_client: reqwest::Client,
}

impl FromRef<AppState> for brain::SharedBrain {
    fn from_ref(state: &AppState) -> Self {
        state.brain.clone()
    }
}
