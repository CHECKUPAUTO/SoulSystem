//! SoulLink Reasoning — Tree of Thoughts with eval-based pruning.
//!
//! Each node in the tree is a reasoning step that can be evaluated
//! by soullink-eval for semantic coherence with the original query.

pub mod node;
pub mod tree;

pub use node::ThoughtNode;
pub use tree::{ThoughtTree, TreeConfig};
