use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "req")]
    Request(Request),
    #[serde(rename = "res")]
    Response(Response),
    #[serde(rename = "event")]
    Event(Event),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub ok: bool,
    #[serde(flatten)]
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsePayload {
    Success { payload: Value },
    Error { error: ErrorPayload },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub min_protocol: u16,
    pub max_protocol: u16,
    pub client: ClientInfo,
    pub role: Role,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub caps: Vec<String>,
    pub auth: AuthInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    pub typ: String,
    pub protocol: u16,
    pub session_id: String,
    pub device_token: String,
    pub policy: PolicyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub heartbeat_interval_ms: u64,
    pub max_message_size: usize,
    pub idle_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Operator,
    Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub fn error_response(id: &str, code: &str, message: &str) -> Response {
    Response {
        id: id.to_string(),
        ok: false,
        payload: ResponsePayload::Error {
            error: ErrorPayload {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            },
        },
    }
}

pub fn success_response(id: &str, payload: Value) -> Response {
    Response {
        id: id.to_string(),
        ok: true,
        payload: ResponsePayload::Success { payload },
    }
}
