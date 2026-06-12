#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
//! SoulLink Organs v7.0 — Unified organ library.
//!
//! Provides the shared foundation for all neural mesh organs.
//! Each organ is a lightweight HTTP server with standard mesh endpoints
//! plus organ-specific routes.

pub mod organ;
pub mod registry;
