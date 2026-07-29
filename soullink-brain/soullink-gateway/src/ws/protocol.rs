//! WebSocket protocol — a **strict functional copy** of the OpenClaw gateway
//! wire protocol (`openclaw-gateway/src/protocol.rs`), so this gateway is a
//! drop-in replacement for the Node-distributed OpenClaw binary.
//!
//! Frame model (discriminated by `type`): `req` / `res` / `event`. The
//! handshake is a `req` with `method: "connect"`; the server replies with a
//! `res` whose payload is a `hello-ok`. This crate additionally dispatches a
//! superset of RPC methods (chat, completion, providers.list, …) which OpenClaw
//! clients simply never call — strict compatibility is preserved on the frames
//! and the handshake.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use soulsystem_common::secrets::ProtocolSecret;

/// Negotiated protocol version. Must match the OpenClaw reference exactly.
pub const PROTOCOL_VERSION: u16 = 3;

// Error codes — identical to the OpenClaw reference.
pub const ERR_INVALID_PARAMS: &str = "INVALID_PARAMS";
pub const ERR_UNSUPPORTED_PROTOCOL: &str = "UNSUPPORTED_PROTOCOL";
pub const ERR_AUTH_REQUIRED: &str = "AUTH_REQUIRED";
pub const ERR_AUTH_FAILED: &str = "AUTH_FAILED";
pub const ERR_UNKNOWN_METHOD: &str = "UNKNOWN_METHOD";

/// Top-level frame, discriminated by `type` = `req` | `res` | `event`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    #[serde(rename = "req")]
    Req(RequestFrame),
    #[serde(rename = "res")]
    Res(ResponseFrame),
    #[serde(rename = "event")]
    Event(EventFrame),
}

/// RPC request frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// RPC response frame: `{ id, ok, ...payload | ...error }` (flattened, matching
/// the OpenClaw reference exactly — no `null` payload/error fields on the wire).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFrame {
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

/// Server-sent event frame (used for streaming chat deltas).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFrame {
    pub event: String,
    #[serde(default)]
    pub payload: Value,
}

/// `connect` request params.
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

/// `hello-ok` payload returned on a successful `connect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    #[serde(rename = "type")]
    pub typ: String,
    pub protocol: u16,
    pub session_id: String,
    /// Sent to the client, which must present it back — so this is a
    /// `ProtocolSecret`, not a `SecretString`: redacting the wire form
    /// would hand every client `"<redacted>"` as its token.
    pub device_token: ProtocolSecret,
    pub policy: PolicyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub heartbeat_interval_ms: u64,
    pub max_message_size: usize,
    pub idle_timeout_ms: u64,
}

