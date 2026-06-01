//! WebSocket connection handler — manages the per-connection lifecycle.
//!
//! Flow: upgrade → receive messages → route → respond → close

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::stream::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use super::protocol::*;
use super::session::SessionStore;
use crate::api::ApiState;
use crate::rpc::{RpcError, RpcRegistry, RpcState};

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
    _shutdown: broadcast::Receiver<()>,
) {
    let mut state = ConnState {
        session_id: None,
        authenticated: false,
    };

    // Incoming message loop
    loop {
        tokio::select! {
            msg = ws.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(ref e) = handle_message(&mut ws, &mut state, &store, &api, &text).await {
                            warn!(err = %e, "message handler error");
                            let err_msg = e.to_string();
                            let _ = ws.send(Message::Text(
                                serde_json::to_string(&WsMessage::Error {
                                    code: "HANDLER_ERROR".into(),
                                    message: err_msg,
                                    id: None,
                                }).unwrap()
                            )).await;
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws.send(Message::Pong(data)).await;
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
                    _ => {} // Binary, Pong — ignore
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal — closing ws connection");
                let _ = ws.send(Message::Text(
                    serde_json::to_string(&WsMessage::Error {
                        code: "SHUTDOWN".into(),
                        message: "server shutting down".into(),
                        id: None,
                    }).unwrap()
                )).await;
                break;
            }
        }
    }

    // Cleanup
    if let Some(ref sid) = state.session_id {
        store.remove(sid).await;
        info!(session = %sid, "session cleaned up on disconnect");
    }
}

/// Route a single text message to the appropriate handler.
async fn handle_message(
    ws: &mut WebSocket,
    state: &mut ConnState,
    store: &SessionStore,
    api: &ApiState,
    text: &str,
) -> HandlerResult<()> {
    let msg: WsMessage = serde_json::from_str(text)?;

    match msg {
        WsMessage::Connect(params) => {
            // Authenticate
            let client_name = params.client.name.clone();
            let session = store.create(params.client.id, client_name).await;
            state.session_id = Some(session.id.clone());
            state.authenticated = true;

            let hello = store.hello(&session);
            let reply = WsMessage::HelloOk(hello);
            let json = serde_json::to_string(&reply)?;
            ws.send(Message::Text(json)).await?;
            info!(session = %session.id, "client authenticated");
        }

        WsMessage::Req(frame) => {
            if !state.authenticated {
                send_error(
                    ws,
                    "NOT_AUTHENTICATED",
                    "send Connect first",
                    Some(&frame.id),
                )
                .await?;
                return Ok(());
            }
            handle_rpc(ws, state, api, &frame).await?;
        }

        WsMessage::Ping => {
            ws.send(Message::Text(serde_json::to_string(&WsMessage::Pong)?))
                .await?;
        }

        WsMessage::Pong => {
            // no-op
        }

        other => {
            debug!(msg = ?other, "unhandled message type");
        }
    }

    Ok(())
}

