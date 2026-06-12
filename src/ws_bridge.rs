//! Bridge WebSocket pour la plateforme unifiee Telegram.
//!
//! Permet a openclaw-gateway (Node.js) de communiquer avec SoulSystem
//! via WebSocket, evitant les conflits de token Telegram (409 Conflict).
//!
//! Architecture:
//!   Clawd (Rust/teloxide) = unique point d'entree Telegram
//!   WS Bridge (port 9020) = relais bidirectionnel vers le gateway Node.js
//!
//! Protocole:
//!   - Connexion: ws://127.0.0.1:9022/
//!   - Auth: token dans le query string `?token=<secret>` (pas de headers WS)
//!   - Publish: {"type":"publish","topic":"...","payload":{}}
//!   - Subscribe: {"type":"subscribe","topic":"telegram.*"}
//!   - Forward: {"topic":"...","payload":{}}
//!
//! Implementation: tokio-tungstenite direct (pas d'axum::extract::ws
//! car axum 0.7 n'a pas le support WS integre).

use crate::bus::{Bus, Message};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message as WsMessage};
use tracing::{debug, error, info, warn};

/// Lance le serveur WebSocket avec le bus partage.
pub async fn run_ws_bridge(config: WsBridgeConfig, bus: Arc<Bus>) {
    let state = WsBridgeState::new(config, bus);
    run_server(state).await;
}

/// Configuration du bridge WebSocket.
#[derive(Clone, Debug)]
pub struct WsBridgeConfig {
    pub listen: String,
    pub shared_secret: Option<String>,
}

impl Default for WsBridgeConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:9022".to_string(),
            shared_secret: None,
        }
    }
}

/// Messages JSON echanges sur le WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Publish {
        topic: String,
        payload: serde_json::Value,
    },
    Subscribe {
        topic: String,
    },
    Unsubscribe {
        topic: String,
    },
}

/// Message forward du bus vers le client WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsForwardMessage {
    pub topic: String,
    pub payload: serde_json::Value,
}

/// Etat partage du bridge.
pub struct WsBridgeState {
    pub config: WsBridgeConfig,
    pub bus: Arc<Bus>,
    /// Sujets souscrits par chaque connexion (peer_id -> topics).
    pub subscriptions: RwLock<HashMap<String, Vec<String>>>,
    /// Compteur de connexions actives (pour /status).
    pub connected_clients: RwLock<usize>,
}

impl WsBridgeState {
    pub fn new(config: WsBridgeConfig, bus: Arc<Bus>) -> Arc<Self> {
        Arc::new(Self {
            config,
            bus,
            subscriptions: RwLock::new(HashMap::new()),
            connected_clients: RwLock::new(0),
        })
    }

    pub async fn client_count(&self) -> usize {
        *self.connected_clients.read().await
    }

    pub async fn bus_subscriber_count(&self) -> usize {
        self.bus.subscriber_count()
    }
}

/// Lance le serveur WebSocket.
pub async fn run_server(state: Arc<WsBridgeState>) {
    let listener = match TcpListener::bind(&state.config.listen).await {
        Ok(l) => l,
        Err(e) => {
            error!(
                "WS bridge: impossible de binder {} — {}",
                state.config.listen, e
            );
            return;
        }
    };

    info!(
        "WS bridge en ecoute sur {} — attente de connexions gateway",
        state.config.listen
    );

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    handle_connection(stream, addr, state).await;
                });
            }
            Err(e) => {
                error!("WS bridge: erreur accept: {}", e);
            }
        }
    }
}

