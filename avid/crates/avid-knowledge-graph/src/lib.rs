//! AVID Knowledge Graph — Memory Tree and Knowledge Graph for the AVID system.
//!
//! This crate provides a full entity-relationship knowledge graph backed by SQLite,
//! compatible with the existing `MemoryStore` (same database, separate tables).
//!
//! # Features
//!
//! - **Entities**: Concepts, tools, tasks, patterns, agents, documents with typed properties
//! - **Relationships**: DependsOn, Implements, Uses, SimilarTo, Contradicts, Precedes, Contains
//! - **Graph traversal**: BFS, shortest path, cluster detection
//! - **LLM extraction**: Automatic entity/relationship extraction from natural text
//! - **Context generation**: Rich agent context with entity + relationships + dependencies
//! - **Statistics**: Graph density, degree distribution, type breakdowns
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use std::path::Path;
//! use avid_knowledge_graph::{KnowledgeGraph, Entity, EntityType, Relationship, RelType};
//!
//! let kg = KnowledgeGraph::new(Path::new("avid.db")).unwrap();
//!
//! let entity = Entity::new("MyConcept".into(), EntityType::Concept);
//! let id = kg.add_entity(entity).unwrap();
//!
//! let found = kg.get_entity(&id).unwrap();
//! assert!(found.is_some());
//! ```
//!
//! # Coexistence with MemoryStore
//!
//! Both `KnowledgeGraph` and `MemoryStore` can operate on the same SQLite database file
//! because they use different table prefixes (`kg_` vs the defaults in `MemoryStore`).

#![forbid(unsafe_code)]
#![deny(clippy::todo, clippy::unimplemented)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::unnecessary_cast,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::single_match,
    clippy::match_single_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::significant_drop_tightening,
    clippy::missing_const_for_fn,
    clippy::unnecessary_map_or,
    clippy::option_if_let_else,
    clippy::format_collect,
    clippy::needless_for_each,
    clippy::iter_on_single_items,
    clippy::cognitive_complexity
)]

pub mod entity;
pub mod extractor;
pub mod graph;
pub mod relationship;
pub mod stats;

// Re-export the most important types at the crate root
pub use entity::{Entity, EntityType};
pub use extractor::extract_from_text;
pub use graph::{KnowledgeContext, KnowledgeGraph};
pub use relationship::{RelType, Relationship};
pub use stats::GraphStats;

// Re-export extraction error for downstream error handling
pub use extractor::ExtractionError;
pub use graph::GraphError;
