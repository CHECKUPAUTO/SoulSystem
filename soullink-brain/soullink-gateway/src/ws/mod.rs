//! WebSocket RPC server — bidirectional protocol for gateway clients.
//!
//! ## Protocol
//!
//! 1. Client opens WebSocket, sends `Connect` with auth
//! 2. Server responds with `HelloOk` (session_id, capabilities)
//! 3. Client sends `Req` frames (RPC calls)
//! 4. Server responds with `Res` frames
//! 5. Server can push `Event` frames at any time

pub mod handler;
pub mod protocol;
pub mod session;

/// WebSocket path registered on the axum router.
pub const WS_PATH: &str = "/ws";