/// Gere une connexion WebSocket entrante.
async fn handle_connection(stream: TcpStream, addr: SocketAddr, state: Arc<WsBridgeState>) {
    // Pour un serveur simple, on accepte tout puis on verifie au premier message
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            warn!("WS bridge: handshake echoue depuis {}: {}", addr, e);
            return;
        }
    };

    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let peer_id = format!("ws-{}", COUNTER.fetch_add(1, Ordering::Relaxed));

    info!(
        "WS bridge: connexion acceptee — peer={} addr={}",
        peer_id, addr
    );

    {
        let mut count = state.connected_clients.write().await;
        *count += 1;
    }
    {
        let mut subs = state.subscriptions.write().await;
        subs.insert(peer_id.clone(), vec!["*".to_string()]);
    }

    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    let mut bus_rx = state.bus.subscribe();
    let mut authenticated = state.config.shared_secret.is_none()
        || state
            .config
            .shared_secret
            .as_ref()
            .map(|s| s.is_empty())
            .unwrap_or(true);

    loop {
        tokio::select! {
            // Reception depuis le bus interne
            Ok(msg) = bus_rx.recv() => {
                if let Some((topic, payload)) = extract_topic_payload(&msg) {
                    if should_forward(&peer_id, &state, &topic).await {
                        let forward = WsForwardMessage { topic, payload };
                        let json = match serde_json::to_string(&forward) {
                            Ok(j) => j,
                            Err(e) => {
                                error!("WS bridge: serialisation: {}", e);
                                continue;
                            }
                        };
                        if ws_tx.send(WsMessage::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Reception depuis le client WebSocket
            Some(msg_result) = ws_rx.next() => {
                match msg_result {
                    Ok(WsMessage::Text(text)) => {
                        debug!("WS bridge: recu depuis {}: {}", peer_id, text);

                        if !authenticated {
                            // Premier message = auth
                            if let Ok(auth) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(token) = auth.get("token").and_then(|v| v.as_str()) {
                                    if let Some(secret) = &state.config.shared_secret {
                                        if token == secret {
                                            authenticated = true;
                                            let ack = serde_json::json!({"type":"auth_ok"});
                                            let _ = ws_tx.send(WsMessage::Text(ack.to_string().into())).await;
                                            continue;
                                        }
                                    }
                                }
                            }
                            warn!("WS bridge: auth echoue — peer={}", peer_id);
                            let err = serde_json::json!({"type":"auth_error"});
                            let _ = ws_tx.send(WsMessage::Text(err.to_string().into())).await;
                            break;
                        }

                        if let Err(e) = handle_client_message(&text, &peer_id, &state
                        ).await {
                            warn!("WS bridge: message invalide — peer={}: {}", peer_id, e);
                        }
                    }
                    Ok(WsMessage::Close(_)) | Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {}
                    Ok(_) => {}
                    Err(e) => {
                        warn!("WS bridge: erreur recv — peer={}: {}", peer_id, e);
                        break;
                    }
                }
            }

            else => break,
        }
    }

    // Nettoyage
    {
        let mut subs = state.subscriptions.write().await;
        subs.remove(&peer_id);
    }
    {
        let mut count = state.connected_clients.write().await;
        *count = count.saturating_sub(1);
    }
    info!("WS bridge: deconnexion de {} — nettoyage effectue", peer_id);
}

/// Traite un message JSON venant du client.
async fn handle_client_message(
    text: &str,
    peer_id: &str,
    state: &Arc<WsBridgeState>,
) -> anyhow::Result<()> {
    let msg: WsClientMessage = serde_json::from_str(text)?;

    match msg {
        WsClientMessage::Publish { topic, payload } => {
            let bus_msg = Message::Custom {
                topic: topic.clone(),
                payload: payload.clone(),
            };
            state.bus.publish(bus_msg);
            debug!("WS bridge: publie — topic={} peer={}", topic, peer_id);
        }
        WsClientMessage::Subscribe { topic } => {
            let mut subs = state.subscriptions.write().await;
            if let Some(topics) = subs.get_mut(peer_id) {
                if !topics.contains(&topic) {
                    topics.push(topic.clone());
                }
            }
            debug!("WS bridge: souscription — peer={} topic={}", peer_id, topic);
        }
        WsClientMessage::Unsubscribe { topic } => {
            let mut subs = state.subscriptions.write().await;
            if let Some(topics) = subs.get_mut(peer_id) {
                topics.retain(|t| t != &topic);
            }
            debug!(
                "WS bridge: desouscription — peer={} topic={}",
                peer_id, topic
            );
        }
    }

    Ok(())
}

/// Verifie si un message doit etre forward vers un peer donne.
async fn should_forward(peer_id: &str, state: &Arc<WsBridgeState>, topic: &str) -> bool {
    let subs = state.subscriptions.read().await;
    if let Some(topics) = subs.get(peer_id) {
        topics.iter().any(|pat| match_topic(pat, topic))
    } else {
        false
    }
}

/// Matching de topic avec wildcard `*`.
fn match_topic(pattern: &str, topic: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return topic.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return topic.ends_with(suffix);
    }
    pattern == topic
}

/// Extrait (topic, payload) d'un Message du bus.
fn extract_topic_payload(msg: &Message) -> Option<(String, serde_json::Value)> {
    match msg {
        Message::Custom { topic, payload } => Some((topic.clone(), payload.clone())),
        _ => None,
    }
}

// Re-export pour utilisation externe
pub use tokio_tungstenite::tungstenite::protocol::Message as WsRawMessage;
