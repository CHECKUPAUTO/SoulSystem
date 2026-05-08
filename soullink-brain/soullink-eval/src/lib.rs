//! SoulLink Eval — Self-learning evaluation loop.
//!
//! BrainEvaluator computes semantic loss between query and response embeddings,
//! then applies gradient-like weight updates to the MemoryGraph.
//!
//! Loss = 1 - cosine_similarity(query_emb, response_emb)
//!
//! The cosine is computed via `soullink_simd::cosine_dispatch` which uses
//! AVX2 on Broadwell (64 threads) and falls back to scalar on other CPUs.

pub mod evaluator;
pub mod loss;
pub mod feedback;

pub use evaluator::BrainEvaluator;
pub use loss::SemanticLoss;
pub use feedback::FeedbackLoop;