impl Default for PolicyInfo {
    fn default() -> Self {
        // Identical to the OpenClaw reference defaults.
        Self {
            heartbeat_interval_ms: 30_000,
            max_message_size: 65_536,
            idle_timeout_ms: 300_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Operator,
    Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    /// Presented by the client on the wire, so serialization must be faithful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<ProtocolSecret>,
}

/// Build an error `res` frame.
pub fn error_response(id: &str, code: &str, message: &str) -> WsMessage {
    WsMessage::Res(ResponseFrame {
        id: id.to_string(),
        ok: false,
        payload: ResponsePayload::Error {
            error: ErrorPayload {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            },
        },
    })
}

/// Build a success `res` frame.
pub fn success_response(id: &str, payload: Value) -> WsMessage {
    WsMessage::Res(ResponseFrame {
        id: id.to_string(),
        ok: true,
        payload: ResponsePayload::Success { payload },
    })
}

/// Build an `event` frame.
pub fn event(event: &str, payload: Value) -> WsMessage {
    WsMessage::Event(EventFrame {
        event: event.to_string(),
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_type_tags_match_openclaw() {
        let req = WsMessage::Req(RequestFrame {
            id: "1".into(),
            method: "connect".into(),
            params: json!({}),
        });
        let v: Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["type"], "req");
        assert_eq!(v["method"], "connect");
    }

    #[test]
    fn success_response_has_no_error_field() {
        let v: Value = serde_json::to_value(success_response("7", json!({"a": 1}))).unwrap();
        assert_eq!(v["type"], "res");
        assert_eq!(v["ok"], true);
        assert_eq!(v["payload"]["a"], 1);
        assert!(v.get("error").is_none(), "no null error field on the wire");
    }

    #[test]
    fn error_response_has_no_payload_field() {
        let v: Value = serde_json::to_value(error_response("7", ERR_AUTH_FAILED, "bad")).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "AUTH_FAILED");
        assert!(
            v.get("payload").is_none(),
            "no null payload field on the wire"
        );
    }

    #[test]
    fn hello_ok_shape_matches_reference() {
        let hello = HelloOk {
            typ: "hello-ok".into(),
            protocol: PROTOCOL_VERSION,
            session_id: "s1".into(),
            device_token: ProtocolSecret::new("oc_dev_x"),
            policy: PolicyInfo::default(),
        };
        let v = serde_json::to_value(&hello).unwrap();
        assert_eq!(v["type"], "hello-ok");
        assert_eq!(v["protocol"], 3);
        assert_eq!(v["policy"]["heartbeat_interval_ms"], 30000);
        assert_eq!(v["policy"]["max_message_size"], 65536);
        assert_eq!(v["policy"]["idle_timeout_ms"], 300000);
    }

    #[test]
    fn connect_request_deserializes_reference_wire_format() {
        let wire = json!({
            "min_protocol": 1, "max_protocol": 3,
            "client": {"id": "c1", "version": "1.0", "platform": "cli"},
            "role": "operator",
            "scopes": ["operator.admin"],
            "auth": {"token": "secret"}
        });
        let cr: ConnectRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(cr.max_protocol, 3);
        assert_eq!(cr.role, Role::Operator);
        assert_eq!(cr.auth.token.as_ref().map(|t| t.expose()), Some("secret"));
    }

    #[test]
    fn role_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(Role::Operator).unwrap(),
            json!("operator")
        );
        assert_eq!(serde_json::to_value(Role::Node).unwrap(), json!("node"));
    }

    /// The regression a naive `SecretString` migration would have introduced.
    ///
    /// `hello-ok` exists to hand the client the device token it must present
    /// back. A `Serialize` impl that redacts would send the literal string
    /// `"<redacted>"` as the token — every client would fail to authenticate,
    /// at runtime, with nothing failing to compile. Hence `ProtocolSecret`.
    #[test]
    fn hello_ok_puts_the_real_device_token_on_the_wire() {
        let hello = HelloOk {
            typ: "hello-ok".into(),
            protocol: 1,
            session_id: "s-1".into(),
            device_token: ProtocolSecret::new("oc_dev_realvalue"),
            policy: PolicyInfo {
                heartbeat_interval_ms: 1000,
                max_message_size: 1024,
                idle_timeout_ms: 5000,
            },
        };
        let json = serde_json::to_string(&hello).expect("serializes");
        assert!(
            json.contains("oc_dev_realvalue"),
            "the client must receive its actual token, got {json}"
        );
        assert!(!json.contains("redacted"), "got {json}");
    }

    /// ...while the Debug path, which is the accidental one, still redacts.
    #[test]
    fn hello_ok_debug_does_not_leak_the_device_token() {
        let hello = HelloOk {
            typ: "hello-ok".into(),
            protocol: 1,
            session_id: "s-1".into(),
            device_token: ProtocolSecret::new("oc_dev_realvalue"),
            policy: PolicyInfo {
                heartbeat_interval_ms: 1000,
                max_message_size: 1024,
                idle_timeout_ms: 5000,
            },
        };
        let rendered = format!("{hello:?}");
        assert!(!rendered.contains("oc_dev_realvalue"), "got {rendered}");
        assert!(rendered.contains("s-1"), "useful fields kept: {rendered}");
    }
}
