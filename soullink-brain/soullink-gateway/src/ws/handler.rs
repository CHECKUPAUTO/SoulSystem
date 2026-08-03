//! WebSocket connection handler — manages the per-connection lifecycle.
//!
//! Flow (SoulSystem-compatible): upgrade → `req{method:"connect"}` → `res{hello-ok}`
//! → further `req` frames → `res`/`event` → close.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::stream::StreamExt;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use super::auth::AuthManager;
use super::protocol::*;
use super::session::SessionStore;
use crate::api::ApiState;
use crate::provider::{Provider, ProviderError};
use soulsystem_common::secrets::ProtocolSecret;

/// Thread-safe error type for WS handler.
type HandlerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Per-connection state.
struct ConnState {
    session_id: Option<String>,
    authenticated: bool,
}

/// Handle a single WebSocket connection (spawned per-upgrade).
pub async fn handle_connection(
    mut ws: WebSocket,
    store: Arc<SessionStore>,
    api: Arc<ApiState>,
    auth: Arc<AuthManager>,
    _shutdown: broadcast::Receiver<()>,
) {
    let mut state = ConnState {
        session_id: None,
        authenticated: false,
    };

    // Enforce the advertised policy: ping every `heartbeat_interval_ms` and
    // close the connection once it has been idle past `idle_timeout_ms`.
    let policy = PolicyInfo::default();
    let idle_timeout = std::time::Duration::from_millis(policy.idle_timeout_ms);
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(
        policy.heartbeat_interval_ms,
    ));
    heartbeat.tick().await; // consume the immediate first tick
    let mut last_activity = std::time::Instant::now();

    loop {
        tokio::select! {
            msg = ws.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = std::time::Instant::now();
                        if let Err(ref e) = handle_message(&mut ws, &mut state, &store, &api, &auth, &text).await {
                            warn!(err = %e, "message handler error");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_activity = std::time::Instant::now();
                        let _ = ws.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_activity = std::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("client sent close frame");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(err = %e, "websocket error");
                        break;
                    }
                    None => {
                        debug!("websocket stream ended");
                        break;
                    }
                    _ => {} // Binary — ignore
                }
            }
            _ = heartbeat.tick() => {
                if last_activity.elapsed() >= idle_timeout {
                    info!("idle timeout — closing ws connection");
                    break;
                }
                if ws.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal — closing ws connection");
                break;
            }
        }
    }

    if let Some(ref sid) = state.session_id {
        store.remove(sid).await;
        info!(session = %sid, "session cleaned up on disconnect");
    }
}

/// Route a single text message. Only `req` frames are client-originated; on a
/// malformed frame we mirror the SoulSystem reference and silently ignore it.
async fn handle_message(
    ws: &mut WebSocket,
    state: &mut ConnState,
    store: &SessionStore,
    api: &ApiState,
    auth: &AuthManager,
    text: &str,
) -> HandlerResult<()> {
    let msg: WsMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            debug!(err = %e, "ignoring unparseable frame");
            return Ok(());
        }
    };

    if let WsMessage::Req(frame) = msg {
        if frame.method == "connect" {
            handle_connect(ws, state, store, auth, &frame).await?;
        } else if !state.authenticated {
            send(
                ws,
                error_response(&frame.id, ERR_AUTH_REQUIRED, "send connect first"),
            )
            .await?;
        } else {
            handle_rpc(ws, api, &frame).await?;
        }
    }
    // `res` / `event` frames are server→client; ignore if received.
    Ok(())
}

