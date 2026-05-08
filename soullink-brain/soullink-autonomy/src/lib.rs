//! SoulLink Autonomy — The nervous system that makes the mesh alive.
//!
//! Core components:
//! - **Pulse**: 1Hz Hamiltonian evolution (Nesterov momentum + potential energy)
//! - **Afferent**: Hardware senses (GPU temp → neural turbulence)
//! - **DreamCycle**: Periodic random walk on MemoryGraph (semantic reinforcement)
//! - **Synapse**: OpenClaw message hook → mesh injection + memory ingestion

pub mod pulse;
pub mod afferent;
pub mod dream;
pub mod node;
pub mod synapse;

pub use pulse::AutonomyPulse;
pub use afferent::AfferentNerve;
pub use dream::DreamCycle;
pub use node::BrainNode;
pub use synapse::SynapseHook;