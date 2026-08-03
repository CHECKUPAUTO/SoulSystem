//! Bridge WebSocket pour la plateforme unifiee Telegram.
//!
//! Permet a soulsystem-gateway de communiquer avec SoulSystem
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
use soul_gateway::limits::ConnectionLimiter;
use soulsystem_common::secrets::SecretString;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::protocol::{Message as WsMessage, WebSocketConfig},
};
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
    /// Redacted in `Debug` so the derive on this struct cannot print it
    /// (INV-SEC-2). Read it with `SecretString::expose`.
    pub shared_secret: Option<SecretString>,
    /// Simultaneously accepted connections. Beyond this, a connection is
    /// refused rather than accepted and parked (INV-NET-5).
    pub max_connections: usize,
    /// Largest accepted WebSocket message, in bytes.
    ///
    /// Without this, tungstenite's default is generous and a single client can
    /// announce a frame large enough to make the process buffer it — before
    /// the shared-secret check, which happens on the first message.
    pub max_message_bytes: usize,
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

/// Simultaneously accepted bridge connections.
///
/// Lower than the gateway's cap: this is an internal component-to-component
/// bridge, not a public API, so a large number of live connections is itself a
/// signal that something is wrong.
pub const DEFAULT_WS_MAX_CONNECTIONS: usize = 64;

/// Largest accepted bridge message, in bytes.
pub const DEFAULT_WS_MAX_MESSAGE_BYTES: usize = 256 * 1024;

impl Default for WsBridgeConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:9022".to_string(),
            shared_secret: None,
            max_connections: DEFAULT_WS_MAX_CONNECTIONS,
            max_message_bytes: DEFAULT_WS_MAX_MESSAGE_BYTES,
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
            .as_ref()
            .map(SecretString::expose)
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

    let limiter = ConnectionLimiter::new(state.config.max_connections);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                // Refused, not parked: waiting for a slot would cost us a
                // socket and a queue entry for the attacker's one socket.
                let Some(permit) = limiter.try_acquire() else {
                    warn!(
                        "WS bridge: refus de {} — plafond de connexions atteint ({}/{})",
                        addr,
                        limiter.in_flight(),
                        limiter.capacity()
                    );
                    drop(stream);
                    continue;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    // Held for the connection's lifetime, so the slot returns
                    // however the connection ends.
                    let _permit = permit;
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
    // Bound the frame before the handshake completes, so an oversized message
    // is rejected by the protocol layer rather than buffered and then checked.
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(state.config.max_message_bytes))
        .max_frame_size(Some(state.config.max_message_bytes));

    let ws_stream = match accept_async_with_config(stream, Some(ws_config)).await {
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
                shared_secret: Some(SecretString::new(blank)),
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
            shared_secret: Some(SecretString::new("  s3cret  ")),
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
            shared_secret: Some(SecretString::new("token")),
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

    // ── Connection and frame bounds (P1-11, INV-NET-5) ───────────────────

    #[test]
    fn the_bridge_bounds_connections_and_messages_by_default() {
        // This listener had no limits of any kind: an unbounded accept loop
        // and tungstenite's generous default frame size, both reachable
        // before the shared-secret check (which happens on the first message).
        let config = WsBridgeConfig::default();
        assert_eq!(config.max_connections, DEFAULT_WS_MAX_CONNECTIONS);
        assert_eq!(config.max_message_bytes, DEFAULT_WS_MAX_MESSAGE_BYTES);
        assert!(
            config.max_connections > 0 && config.max_message_bytes > 0,
            "a zero default would read as an outage rather than a limit"
        );
    }

    #[test]
    fn the_accept_loop_refuses_past_the_cap_rather_than_parking() {
        // Guard on the wiring: the value being present in the config does
        // nothing unless the accept loop consults it, and "refused" rather
        // than "queued" is the whole point — parking is what a slow-loris
        // flood wants.
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ws_bridge.rs"))
                .expect("own source is readable");
        let run_server = source
            .split("pub async fn run_server(")
            .nth(1)
            .expect("run_server is present");
        assert!(
            run_server.contains("ConnectionLimiter::new(state.config.max_connections)"),
            "the accept loop must build its limiter from the configured cap"
        );
        assert!(
            run_server.contains("try_acquire()"),
            "try_acquire refuses immediately; acquire() would park the \
             connection, which is the failure mode this closes"
        );
    }

    #[test]
    fn the_handshake_bounds_the_frame_before_completing() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ws_bridge.rs"))
                .expect("own source is readable");
        assert!(
            source.contains("accept_async_with_config"),
            "the bare accept_async takes tungstenite's default frame size"
        );
        assert!(
            source.contains("max_message_size(Some(state.config.max_message_bytes))")
                && source.contains("max_frame_size(Some(state.config.max_message_bytes))"),
            "both bounds must come from the configured value — max_message_size \
             alone still lets a single oversized frame be buffered"
        );
    }
}
