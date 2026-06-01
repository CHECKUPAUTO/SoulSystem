//! SoulLink Autonomy v14 — The nervous system that makes the mesh alive.
//!
//! Core components:
//! - **Pulse**: 1Hz Hamiltonian evolution (Nesterov momentum + potential energy)
//! - **Afferent**: Hardware senses (GPU temp → neural turbulence)
//! - **DreamCycle**: Periodic random walk on MemoryGraph (semantic reinforcement)
//! - **Synapse**: OpenClaw message hook → mesh injection + memory ingestion
//! - **Preservation**: Self-preservation instinct — danger detection + auto-defense
//! - **Metacognition**: Self-model, introspection, capability awareness

pub mod afferent;
pub mod dream;
pub mod metacognition;
pub mod node;
pub mod preservation;
pub mod pulse;
pub mod synapse;

pub use afferent::AfferentNerve;
pub use dream::DreamCycle;
pub use metacognition::{Capability, IntrospectionReport, MetaCognition, SelfModel};
pub use node::BrainNode;
pub use preservation::{
    DangerEvent, DangerLevel, DangerType, DefenseAction, Preservation, PreservationConfig,
};
pub use pulse::AutonomyPulse;
pub use synapse::SynapseHook;
