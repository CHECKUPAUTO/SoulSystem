//! SoulLink Actions — Async tool execution framework.
//!
//! Defines the `AsyncTool` trait and implements a safe `ShellExecutor`.

pub mod tool;
pub mod shell;

pub use tool::{AsyncTool, ToolResult, ToolError};
pub use shell::ShellExecutor;