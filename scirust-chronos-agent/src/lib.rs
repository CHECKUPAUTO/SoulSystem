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
