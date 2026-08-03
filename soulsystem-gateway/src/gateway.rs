use crate::auth::AuthManager;
use crate::config::GatewayConfig;
use crate::protocol::{
    ConnectRequest, HelloOk, PolicyInfo, Message, Request, Response,
    PROTOCOL_VERSION, error_response, success_response,
};
use crate::session::{Session, SessionManager};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde_json::json;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct GatewayState {
    pub config: GatewayConfig,
    pub auth: Arc<AuthManager>,
    pub sessions: Arc<SessionManager>,
}

pub struct GatewayServer {
    state: GatewayState,
}

impl GatewayServer {
    pub fn new(config: GatewayConfig) -> Self {
        let auth = Arc::new(AuthManager::new(config.auth_token.clone()));
        let sessions = Arc::new(SessionManager::new());

        Self {
            state: GatewayState {
                config,
                auth,
                sessions,
            },
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let state = self.state.clone();

        state.sessions.clone().start_cleanup_task();

        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/health", get(health_handler))
            .route("/status", get(status_handler))
            .with_state(state);

        let addr: SocketAddr = format!("0.0.0.0:{}", self.state.config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;

        info!("🚀 Gateway listening on ws://{}/ws", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn status_handler(State(state): State<GatewayState>) -> impl IntoResponse {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "sessions": state.sessions.sessions.len(),
        "port": state.config.port,
    }))
}

async fn ws_handler(State(state): State<GatewayState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: GatewayState) {
    let (mut sender, mut receiver) = socket.split();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Value>();

    let send_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Ok(msg) = serde_json::to_string(&event) {
                if sender.send(WsMessage::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut current_session: Option<Arc<Session>> = None;

    while let Some(Ok(msg)) = receiver.next().await {
        if let WsMessage::Text(text) = msg {
            match serde_json::from_str::<Message>(&text) {
                Ok(Message::Request(req)) => {
                    let response = handle_request(&req, &state, &mut current_session, &event_tx).await;
                    if let Ok(val) = serde_json::to_value(&response) {
                        let _ = event_tx.send(val);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(s) = current_session {
        state.sessions.sessions.remove(&s.id);
    }

    drop(event_tx);
    let _ = send_task.await;
}

async fn handle_request(
    req: &Request,
    state: &GatewayState,
    current_session: &mut Option<Arc<Session>>,
    event_tx: &mpsc::UnboundedSender<Value>,
) -> Response {
    match req.method.as_str() {
        "connect" => {
            let Ok(connect_req) = serde_json::from_value::<ConnectRequest>(req.params.clone()) else {
                return error_response(&req.id, "INVALID_PARAMS", "Invalid connect params");
            };

            if PROTOCOL_VERSION < connect_req.min_protocol || PROTOCOL_VERSION > connect_req.max_protocol {
                return error_response(&req.id, "UNSUPPORTED_PROTOCOL", "Protocol mismatch");
            }

            let Some(token) = connect_req.auth.token else {
                return error_response(&req.id, "AUTH_REQUIRED", "Missing token");
            };

            let Some(_token_info) = state.auth.validate_token(&token) else {
                return error_response(&req.id, "AUTH_FAILED", "Invalid token");
            };

            let mut session = Session::new(connect_req.client.id.clone(), connect_req.role.clone());
            session.scopes = connect_req.scopes.clone();
            session.caps = connect_req.caps.clone();
            session.event_tx = Some(event_tx.clone());

            let session = Arc::new(session);
            state.sessions.insert(session.clone());
            *current_session = Some(session.clone());

            let auth_response = state.auth.generate_device_token(connect_req.role, connect_req.scopes, connect_req.client.id);

            let hello = HelloOk {
                typ: "hello-ok".into(),
                protocol: PROTOCOL_VERSION,
                session_id: session.id.clone(),
                device_token: auth_response.token.clone(),
                policy: PolicyInfo {
                    heartbeat_interval_ms: 30000,
                    max_message_size: 65536,
                    idle_timeout_ms: 300000,
                },
            };

            success_response(&req.id, json!(hello))
        }
        _ => error_response(&req.id, "UNKNOWN_METHOD", "Unknown method"),
    }
}
