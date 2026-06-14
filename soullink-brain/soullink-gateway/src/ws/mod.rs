//! WebSocket RPC server — a strict drop-in for the OpenClaw gateway protocol.
//!
//! ## Protocol (OpenClaw-compatible)
//!
//! 1. Client opens WebSocket, sends a `req` with `method: "connect"` carrying
//!    `ConnectRequest` (protocol range + auth token).
//! 2. Server validates the token and replies with a `res` whose payload is a
//!    `hello-ok` (protocol version, session_id, device_token, policy).
//! 3. Client sends further `req` frames (RPC calls); server replies with `res`.
//! 4. Server can push `event` frames at any time (e.g. streaming chat deltas).

pub mod auth;
pub mod handler;
pub mod protocol;
pub mod session;

/// WebSocket path registered on the axum router.
pub const WS_PATH: &str = "/ws";
