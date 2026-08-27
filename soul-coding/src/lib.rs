//! Contracts for the canonical SoulSystem coding harness.
//!
//! This crate deliberately contains policy-neutral data contracts and
//! filesystem boundary checks. It does not execute a model, a shell command,
//! or a Git operation yet. Keeping those concerns separate lets the next
//! implementation stages prove each side effect at a stable boundary.

#![deny(unsafe_code)]

pub mod contract;
pub mod feedback;
pub mod workspace;

pub use contract::{
    ChangeKind, ChangeSet, CheckResult, CheckSpec, CompletionError, TaskResult, TaskSpec,
    TaskStatus,
};
pub use feedback::{CodingFeedback, FeedbackError, FeedbackKind, PreferenceScope};
pub use workspace::{WorkspaceContext, WorkspaceError};
