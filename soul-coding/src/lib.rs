//! Contracts for the canonical SoulSystem coding harness.
//!
//! This crate owns the canonical coding-harness path: task contracts, Git
//! worktree identity, typed tool dispatch, sandboxed commands, model turns,
//! and evidence-based finalization. Keeping each side effect behind a small
//! boundary lets callers embed the same runtime in a CLI, daemon, or REPL.

#![deny(unsafe_code)]

pub mod agent;
pub mod command;
pub mod contract;
pub mod feedback;
pub mod git;
pub mod runner;
pub mod runtime;
pub mod session;
pub mod tools;
pub mod verify;
pub mod workspace;

pub use agent::{AgentError, CodingAgent, CodingAgentConfig, CodingAgentEvent};
pub use command::{CommandArg, CommandSpec, CommandSpecError};
pub use contract::{
    ChangeKind, ChangeSet, CheckResult, CheckSpec, CompletionError, TaskResult, TaskSpec,
    TaskStatus,
};
pub use feedback::{CodingFeedback, FeedbackError, FeedbackKind, PreferenceScope};
pub use git::{GitError, GitWorkspace};
pub use runner::{CommandOutput, CommandRunner, RunnerError, SandboxCommandRunner};
pub use runtime::{CodingRuntime, RuntimeError};
pub use session::{SessionError, SessionRecord, SessionStore, SESSION_SCHEMA_VERSION};
pub use soullink_gate::ExecutionMode;
pub use tools::{coding_tool_schemas, CodingToolExecutor, ToolExecutionResult};
pub use verify::{VerificationReport, Verifier};
pub use workspace::{WorkspaceContext, WorkspaceError};