/// Strict SoulSystem `connect` handshake: validate protocol range and token,
/// create a session, mint a device token, reply with `hello-ok`.
async fn handle_connect(
    ws: &mut WebSocket,
    state: &mut ConnState,
    store: &SessionStore,
    auth: &AuthManager,
    frame: &RequestFrame,
) -> HandlerResult<()> {
    let Ok(req) = serde_json::from_value::<ConnectRequest>(frame.params.clone()) else {
        send(
            ws,
            error_response(&frame.id, ERR_INVALID_PARAMS, "invalid connect params"),
        )
        .await?;
        return Ok(());
    };

    if PROTOCOL_VERSION < req.min_protocol || PROTOCOL_VERSION > req.max_protocol {
        send(
            ws,
            error_response(&frame.id, ERR_UNSUPPORTED_PROTOCOL, "protocol mismatch"),
        )
        .await?;
        return Ok(());
    }

    // Token is required whenever a gateway token is configured (operator mode).
    if auth.requires_token() {
        let Some(token) = req.auth.token.as_ref().map(|t| t.expose()) else {
            send(
                ws,
                error_response(&frame.id, ERR_AUTH_REQUIRED, "missing token"),
            )
            .await?;
            return Ok(());
        };
        if auth.validate_token(token).is_none() {
            send(
                ws,
                error_response(&frame.id, ERR_AUTH_FAILED, "invalid token"),
            )
            .await?;
            return Ok(());
        }
    }

    let session = store
        .create(req.client.id.clone(), Some(req.client.platform.clone()))
        .await;
    state.session_id = Some(session.id.clone());
    state.authenticated = true;

    let device = auth.generate_device_token(req.role, req.scopes.clone(), req.client.id.clone());
    let hello = HelloOk {
        typ: "hello-ok".into(),
        protocol: PROTOCOL_VERSION,
        session_id: session.id.clone(),
        device_token: ProtocolSecret::new(device.token.expose()),
        policy: PolicyInfo::default(),
    };
    send(
        ws,
        success_response(&frame.id, serde_json::to_value(hello)?),
    )
    .await?;
    info!(session = %session.id, "client authenticated");
    Ok(())
}

/// Route an authenticated RPC request to the provider registry / metadata.
async fn handle_rpc(ws: &mut WebSocket, api: &ApiState, frame: &RequestFrame) -> HandlerResult<()> {
    let providers = api.registry.list_providers().await;

    match frame.method.as_str() {
        "health" => {
            send_ok(
                ws,
                &frame.id,
                serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}),
            )
            .await?;
        }
        "status" => {
            send_ok(ws, &frame.id, serde_json::json!({"gateway": "soullink-gateway-rs", "version": env!("CARGO_PKG_VERSION"), "providers": providers})).await?;
        }
        "providers.list" => {
            send_ok(ws, &frame.id, serde_json::json!({"providers": providers})).await?;
        }
        "models.list" => {
            let mut models = Vec::new();
            for p in &providers {
                let pname = p["name"].as_str().unwrap_or("unknown");
                if let Some(arr) = p["models"].as_array() {
                    for m in arr {
                        if let Some(mname) = m.as_str() {
                            models.push(serde_json::json!({"id": format!("{pname}/{mname}"), "provider": pname}));
                        }
                    }
                }
            }
            send_ok(ws, &frame.id, serde_json::json!({"models": models})).await?;
        }
        "sessions.list" => {
            send_ok(ws, &frame.id, serde_json::json!({"sessions": []})).await?;
        }
        "gateway.identity.get" => {
            send_ok(ws, &frame.id, serde_json::json!({"name": "soullink-gateway", "version": env!("CARGO_PKG_VERSION")})).await?;
        }

        // Cost-aware routing decision for a query, without executing it.
        "route" => {
            let query = routing_query(&frame.params);
            let caps = routing_caps(&frame.params);
            let configs = api.registry.provider_configs().await;
            match crate::routing::route(&configs, &query, &caps) {
                Some(d) => {
                    send_ok(
                        ws,
                        &frame.id,
                        serde_json::json!({
                            "provider": d.profile.name,
                            "type": d.profile.provider,
                            "difficulty": d.difficulty,
                            "uncertain": d.uncertain,
                            "reason": d.reason,
                        }),
                    )
                    .await?;
                }
                None => {
                    send_error(
                        ws,
                        &frame.id,
                        "NO_ROUTE",
                        "no provider satisfies the requested capabilities",
                    )
                    .await?;
                }
            }
        }

        "chat" => {
            let (model, messages, stream) = parse_chat_params(&frame.params);
            let provider = match select_provider(api, &frame.params, &model).await {
                Ok(p) => p,
                Err(e) => {
                    send_error(ws, &frame.id, "PROVIDER_ERROR", &e.to_string()).await?;
                    return Ok(());
                }
            };

            let req = crate::provider::ChatRequest {
                model,
                messages,
                stream,
                temperature: 0.7,
                max_tokens: 4096,
            };

            if stream {
                match provider.chat_stream(req).await {
                    Ok(mut stream) => {
                        let mut full_content = String::new();
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(delta) => {
                                    full_content.push_str(&delta.content);
                                    send_event(
                                        ws,
                                        "chat.delta",
                                        serde_json::json!({
                                            "type": "delta",
                                            "content": delta.content,
                                            "finish_reason": delta.finish_reason,
                                        }),
                                    )
                                    .await?;
                                    if delta.finish_reason.is_some() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    send_error(ws, &frame.id, "STREAM_ERROR", &e.to_string())
                                        .await?;
                                    return Ok(());
                                }
                            }
                        }
                        send_ok(
                            ws,
                            &frame.id,
                            serde_json::json!({
                                "content": full_content,
                                "role": "assistant",
                                "done": true,
                            }),
                        )
                        .await?;
                    }
                    Err(e) => {
                        send_error(ws, &frame.id, "PROVIDER_ERROR", &e.to_string()).await?;
                    }
                }
            } else {
                match provider.chat(req).await {
                    Ok(resp) => {
                        send_ok(
                            ws,
                            &frame.id,
                            serde_json::json!({
                                "content": resp.message.content,
                                "role": resp.message.role,
                                "model": resp.model,
                                "usage": resp.usage,
                            }),
                        )
                        .await?;
                    }
                    Err(e) => {
                        send_error(ws, &frame.id, "PROVIDER_ERROR", &e.to_string()).await?;
                    }
                }
            }
        }

        "completion" => {
            let prompt = frame
                .params
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let model = frame
                .params
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let provider = match select_provider(api, &frame.params, &model).await {
                Ok(p) => p,
                Err(e) => {
                    send_error(ws, &frame.id, "PROVIDER_ERROR", &e.to_string()).await?;
                    return Ok(());
                }
            };

            let req = crate::provider::CompletionRequest {
                prompt,
                model,
                temperature: 0.7,
                max_tokens: 4096,
            };

            match provider.complete(req).await {
                Ok(resp) => {
                    send_ok(
                        ws,
                        &frame.id,
                        serde_json::json!({
                            "text": resp.text,
                            "model": resp.model,
                            "usage": resp.usage,
                        }),
                    )
                    .await?;
                }
                Err(e) => {
                    send_error(ws, &frame.id, "PROVIDER_ERROR", &e.to_string()).await?;
                }
            }
        }

        _ => {
            send(
                ws,
                error_response(&frame.id, ERR_UNKNOWN_METHOD, &frame.method),
            )
            .await?;
        }
    }

    Ok(())
}

