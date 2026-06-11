#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
pub use soulsystem_common::embedder::{
    compute_initial_importance, cosine_similarity, Embedder, NGramEmbedder, SciRustEmbedder,
};

mod persist;
pub use persist::*;

mod graph;
pub use graph::*;

pub mod store;
pub use store::*;

mod conversations;
pub use conversations::*;

#[cfg(feature = "web")]
mod rag;
#[cfg(feature = "web")]
pub use rag::*;
