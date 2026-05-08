//! SoulLink Gateway — library exports for tests and potential external use.
//!
//! The binary entry point lives in `main.rs`. This lib module re-exports
//! the internals so integration tests under `tests/` can construct them
//! directly with mock servers.

pub mod config;
pub mod orchestrator_bridge;
pub mod streaming_consumer;
pub mod telegram;
