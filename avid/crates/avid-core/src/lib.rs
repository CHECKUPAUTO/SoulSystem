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

//! Core application logic for the AVID engineering intelligence system.
//!
//! Contains the LLM-driven agent pipeline (Planner → CoreDesign → Critic),
//! task orchestrator with background workers, Redis/SQLite queue backends,
//! SQLite memory persistence, and Prometheus metrics instrumentation.
//!
//! # Architecture
//!
//! ```text
//! TaskMessage → Orchestrator → Planner → CoreDesign → Critic
//!                   │              │          │          │
//!                   ▼              ▼          ▼          ▼
//!                Queue         Plan JSON   Python     Quality
//!               (Redis/         schema     code +     report +
//!                SQLite)                   AST        score
//!                                          check
//! ```

pub mod agents;
pub mod config;
pub mod context_engine;
pub mod errors;
pub mod llm;
pub mod log_setup;
pub mod memory;
pub mod models;
pub mod orchestrator;
pub mod policy_engine;
pub mod queue;

// Re-export shared types from avid-anticlone for convenience
pub use avid_anticlone::{Fingerprint, Submission};

pub use config::Config;
