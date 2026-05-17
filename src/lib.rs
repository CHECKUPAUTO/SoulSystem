//! SoulSystem library — exposes shared types and utilities for
//! both the main binary and integration tests.

#[cfg(feature = "dev")]
pub mod anomaly;
pub mod audit_log;
pub mod bus;
pub mod code_signing;
pub mod compute_backend;
pub mod config;
pub mod discovery;
pub mod soul_memory;
pub mod telemetry;

#[cfg(feature = "dev")]
pub mod dev_dashboard;
