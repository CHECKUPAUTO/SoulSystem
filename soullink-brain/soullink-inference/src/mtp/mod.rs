//! Multi-Token Prediction (MTP) — Speculative Decoding Engine
//!
//! Implements the DeepSeek-V3 style MTP architecture:
//! N parallel prediction heads each propose K candidate continuations;
//! Tree Attention evaluates all speculative paths in a single forward pass;
//! Rejection Sampling validator accepts or rejects proposals;
//! Paged KV cache with O(1) rollback for rejected branches.
//!
//! # Architecture
//! ```text
//! Main Model(t) → MTP Head 0 → MTP Head 1 → ... → MTP Head N
//!       ↓              ↓              ↓                   ↓
//! Tree Attention Mask (causal tree: each node attends to ancestors only)
//!       ↓
//! MtpValidator (rejection sampling / argmax)
//!       ↓
//! Accepted: advance cache   |   Rejected: rollback cache
//! ```
//!
//! # Integration into an async inference loop
//! ```ignore
//! use soullink_inference::mtp::{
//!     KvCacheManager, MtpTreeAttention, MtpValidator, AcceptanceStrategy,
//! };
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! let cache = Arc::new(RwLock::new(
//!     KvCacheManager::new(n_layers, max_seq_len, head_dim, n_heads, tokens_per_page)
//! ));
//! let tree = MtpTreeAttention::new(num_steps, branch_factor);
//! let validator = MtpValidator::new(num_steps, vocab_size)
//!     .with_strategy(AcceptanceStrategy::RejectionSampling);
//!
//! // --- Per-step inference loop ---
//! async fn step(
//!     cache: &RwLock<KvCacheManager>,
//!     tree: &MtpTreeAttention,
//!     validator: &MtpValidator,
//!     mtp_drafts: &[usize],          // tokens proposed by MTP heads
//!     main_logits: &[f32],           // main model forward with tree mask
//!     k: &[f32], v: &[f32],          // new token K/V embeddings
//! ) -> usize {
//!     let result = validator.verify(mtp_drafts, main_logits).unwrap();
//!     let mut c = cache.write().await;
//!     c.checkpoint();                   // snapshot before speculative writes
//!     for t in &result.accepted_tokens {
//!         c.push(0, k, v);             // layer 0 = current
//!     }
//!     if result.all_accepted {
//!         c.commit();
//!         result.accepted_count
//!     } else {
//!         c.rollback_to_checkpoint();   // O(1) — no allocations
//!         c.commit();
//!         result.accepted_count
//!     }
//! }
//! ```

pub mod page_cache;
pub mod tree_attention;
pub mod validator;

pub use page_cache::KvCacheManager;
pub use tree_attention::MtpTreeAttention;
pub use validator::{AcceptanceStrategy, MtpValidator, VerificationResult};