/// Route an RPC request — dispatches to provider registry for chat/completion,
/// returns metadata for status/providers queries.
async fn handle_rpc(
    ws: &mut WebSocket,
    _state: &ConnState,
    api: &ApiState,
    frame: &RequestFrame,
) -> HandlerResult<()> {
    let providers = api.registry.list_providers().await;

    match frame.method.as_str() {
        // ── Diagnostic / metadata methods ──────────────────────
        "health" => {
            let payload = serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")});
            send_ok(ws, &frame.id, payload).await?;
        }
        "status" => {
            let payload = serde_json::json!({"gateway": "soullink-gateway-rs", "version": env!("CARGO_PKG_VERSION"), "providers": providers});
            send_ok(ws, &frame.id, payload).await?;
        }
        "providers.list" => {
            let payload = serde_json::json!({"providers": providers});
            send_ok(ws, &frame.id, payload).await?;
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
            let payload = serde_json::json!({"name": "soullink-gateway", "version": env!("CARGO_PKG_VERSION")});
            send_ok(ws, &frame.id, payload).await?;
        }

        // ── LLM methods — dispatch to provider ────────────────
        "chat" => {
            let (model, messages, stream) = parse_chat_params(&frame.params);
            let provider = match api.registry.resolve(model.as_deref()).await {
                Ok(p) => p,
                Err(e) => {
                    send_error(ws, "PROVIDER_ERROR", &e.to_string(), Some(&frame.id)).await?;
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
                                    let delta_payload = serde_json::json!({
                                        "type": "delta",
                                        "content": delta.content,
                                        "finish_reason": delta.finish_reason,
                                    });
                                    send_event(ws, "chat.delta", delta_payload).await?;
                                    if delta.finish_reason.is_some() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    send_error(ws, "STREAM_ERROR", &e.to_string(), Some(&frame.id))
                                        .await?;
                                    return Ok(());
                                }
                            }
                        }
                        let payload = serde_json::json!({
                            "content": full_content,
                            "role": "assistant",
                            "done": true,
                        });
                        send_ok(ws, &frame.id, payload).await?;
                    }
                    Err(e) => {
                        send_error(ws, "PROVIDER_ERROR", &e.to_string(), Some(&frame.id)).await?;
                    }
                }
            } else {
                match provider.chat(req).await {
                    Ok(resp) => {
                        let payload = serde_json::json!({
                            "content": resp.message.content,
                            "role": resp.message.role,
                            "model": resp.model,
                            "usage": resp.usage,
                        });
                        send_ok(ws, &frame.id, payload).await?;
                    }
                    Err(e) => {
                        send_error(ws, "PROVIDER_ERROR", &e.to_string(), Some(&frame.id)).await?;
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

            let provider = match api.registry.resolve(model.as_deref()).await {
                Ok(p) => p,
                Err(e) => {
                    send_error(ws, "PROVIDER_ERROR", &e.to_string(), Some(&frame.id)).await?;
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
                    let payload = serde_json::json!({
                        "text": resp.text,
                        "model": resp.model,
                        "usage": resp.usage,
                    });
                    send_ok(ws, &frame.id, payload).await?;
                }
                Err(e) => {
                    send_error(ws, "PROVIDER_ERROR", &e.to_string(), Some(&frame.id)).await?;
                }
            }
        }

        _ => {
            send_error(ws, "METHOD_NOT_FOUND", &frame.method, Some(&frame.id)).await?;
        }
    }

    Ok(())
}

/// Parse chat params from RPC params JSON.
fn parse_chat_params(
    params: &serde_json::Value,
) -> (Option<String>, Vec<crate::provider::ChatMessage>, bool) {
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

/// Send a successful RPC response.
async fn send_ok(ws: &mut WebSocket, id: &str, payload: serde_json::Value) -> HandlerResult<()> {
    let reply = WsMessage::Res(ResponseFrame {
        id: id.to_string(),
        ok: true,
        payload: Some(payload),
        error: None,
    });
    ws.send(Message::Text(serde_json::to_string(&reply)?))
        .await?;
    Ok(())
}

/// Send a server-sent event (for streaming chat deltas).
async fn send_event(
    ws: &mut WebSocket,
    event: &str,
    payload: serde_json::Value,
) -> HandlerResult<()> {
    let msg = WsMessage::Event(EventFrame {
        event: event.to_string(),
        payload: Some(payload),
    });
    ws.send(Message::Text(serde_json::to_string(&msg)?)).await?;
    Ok(())
}

/// Send an error response.
async fn send_error(
    ws: &mut WebSocket,
    code: &str,
    message: &str,
    id: Option<&str>,
) -> HandlerResult<()> {
    let reply = if let Some(req_id) = id {
        WsMessage::Res(ResponseFrame {
            id: req_id.to_string(),
            ok: false,
            payload: None,
            error: Some(ResponseError {
                code: code.to_string(),
                message: message.to_string(),
            }),
        })
    } else {
        WsMessage::Error {
            code: code.to_string(),
            message: message.to_string(),
            id: None,
        }
    };
    let json = serde_json::to_string(&reply)?;
    ws.send(Message::Text(json)).await?;
    Ok(())
}
