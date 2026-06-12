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
//! soullink-proxy — Reverse proxy and TLS gateway for SoulLink V13.5 APIs.
//!
//! Routes external HTTPS requests to internal SoulLink services:
//! - /api/mesh/*    → localhost:9020 (Orchestrator)
//! - /api/rag/*     → localhost:9020 (RAG)
//! - /api/autonomy/* → localhost:9046 (Autonomy)
//! - /monitoring/*  → localhost:9091 (Guardian)
//!
//! Features:
//! - Auto TLS with self-signed certs (or Let's Encrypt via rcgen)
//! - Bearer token auth passthrough
//! - CORS for web dashboard access
//! - Gzip compression
//! - Health check endpoint

pub mod config;
pub mod proxy;
pub mod routes;
pub mod tls;

pub use config::ProxyConfig;
pub use proxy::ProxyServer;
