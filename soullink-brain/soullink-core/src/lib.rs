//! SoulLink Core — shared primitives used by all binaries.
//!
//! Modules:
//! * [`proto`]       — wire types: SystemSnapshot, GuardianEvent
//! * [`auth`]        — Bearer token middleware (constant-time comparison)
//! * [`validation`]  — input validation helpers (domain names, paths…)
//! * [`secrets`]     — secret loader (`/etc/soullink/secrets.env`, mode 0600)

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms)]

pub mod auth;
pub mod mesh;
pub mod dynamics;
pub mod http;
pub mod ipc;
pub mod kmsg_parse;
pub mod proto;
pub mod secrets;
pub mod validation;

pub use proto::{GuardianEvent, SystemSnapshot};
