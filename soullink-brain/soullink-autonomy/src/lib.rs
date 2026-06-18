//! SoulLink Autonomy v14 — The nervous system that makes the mesh alive.
//!
//! Core components:
//! - **Pulse**: 1Hz Hamiltonian evolution (Nesterov momentum + potential energy)
//! - **Afferent**: Hardware senses (GPU temp → neural turbulence)
//! - **DreamCycle**: Periodic random walk on MemoryGraph (semantic reinforcement)
//! - **Synapse**: OpenClaw message hook → mesh injection + memory ingestion
//! - **Preservation**: Self-preservation instinct — danger detection + auto-defense
//! - **Metacognition**: Self-model, introspection, capability awareness
//! - **ErrorMetrics**: Global error calculation for adaptive learning
//! - **RewardSystem**: Reward calculation for learning from actions and social interactions
//! - **PolicyEvolution**: Policy adaptation mechanisms

pub mod afferent;
pub mod dream;
pub mod metacognition;
pub mod node;
pub mod preservation;
pub mod pulse;
pub mod synapse;
pub mod error_metrics;
pub mod reward_system;
pub mod policy_evolution;

pub use afferent::AfferentNerve;
pub use dream::DreamCycle;
pub use metacognition::{Capability, IntrospectionReport, MetaCognition, SelfModel};
pub use node::BrainNode;
pub use preservation::{
    DangerEvent, DangerLevel, DangerType, DefenseAction, Preservation, PreservationConfig,
};
pub use pulse::AutonomyPulse;
pub use synapse::SynapseHook;
pub use error_metrics::{GlobalError, ErrorWeights, PredictionError, GoalError, SocialError, UncertaintyMetric};
pub use reward_system::{AgentReward, RewardWeights, ActionReward, SocialReward, InformationReward, SocialLearningReward};
pub use policy_evolution::{PolicyEvolution, ActionSelector, PolicyMetrics, ExplorationStrategy};
