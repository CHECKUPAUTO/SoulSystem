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
    /// Whether to serve clients when no shared secret is configured.
    ///
    /// Default [`UnauthenticatedAccess::Deny`]. See [`UnauthenticatedAccess`].
    pub unauthenticated_access: UnauthenticatedAccess,
}

/// What the bridge does when `shared_secret` is unset or empty.
///
/// The bridge relays the internal [`Bus`] — a subscriber sees every message the
/// process publishes, and a publisher can inject onto any topic. Before this was
/// introduced the handshake initialised `authenticated` to `true` whenever no
/// secret was configured, i.e. it failed **open**, and an unset secret is the
/// default. The loopback default bind was the only thing standing between an
/// unconfigured deployment and an unauthenticated bus tap
/// (CRIT-007 / INV-NET-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnauthenticatedAccess {
    /// Refuse every connection while no secret is configured. The listener still
    /// binds, so a misconfiguration is visible rather than silent.
    #[default]
    Deny,
    /// Serve clients with no authentication at all. Only for local development;
    /// the production startup guard rejects this posture.
    Allow,
}

impl Default for WsBridgeConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:9022".to_string(),
            shared_secret: None,
            unauthenticated_access: UnauthenticatedAccess::Deny,
        }
    }
}

impl WsBridgeConfig {
    /// The secret to authenticate against, if one is usable.
    ///
    /// An empty string counts as unset: a blank value in config or environment
    /// must not become a valid credential.
    pub fn effective_secret(&self) -> Option<&str> {
        self.shared_secret
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Whether this configuration authenticates its clients.
    ///
    /// Reported to the production startup guard as the listener's posture, so
    /// the guard's view is derived from the real configuration rather than
    /// hardcoded.
    pub fn is_authenticated(&self) -> bool {
        self.effective_secret().is_some()
    }
}

/// Constant-time byte comparison, so a wrong token cannot be recovered by
/// timing the reply. Mirrors `soul_gateway`'s bearer-token comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
    // Fail closed before doing any work: with no usable secret the only way to
    // serve this connection would be without authentication, which must be an
    // explicit opt-in rather than the default (CRIT-007 / INV-NET-1).
    let secret = state.config.effective_secret().map(str::to_owned);
    if secret.is_none() && state.config.unauthenticated_access == UnauthenticatedAccess::Deny {
        warn!(
            "WS bridge: refusing connection from {} — no shared secret is configured. \
             Set one, or opt in explicitly with UnauthenticatedAccess::Allow for local \
             development.",
            addr
        );
        return;
    }

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
    // Authenticated up-front only on the explicit opt-in path (no secret AND
    // UnauthenticatedAccess::Allow); the deny case already returned above. When
    // a secret exists the client must present it in its first message.
    let mut authenticated = secret.is_none();

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
                                    if let Some(secret) = &secret {
                                        if constant_time_eq(token.as_bytes(), secret.as_bytes()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The bridge relays the internal bus, so an unconfigured deployment must
    /// not serve anyone. Before this it initialised `authenticated = true`
    /// whenever no secret was set — i.e. it failed open, by default.
    #[test]
    fn default_config_denies_unauthenticated_access() {
        let config = WsBridgeConfig::default();
        assert_eq!(
            config.unauthenticated_access,
            UnauthenticatedAccess::Deny,
            "an unset secret must refuse connections, not serve them"
        );
        assert!(
            !config.is_authenticated(),
            "no secret configured means the listener is not authenticated"
        );
    }

    /// A blank value in config or environment is a misconfiguration, not a
    /// credential: it must never authenticate a client.
    #[test]
    fn blank_secret_is_treated_as_unset() {
        for blank in ["", "   ", "\t", "\n"] {
            let config = WsBridgeConfig {
                shared_secret: Some(blank.to_string()),
                ..Default::default()
            };
            assert_eq!(
                config.effective_secret(),
                None,
                "blank secret {blank:?} must not count as configured"
            );
            assert!(!config.is_authenticated());
        }
    }

    #[test]
    fn a_real_secret_authenticates_and_is_trimmed() {
        let config = WsBridgeConfig {
            shared_secret: Some("  s3cret  ".to_string()),
            ..Default::default()
        };
        assert_eq!(config.effective_secret(), Some("s3cret"));
        assert!(config.is_authenticated());
    }

    /// The posture reported to the production startup guard must be derived
    /// from the configuration actually served, so the guard cannot claim
    /// authentication the bridge does not perform.
    #[test]
    fn reported_posture_tracks_the_real_configuration() {
        assert!(!WsBridgeConfig::default().is_authenticated());
        assert!(WsBridgeConfig {
            shared_secret: Some("token".into()),
            ..Default::default()
        }
        .is_authenticated());
        // Opting in to unauthenticated access does NOT make it authenticated:
        // the guard must still see an unauthenticated listener.
        assert!(!WsBridgeConfig {
            shared_secret: None,
            unauthenticated_access: UnauthenticatedAccess::Allow,
            ..Default::default()
        }
        .is_authenticated());
    }

    #[test]
    fn constant_time_eq_matches_only_identical_tokens() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"token", b"tokeN"));
        assert!(!constant_time_eq(b"token", b"token "));
        assert!(!constant_time_eq(b"token", b""));
        assert!(!constant_time_eq(b"", b"token"));
    }
}
