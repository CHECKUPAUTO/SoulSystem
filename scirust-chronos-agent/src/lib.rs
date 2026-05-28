// ==========================================================================
// chronos-agent — ChronosAgent V5 Framework + Chronos-Lingua Bridge
//
// Modules
// - ptnl_perceiver  : Cross-attention latent encoder
// - memory          : Atemporal dual-store (episodic + semantic)
// - bci             : GRU-based recurrent cell with retro-causal insight
// - planner         : Stochastic diffusion trajectory planner
// - hypernetwork    : Error-conditioned MLP for latent space adjustment
// - coherence       : Continuous coherence loss (MSE)
// - projector       : TopologicalProjector + deep prefix-tuning (Pont Holonomique)
// - learning        : RegretOptimizer (Apprentissage A Posteriori)
// - observer        : Real-time PCA latent observer + mpsc export
// ==========================================================================

pub mod ptnl_perceiver;
pub mod memory;
pub mod bci;
pub mod planner;
pub mod hypernetwork;
pub mod coherence;
pub mod projector;
pub mod learning;
pub mod observer;
pub mod metacognition;
pub mod llm_bridge;
pub mod training;
pub mod checkpoint;
pub mod predictive_cache;
pub mod evopulse;
pub mod temporal_index;
pub mod memory_health;
pub mod consolidation;
pub mod memory_journal;
pub mod working_memory;
pub mod soul_bridge;
pub mod access_control;
pub mod sharded_index;
pub mod device;
