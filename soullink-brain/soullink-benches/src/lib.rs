//! SoulLink benches — criterion microbenchmarks.
//!
//! This crate exists only to host the `benches/` harness. All executable
//! code lives under `benches/*.rs`. Build & run with:
//!
//! ```bash
//! cargo bench --package soullink-benches
//! # Only one bench group:
//! cargo bench --package soullink-benches --bench xid_parse
//! # Include real NVML benches (on the T430 only):
//! NVML_BENCH_REAL=1 cargo bench --package soullink-benches --bench nvml_read
//! ```
