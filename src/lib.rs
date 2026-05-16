//! SoulSystem library — exposes shared types and utilities for
//! both the main binary and integration tests.

pub mod audit_log;
pub mod bus;
pub mod code_signing;
pub mod compute_backend;
pub mod config;
pub mod discovery;
pub mod federated_learning;
pub mod hardware_autoscaler;
pub mod jit_hnn;
pub mod meta_learning;
#[cfg(feature = "dev")]
pub mod skill_api;
pub mod skill_marketplace;
pub mod soul_memory;
pub mod soul_wallet;
pub mod swarm;
pub mod telemetry;

#[cfg(feature = "dev")]
pub mod dev_dashboard;
