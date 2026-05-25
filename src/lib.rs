//! SoulSystem library — exposes shared types and utilities for
//! both the main binary and integration tests.

#[cfg(feature = "dev")]
#[cfg(feature = "dev")]
pub mod anomaly;
pub use ansi_converter;
pub mod api;
pub mod audit_log;
pub mod backup;
pub use bound_system;
pub use bus;
pub use clawd;
pub mod code_signing;
pub mod compute_backend;
pub mod config;
pub mod discovery;
pub use local_skills;
pub use model_router;
pub use pty_terminal;
pub use soul_memory;
pub use spinner;
pub mod telemetry;
pub use terminal_stream;
pub mod ws_bridge;

#[cfg(feature = "dev")]
pub mod dev_dashboard;
