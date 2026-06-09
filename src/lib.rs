//! SoulSystem library — exposes shared types and utilities for
//! both the main binary and integration tests.

#[cfg(feature = "dev")]
pub mod anomaly;
pub use ansi_converter;
pub mod api;
pub mod audit_log;
pub mod backup;
pub mod bridge_store;
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
pub mod checkpoint_loader;
pub mod circuit_breaker;
pub mod compaction_watchdog;
pub mod continuous_summarizer;
pub mod memory_health;
pub mod memory_hub;
pub mod memory_suggest;
pub mod metrics;
pub mod rag_middleware;
pub mod sleep_cycle;
pub mod temporal_index;
pub use spinner;
pub mod telemetry;
pub use terminal_stream;
pub mod ws_bridge;

// Autonomous entity modules
pub mod autonomous;
pub mod autonomous_loop;

#[cfg(feature = "dev")]
pub mod dev_dashboard;