/// Parse chat params from RPC params JSON.
fn parse_chat_params(params: &Value) -> (Option<String>, Vec<crate::provider::ChatMessage>, bool) {
    let model = params
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let stream = params
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let messages = params
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| crate::provider::ChatMessage {
                    role: m
                        .get("role")
                        .and_then(|r| r.as_str())
                        .unwrap_or("user")
                        .to_string(),
                    content: m
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    (model, messages, stream)
}

/// Derive the text used for cost-aware routing: the `prompt` field, else the
/// concatenation of message contents.
fn routing_query(params: &Value) -> String {
    if let Some(p) = params.get("prompt").and_then(|v| v.as_str()) {
        return p.to_string();
    }
    params
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Parse an optional `capabilities` array from the params.
fn routing_caps(params: &Value) -> Vec<avid_model_router::Capability> {
    let raw: Vec<String> = params
        .get("capabilities")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    crate::routing::parse_caps(&raw)
}

/// Resolve a provider for a request. With an explicit `model`, defer to the
/// registry's resolver (provider/model prefix or round-robin). Without one,
/// try cost-aware routing first, falling back to the round-robin resolver.
async fn select_provider(
    api: &ApiState,
    params: &Value,
    model: &Option<String>,
) -> Result<Arc<dyn Provider>, ProviderError> {
    if model.is_none() {
        let query = routing_query(params);
        let caps = routing_caps(params);
        let configs = api.registry.provider_configs().await;
        if let Some(decision) = crate::routing::route(&configs, &query, &caps) {
            if let Some(provider) = api.registry.get(&decision.profile.name).await {
                return Ok(provider);
            }
        }
    }
    api.registry.resolve(model.as_deref()).await
}

async fn send(ws: &mut WebSocket, msg: WsMessage) -> HandlerResult<()> {
    ws.send(Message::Text(serde_json::to_string(&msg)?.into()))
        .await?;
    Ok(())
}

async fn send_ok(ws: &mut WebSocket, id: &str, payload: Value) -> HandlerResult<()> {
    send(ws, success_response(id, payload)).await
}

async fn send_event(ws: &mut WebSocket, name: &str, payload: Value) -> HandlerResult<()> {
    send(ws, event(name, payload)).await
}

async fn send_error(ws: &mut WebSocket, id: &str, code: &str, message: &str) -> HandlerResult<()> {
    send(ws, error_response(id, code, message)).await
}
