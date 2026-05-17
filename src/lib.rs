//! SoulSystem library — exposes shared types and utilities for
//! both the main binary and integration tests.

#[cfg(feature = "dev")]
#[cfg(feature = "dev")]
pub mod anomaly;
pub mod ansi_converter;
pub mod api;
pub mod audit_log;
pub mod backup;
pub mod bound_system;
pub mod bus;
pub mod clawd;
pub mod code_signing;
pub mod compute_backend;
pub mod config;
pub mod discovery;
pub mod local_skills;
pub mod model_router;
pub mod pty_terminal;
pub mod soul_memory;
pub mod spinner;
pub mod telemetry;
pub mod terminal_stream;
pub mod ws_bridge;

#[cfg(feature = "dev")]
pub mod dev_dashboard;
