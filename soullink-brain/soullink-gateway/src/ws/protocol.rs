//! WebSocket protocol message types for the gateway RPC protocol.
//!
//! Matches the OpenClaw gateway protocol frame types:
//! - Connect / HelloOk — handshake
//! - Request / Response — RPC calls
//! - Event — server-sent events

use serde::{Deserialize, Serialize};

/// Client connection parameters (sent immediately after WS open).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    pub version: String,
    pub auth: Option<String>,
    pub client: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub name: Option<String>,
    pub platform: Option<String>,
}

/// Server response to successful connect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    pub session_id: String,
    pub version: String,
    pub methods: Vec<String>,
    pub events: Vec<String>,
}

/// RPC Request frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// RPC Response frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFrame {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    pub error: Option<ResponseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: String,
    pub message: String,
}

/// Server-sent event frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFrame {
    pub event: String,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Top-level WebSocket message (discriminated by `type` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    Connect(ConnectParams),
    HelloOk(HelloOk),
    Req(RequestFrame),
    Res(ResponseFrame),
    Event(EventFrame),
    Ping,
    Pong,
    Error {
        code: String,
        message: String,
        id: Option<String>,
    },
}
