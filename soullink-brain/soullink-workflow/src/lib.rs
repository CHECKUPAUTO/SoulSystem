//! SoulLink Workflow Engine — Deterministic AI coding workflows, Rust-native.
//!
//! Inspired by Archon (coleam00/Archon): YAML-defined workflows with
//! deterministic nodes (bash, tests, git ops) and AI nodes (planning,
//! code generation, review). Loops with conditions, validation gates,
//! git worktree isolation.

pub mod conditions;
pub mod config;
pub mod context;
pub mod engine;
pub mod git_worktree;
pub mod node;

pub use config::WorkflowConfig;
pub use context::WorkflowContext;
pub use engine::WorkflowEngine;
pub use node::{Node, NodeResult, NodeType};
pub use conditions::LoopCondition;