//! SoulLink Actions — Async tool execution framework.
//!
//! Defines the `AsyncTool` trait and implements a safe `ShellExecutor`.

pub mod shell;
pub mod tool;

pub use shell::ShellExecutor;
pub use tool::{AsyncTool, ToolError, ToolResult};
