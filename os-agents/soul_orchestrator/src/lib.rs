//! Ordonnancement evenementiel d'agents (routage par identite + cycle de vie).
pub mod orchestrator;
pub use orchestrator::{
    AgentState, DispatchOutcome, OrchestratorError, SovereignOrchestrator, BROADCAST,
};
