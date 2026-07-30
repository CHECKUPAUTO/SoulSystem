//! # soul_gateway — Surface de contrôle HTTP/WebSocket
//!
//! Inspiré du gateway openclaw (style `/v1/chat/completions`,
//! `/tools/invoke`, sessions, hooks). Expose l'entité autonome via :
//!
//! - `POST /v1/ask`            — question au LLM
//! - `POST /v1/plan`           — crée un plan pour un objectif
//! - `POST /v1/run`            — exécute une commande via le sandbox
//! - `POST /v1/cycle`          — un cycle cognitif complet
//! - `GET  /v1/status`         — état de l'entité
//! - `GET  /v1/goals`          — liste des objectifs
//! - `WS   /v1/stream`         — flux d'événements temps réel
//! - `GET  /health`            — healthcheck
//!
//! Le routeur est construit autour d'un trait `EntityHandle` que l'entité
//! implémente, ce qui permet de tester le gateway sans dépendre du runtime
//! complet.

pub mod limits;
pub mod providers;
pub mod scope;
pub mod webhook_auth;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

/// Erreur côté API.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("entity error: {0}")]
    Entity(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Événement émis par l'entité, broadcast aux clients WS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EntityEvent {
    /// Un nouveau but a été créé.
    GoalCreated {
        id: String,
        description: String,
        ts: DateTime<Utc>,
    },
    /// Un plan a été généré.
    PlanCreated {
        id: String,
        goal_id: String,
        n_steps: usize,
        ts: DateTime<Utc>,
    },
    /// Une étape a été exécutée.
    StepExecuted {
        command: String,
        success: bool,
        /// `true` when the step was **not actually run** — the outcome is
        /// synthesised, not observed.
        ///
        /// `success` alone cannot express this. A simulated step is not a
        /// failure, so reporting `success: false` would be equally untrue;
        /// without a separate field the only options were to lie in one
        /// direction or the other. A consumer that ignores this field is no
        /// worse off than before, and one that cares can finally tell.
        ///
        /// `#[serde(default)]` keeps older payloads decodable: absent means
        /// `false`, which is the correct reading for events emitted before
        /// this distinction existed by any code that really did execute.
        #[serde(default)]
        simulated: bool,
        ms: u64,
        ts: DateTime<Utc>,
    },
    /// Une observation a été ajoutée.
    Observation { text: String, ts: DateTime<Utc> },
    /// Une décision a été prise.
    Decision {
        action: String,
        confidence: f32,
        ts: DateTime<Utc>,
    },
    /// Un cycle a démarré.
    CycleStarted { id: String, ts: DateTime<Utc> },
    /// Un cycle s'est terminé.
    CycleFinished {
        id: String,
        success: bool,
        ts: DateTime<Utc>,
    },
    /// Erreur système.
    Error { message: String, ts: DateTime<Utc> },
    /// Ping périodique pour keepalive.
    Heartbeat { ts: DateTime<Utc> },
}

/// Trait que toute entité autonome doit implémenter pour être pilotable.
#[async_trait]
pub trait EntityHandle: Send + Sync {
    async fn ask(&self, prompt: &str) -> Result<String, String>;
    async fn status(&self) -> serde_json::Value;
    async fn create_goal(&self, description: &str) -> Result<String, String>;
    async fn plan(&self, goal_id: &str) -> Result<Vec<String>, String>;
    async fn execute_plan(&self, goal_id: &str) -> Result<String, String>;
    async fn execute_shell(&self, cmd: &str) -> Result<String, String>;
    async fn run_cycle(&self) -> Result<serde_json::Value, String>;
    async fn list_goals(&self) -> Vec<serde_json::Value>;
    async fn replay_events(&self, n: usize) -> Vec<EntityEvent>;
    async fn alert(&self, level: &str, message: &str) -> Result<(), String>;
    async fn spawn_subagent(&self, _description: &str) -> Result<String, String> {
        Err("subagents are not supported by this entity".into())
    }
    async fn list_subagents(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }
}

/// Hub d'événements : tous les clients WS sont notifiés.
#[derive(Clone)]
pub struct EventHub {
    inner: Arc<Mutex<VecDeque<EntityEvent>>>,
    capacity: usize,
}

impl EventHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub fn publish(&self, event: EntityEvent) {
        let mut q = self.inner.lock();
        q.push_back(event);
        while q.len() > self.capacity {
            q.pop_front();
        }
    }

    pub fn recent(&self, n: usize) -> Vec<EntityEvent> {
        let q = self.inner.lock();
        let start = q.len().saturating_sub(n);
        q.iter().skip(start).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

/// Bearer-token authenticator for the gateway's operator routes.
///
/// Fails closed: with no token configured, every request is rejected — there
/// is no implicit "open" state (CRIT-007 / INV-NET-1). The token is compared
/// in constant time so response latency cannot be used to recover it byte by
/// byte, and it is never included in `Debug` output or logs.
#[derive(Clone)]
pub struct GatewayAuth {
    credentials: Arc<[GatewayCredential]>,
}

#[derive(Clone)]
struct GatewayCredential {
    principal: Arc<str>,
    token: Arc<str>,
    scopes: scope::ScopeSet,
}

/// Authenticated gateway identity plus the scopes it holds.
///
/// Carried in request extensions so a handler can report *who* acted, and so
/// [`require_scope`] can authorize without re-reading the header.
#[derive(Clone, Debug)]
pub struct GatewayPrincipal(pub Arc<str>, pub scope::ScopeSet);

impl GatewayPrincipal {
    pub fn name(&self) -> &str {
        &self.0
    }

    /// The scopes this principal holds.
    pub fn scopes(&self) -> &scope::ScopeSet {
        &self.1
    }
}

impl std::fmt::Debug for GatewayAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayAuth")
            .field("configured", &self.is_configured())
            .field("principal_count", &self.credentials.len())
            .finish()
    }
}

impl GatewayAuth {
    /// Read gateway credentials from the environment.
    ///
    /// `SOULSYSTEM_GATEWAY_TOKENS` accepts a comma-separated list of
    /// `principal=token` or `principal:scope1+scope2=token` entries.
    /// `SOULSYSTEM_GATEWAY_TOKEN` remains supported as the single-user,
    /// backwards-compatible form.
    ///
    /// An entry with no scope list is granted every scope — scopes are
    /// **opt-in**, so upgrading does not start returning 403 to automation that
    /// worked yesterday. See [`scope`] for why that default was chosen and what
    /// it does not buy.
    pub fn from_env() -> Self {
        let mut credentials = Vec::new();
        if let Ok(entries) = std::env::var("SOULSYSTEM_GATEWAY_TOKENS") {
            for entry in entries.split(',') {
                if let Some((principal, token)) = entry.split_once('=') {
                    let principal = principal.trim();
                    let token = token.trim();
                    if !principal.is_empty() && !token.is_empty() {
                        let (name, scopes, unknown) = scope::split_principal_and_scopes(principal);
                        if !unknown.is_empty() {
                            tracing::warn!(
                                principal = %name,
                                unknown = %unknown.join(","),
                                "ignoring unrecognised scope name(s); the \
                                 credential holds only the scopes that parsed"
                            );
                        }
                        credentials.push(GatewayCredential {
                            principal: Arc::from(name.as_str()),
                            token: Arc::from(token),
                            scopes,
                        });
                    }
                }
            }
        }
        if let Ok(token) = std::env::var("SOULSYSTEM_GATEWAY_TOKEN") {
            if !token.is_empty() {
                credentials.push(GatewayCredential {
                    principal: Arc::from("legacy"),
                    token: Arc::from(token),
                    scopes: scope::ScopeSet::all(),
                });
            }
        }
        Self {
            credentials: credentials.into(),
        }
    }

    /// Build an authenticator with an explicit token (tests, embedders).
    pub fn configured(token: impl Into<Arc<str>>) -> Self {
        Self {
            credentials: vec![GatewayCredential {
                principal: Arc::from("operator"),
                token: token.into(),
                scopes: scope::ScopeSet::all(),
            }]
            .into(),
        }
    }

    /// Build an authenticator for one principal with an explicit scope set
    /// (tests, embedders that manage their own credential store).
    pub fn configured_scoped(
        principal: impl Into<Arc<str>>,
        token: impl Into<Arc<str>>,
        scopes: scope::ScopeSet,
    ) -> Self {
        Self {
            credentials: vec![GatewayCredential {
                principal: principal.into(),
                token: token.into(),
                scopes,
            }]
            .into(),
        }
    }

    /// Build an authenticator for multiple named operators.
    pub fn configured_users<I, N, T>(credentials: I) -> Self
    where
        I: IntoIterator<Item = (N, T)>,
        N: Into<Arc<str>>,
        T: Into<Arc<str>>,
    {
        Self {
            credentials: credentials
                .into_iter()
                .map(|(principal, token)| GatewayCredential {
                    principal: principal.into(),
                    token: token.into(),
                    scopes: scope::ScopeSet::all(),
                })
                .collect::<Vec<_>>()
                .into(),
        }
    }

    /// An authenticator with no token configured — every request rejected.
    pub fn unconfigured() -> Self {
        Self {
            credentials: Arc::from([]),
        }
    }

    /// Whether a token is configured at all.
    pub fn is_configured(&self) -> bool {
        !self.credentials.is_empty()
    }

    pub fn principal_count(&self) -> usize {
        self.credentials.len()
    }

    /// Check a bearer token extracted from an `Authorization` header. Returns
    /// `false` whenever no token is configured, no header was provided, or
    /// the provided value does not match — the same outcome for every
    /// rejection reason (no oracle for which case failed).
    fn verify(&self, provided: Option<&str>) -> bool {
        self.authenticate(provided).is_some()
    }

    fn authenticate(&self, provided: Option<&str>) -> Option<GatewayPrincipal> {
        let given = provided?;
        let mut matched = None;
        for credential in self.credentials.iter() {
            if constant_time_eq(credential.token.as_bytes(), given.as_bytes()) {
                matched = Some(GatewayPrincipal(
                    credential.principal.clone(),
                    credential.scopes.clone(),
                ));
            }
        }
        matched
    }
}

/// Re-exported from `soul-auth` so the workspace has one constant-time
/// comparison rather than one per service (MED-013-B).
///
/// This was the original implementation; `soul-auth` now owns it because
/// `soullink-orchestrator` needed the same primitive, and a second copy is how
/// one of them quietly stops being constant-time. The tests below still
/// exercise it from here, which is what proves the move changed nothing.
use soul_auth::constant_time_eq;

/// `axum::middleware::from_fn_with_state` handler enforcing [`GatewayAuth`] on
/// the routes it is layered over. Rejects with `401` when the bearer token is
/// missing or does not match; never with a message that reveals which.
async fn require_auth(
    State(state): State<GatewayState>,
    mut req: Request,
    next: Next,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if let Some(principal) = state.auth.authenticate(provided) {
        req.extensions_mut().insert(principal);
        Ok(next.run(req).await)
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".into(),
            }),
        ))
    }
}

/// Reject an authenticated request whose principal lacks `required`.
///
/// Applied as a `route_layer` on a group of routes rather than checked inside
/// each handler, so the requirement is *declared* alongside the route. A new
/// route added to a scoped group inherits its scope; a new route added outside
/// every group has no scope requirement and is caught by
/// `every_operator_route_declares_a_scope`, which enumerates the router's
/// routes rather than trusting anyone to remember.
///
/// Runs strictly after [`require_auth`], which is what puts the
/// [`GatewayPrincipal`] in the extensions. A missing principal therefore means
/// the layers were composed wrong, not that the caller is anonymous — it is
/// treated as a 500 rather than a 403 so the bug is visible instead of looking
/// like a policy decision.
async fn require_scope(
    required: scope::Scope,
    req: Request,
    next: Next,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    let Some(principal) = req.extensions().get::<GatewayPrincipal>().cloned() else {
        tracing::error!(
            required = %required,
            "scope layer ran without an authenticated principal — the \
             authentication layer must be applied outside this one"
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "authorization misconfigured".into(),
            }),
        ));
    };

    if principal.scopes().allows(required) {
        return Ok(next.run(req).await);
    }

    // 403, not 401: the caller authenticated fine, they just cannot do this.
    // Returning 401 would invite a client to retry with fresh credentials that
    // would fail identically.
    tracing::warn!(
        principal = principal.name(),
        held = %principal.scopes(),
        required = %required,
        "rejected: principal lacks the required scope"
    );
    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: format!("this credential lacks the '{required}' scope"),
        }),
    ))
}

/// Charge one request against the caller's budget, refusing when it is spent.
///
/// Applied *inside* the authenticated group so the key is the principal, not
/// the source address: a token must not be able to escape its limit by
/// reconnecting from a new port. The cost is that an unauthenticated flood is
/// bounded by the connection cap rather than by this limiter, which is the
/// right split — rejecting an unauthenticated request is already cheap.
async fn require_rate_budget(
    State(state): State<GatewayState>,
    req: Request,
    next: Next,
) -> Result<axum::response::Response, (StatusCode, HeaderMap, Json<ErrorResponse>)> {
    let principal = req
        .extensions()
        .get::<GatewayPrincipal>()
        .map(|p| p.name().to_owned());
    let key = limits::rate_key(principal.as_deref(), None);

    match state.rate_limiter.check(&key) {
        limits::RateDecision::Allowed => Ok(next.run(req).await),
        limits::RateDecision::Throttled { retry_after_secs } => {
            tracing::warn!(
                key = %key,
                retry_after_secs,
                "rejected: client exceeded its request budget"
            );
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                headers.insert(header::RETRY_AFTER, value);
            }
            Err((
                StatusCode::TOO_MANY_REQUESTS,
                headers,
                Json(ErrorResponse {
                    error: "request budget exceeded; retry after the interval \
                            in the Retry-After header"
                        .into(),
                }),
            ))
        }
    }
}

/// État partagé entre les handlers.
#[derive(Clone)]
pub struct GatewayState {
    pub entity: Arc<dyn EntityHandle>,
    pub events: EventHub,
    pub auth: GatewayAuth,
    /// Signatures already accepted, so a captured webhook cannot be replayed
    /// while its timestamp is still fresh (HIGH-007 / INV-NET-3).
    pub webhook_replay: Arc<webhook_auth::ReplayCache>,
    /// Body / concurrency / WebSocket-message bounds (INV-NET-5).
    ///
    /// Carried on the state rather than captured by [`router`] alone because
    /// [`handle_ws`] needs the WebSocket bound at upgrade time.
    pub limits: limits::GatewayLimits,
    /// Per-client request budget (INV-NET-5).
    ///
    /// Shared across clones so every worker charges the same buckets — a
    /// per-clone limiter would multiply the effective rate by the number of
    /// clones, which is the classic way this control silently does nothing.
    pub rate_limiter: limits::RateLimiter,
}

impl GatewayState {
    /// Authentication is read from `SOULSYSTEM_GATEWAY_TOKEN` (see
    /// [`GatewayAuth::from_env`]). Use [`GatewayState::with_auth`] to inject
    /// an explicit authenticator instead (tests, embedders).
    pub fn new(entity: Arc<dyn EntityHandle>) -> Self {
        Self {
            entity,
            events: EventHub::new(500),
            auth: GatewayAuth::from_env(),
            webhook_replay: Arc::new(webhook_auth::ReplayCache::new()),
            limits: limits::GatewayLimits::from_env(),
            rate_limiter: {
                let limits = limits::GatewayLimits::from_env();
                limits::RateLimiter::new(limits.rate_per_second, limits.rate_burst)
            },
        }
    }

    /// Build state with an explicit authenticator.
    pub fn with_auth(entity: Arc<dyn EntityHandle>, auth: GatewayAuth) -> Self {
        Self {
            entity,
            events: EventHub::new(500),
            auth,
            webhook_replay: Arc::new(webhook_auth::ReplayCache::new()),
            limits: limits::GatewayLimits::default(),
            rate_limiter: limits::RateLimiter::new(
                limits::DEFAULT_RATE_PER_SECOND,
                limits::DEFAULT_RATE_BURST,
            ),
        }
    }

    /// Override the request limits (tests, embedders).
    ///
    /// Rebuilds the rate limiter, so a test that lowers the rate actually gets
    /// the lower rate rather than the default one built at construction.
    pub fn with_limits(mut self, limits: limits::GatewayLimits) -> Self {
        self.rate_limiter = limits::RateLimiter::new(limits.rate_per_second, limits.rate_burst);
        self.limits = limits;
        self
    }
}

// ── DTOs ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub response: String,
}

#[derive(Debug, Deserialize)]
pub struct GoalRequest {
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct SubAgentRequest {
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct GoalResponse {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct PlanResponse {
    pub goal_id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub result: String,
}

#[derive(Debug, Deserialize)]
pub struct ShellRequest {
    pub command: String,
}

#[derive(Debug, Serialize)]
pub struct CycleResponse {
    pub cycle: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ── Handlers ────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "ts": Utc::now(),
        "service": "soul_gateway",
    }))
}

async fn handle_ask(
    State(st): State<GatewayState>,
    Json(req): Json<AskRequest>,
) -> Result<Json<AskResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "prompt vide".into(),
            }),
        ));
    }
    match st.entity.ask(&req.prompt).await {
        Ok(resp) => Ok(Json(AskResponse { response: resp })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn handle_create_goal(
    State(st): State<GatewayState>,
    Json(req): Json<GoalRequest>,
) -> Result<Json<GoalResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.description.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "description vide".into(),
            }),
        ));
    }
    let desc = req.description.clone();
    match st.entity.create_goal(&desc).await {
        Ok(id) => {
            st.events.publish(EntityEvent::GoalCreated {
                id: id.clone(),
                description: desc,
                ts: Utc::now(),
            });
            Ok(Json(GoalResponse {
                id,
                description: req.description,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn handle_plan(
    State(st): State<GatewayState>,
    axum::extract::Path(goal_id): axum::extract::Path<String>,
) -> Result<Json<PlanResponse>, (StatusCode, Json<ErrorResponse>)> {
    match st.entity.plan(&goal_id).await {
        Ok(steps) => {
            st.events.publish(EntityEvent::PlanCreated {
                id: Uuid::new_v4().to_string(),
                goal_id: goal_id.clone(),
                n_steps: steps.len(),
                ts: Utc::now(),
            });
            Ok(Json(PlanResponse { goal_id, steps }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn handle_execute_plan(
    State(st): State<GatewayState>,
    axum::extract::Path(goal_id): axum::extract::Path<String>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    match st.entity.execute_plan(&goal_id).await {
        Ok(result) => Ok(Json(ExecuteResponse { result })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn handle_shell(
    State(st): State<GatewayState>,
    Json(req): Json<ShellRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.command.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "commande vide".into(),
            }),
        ));
    }
    match st.entity.execute_shell(&req.command).await {
        Ok(result) => Ok(Json(ExecuteResponse { result })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

async fn handle_cycle(
    State(st): State<GatewayState>,
) -> Result<Json<CycleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cycle_id = Uuid::new_v4().to_string();
    st.events.publish(EntityEvent::CycleStarted {
        id: cycle_id.clone(),
        ts: Utc::now(),
    });
    match st.entity.run_cycle().await {
        Ok(cycle) => {
            st.events.publish(EntityEvent::CycleFinished {
                id: cycle_id,
                success: true,
                ts: Utc::now(),
            });
            Ok(Json(CycleResponse { cycle }))
        }
        Err(e) => {
            st.events.publish(EntityEvent::Error {
                message: e.clone(),
                ts: Utc::now(),
            });
            st.events.publish(EntityEvent::CycleFinished {
                id: cycle_id,
                success: false,
                ts: Utc::now(),
            });
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            ))
        }
    }
}

async fn handle_status(State(st): State<GatewayState>) -> impl IntoResponse {
    let s = st.entity.status().await;
    Json(s)
}

async fn handle_list_goals(State(st): State<GatewayState>) -> impl IntoResponse {
    Json(st.entity.list_goals().await)
}

async fn handle_spawn_subagent(
    State(st): State<GatewayState>,
    Json(req): Json<SubAgentRequest>,
) -> impl IntoResponse {
    if req.description.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "description cannot be empty"})),
        );
    }
    match st.entity.spawn_subagent(&req.description).await {
        Ok(id) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"id": id, "status": "pending"})),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error})),
        ),
    }
}

async fn handle_list_subagents(State(st): State<GatewayState>) -> impl IntoResponse {
    Json(st.entity.list_subagents().await)
}

async fn handle_recent_events(State(st): State<GatewayState>) -> impl IntoResponse {
    Json(st.events.recent(50))
}

/// Upgrade to a WebSocket, bounding what a single message may make us buffer.
///
/// `/v1/stream` is bearer-authenticated, but an authenticated client must
/// still not be able to announce an unbounded frame and make the server
/// allocate for it (INV-NET-5).
async fn handle_ws(State(st): State<GatewayState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    let max_message = st.limits.max_ws_message_bytes;
    ws.max_message_size(max_message)
        .max_frame_size(max_message)
        .on_upgrade(move |socket| ws_loop(socket, st))
}

/// Read a header as UTF-8, or `MalformedHeader`.
fn header_str<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Result<&'a str, webhook_auth::WebhookRejection> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or(webhook_auth::WebhookRejection::MalformedHeader)
}

/// One opaque response for every rejection reason.
///
/// The precise cause is logged server-side but never returned: telling a caller
/// "stale timestamp" rather than "bad signature" would let them probe the
/// difference and learn our clock or confirm a guessed secret's shape.
fn webhook_rejected(
    provider: &str,
    reason: webhook_auth::WebhookRejection,
) -> (StatusCode, Json<ErrorResponse>) {
    tracing::warn!(
        provider,
        reason = reason.as_str(),
        "rejected webhook: signature verification failed"
    );
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: format!("{provider} webhook signature verification failed"),
        }),
    )
}

/// Reject a signature that has already been accepted.
fn check_replay(
    st: &GatewayState,
    provider: &str,
    signature: &[u8],
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if st
        .webhook_replay
        .check_and_insert(signature, std::time::SystemTime::now())
    {
        Ok(())
    } else {
        Err(webhook_rejected(
            provider,
            webhook_auth::WebhookRejection::Replayed,
        ))
    }
}

/// Deserialize a body that has already been cryptographically verified.
fn parse_verified_body<T: serde::de::DeserializeOwned>(
    provider: &str,
    body: &[u8],
) -> Result<T, (StatusCode, Json<ErrorResponse>)> {
    serde_json::from_slice(body).map_err(|e| {
        tracing::warn!(provider, error = %e, "verified webhook body failed to parse");
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("{provider} webhook body is not valid JSON"),
            }),
        )
    })
}

/// Discord signs `timestamp ‖ body` with **Ed25519** (not HMAC);
/// `DISCORD_PUBLIC_KEY` is a hex public key.
async fn handle_discord_webhook(
    State(st): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let Ok(public_key) = std::env::var("DISCORD_PUBLIC_KEY") else {
        return Err(webhook_not_configured("discord"));
    };
    if public_key.is_empty() {
        return Err(webhook_not_configured("discord"));
    }

    let signature = (|| {
        let ts = header_str(&headers, "x-signature-timestamp")?;
        let sig = header_str(&headers, "x-signature-ed25519")?;
        webhook_auth::verify_discord(&public_key, ts, sig, &body, std::time::SystemTime::now())
    })()
    .map_err(|reason| webhook_rejected("discord", reason))?;

    check_replay(&st, "discord", &signature)?;

    let payload: providers::discord::DiscordWebhookBody = parse_verified_body("discord", &body)?;
    let response =
        providers::discord::handle_webhook(&st.entity, &Some(st.events.clone()), payload).await;
    Ok(Json(response))
}

/// Slack signs the base string `v0:{timestamp}:{body}` with HMAC-SHA256.
async fn handle_slack_webhook(
    State(st): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let Ok(secret) = std::env::var("SLACK_SIGNING_SECRET") else {
        return Err(webhook_not_configured("slack"));
    };
    if secret.is_empty() {
        return Err(webhook_not_configured("slack"));
    }

    let signature = (|| {
        let ts = header_str(&headers, "x-slack-request-timestamp")?;
        let sig = header_str(&headers, "x-slack-signature")?;
        webhook_auth::verify_slack(&secret, ts, sig, &body, std::time::SystemTime::now())
    })()
    .map_err(|reason| webhook_rejected("slack", reason))?;

    check_replay(&st, "slack", &signature)?;

    let payload: providers::slack::SlackWebhookBody = parse_verified_body("slack", &body)?;
    let response =
        providers::slack::handle_webhook(&st.entity, &Some(st.events.clone()), payload).await;
    Ok(Json(response))
}

/// Meta signs the raw body with HMAC-SHA256 and sends no timestamp, so replay
/// protection is the only freshness control available for this provider.
async fn handle_whatsapp_webhook(
    State(st): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let Ok(secret) = std::env::var("WHATSAPP_WEBHOOK_SECRET") else {
        return Err(webhook_not_configured("whatsapp"));
    };
    if secret.is_empty() {
        return Err(webhook_not_configured("whatsapp"));
    }

    let signature = (|| {
        let sig = header_str(&headers, "x-hub-signature-256")?;
        webhook_auth::verify_meta(&secret, sig, &body)
    })()
    .map_err(|reason| webhook_rejected("whatsapp", reason))?;

    check_replay(&st, "whatsapp", &signature)?;

    let payload: providers::whatsapp::WhatsAppWebhookBody = parse_verified_body("whatsapp", &body)?;
    let response =
        providers::whatsapp::handle_webhook(&st.entity, &Some(st.events.clone()), payload).await;
    Ok(Json(response))
}

fn webhook_not_configured(provider: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: format!("{provider} webhook verification secret is not configured"),
        }),
    )
}

async fn ws_loop(mut socket: WebSocket, state: GatewayState) {
    let mut last_idx = 0usize;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
        interval.tick().await;
        let recent = state.events.recent(100);
        if recent.len() > last_idx {
            for ev in &recent[last_idx..] {
                let payload = serde_json::to_string(ev).unwrap_or_default();
                if socket.send(Message::Text(payload)).await.is_err() {
                    return;
                }
            }
            last_idx = recent.len();
        } else {
            // keepalive
            let ping = EntityEvent::Heartbeat { ts: Utc::now() };
            let payload = serde_json::to_string(&ping).unwrap_or_default();
            if socket.send(Message::Text(payload)).await.is_err() {
                return;
            }
        }
    }
}

/// Construit le routeur axum.
///
/// Every operator route (`/v1/*`) requires a valid `Authorization: Bearer
/// <token>` header, enforced by [`require_auth`] — including status/goal/event
/// disclosure routes, not only state-changing ones (CRIT-007 / INV-NET-1).
/// `/health` is the sole exception (liveness probe; carries no state).
/// `/providers/*/webhook` routes are not bearer-authenticated (the caller is
/// an external provider, not an operator); each one instead verifies the
/// provider's cryptographic signature over the raw body, checks timestamp
/// freshness and rejects replays (HIGH-007 / INV-NET-3, see
/// [`webhook_auth`]).
///
/// Cross-origin access is an explicit allowlist read from the environment and
/// defaults to *no* cross-origin access (INV-NET-4), and every route carries a
/// body-size and concurrency bound (INV-NET-5). Both come from
/// [`limits::CorsPolicy::from_env`] / [`GatewayState::limits`]; use
/// [`router_with_cors`] to inject a policy explicitly.
pub fn router(state: GatewayState) -> Router {
    router_with_cors(
        state,
        limits::CorsPolicy::from_env(limits::CORS_ALLOWLIST_VAR),
    )
}

/// [`router`], with the cross-origin policy supplied rather than read from the
/// environment.
pub fn router_with_cors(state: GatewayState, cors: limits::CorsPolicy) -> Router {
    let limits = state.limits;
    // Grouped by the scope each route requires, so the requirement is declared
    // next to the route rather than checked ad hoc inside a handler.
    let read_routes = Router::new()
        .route("/v1/status", get(handle_status))
        .route("/v1/goals", get(handle_list_goals))
        .route("/v1/events", get(handle_recent_events))
        .route("/v1/stream", get(handle_ws))
        .route("/v1/subagents", get(handle_list_subagents))
        .route_layer(middleware::from_fn(move |req, next| {
            require_scope(scope::Scope::Read, req, next)
        }));

    let write_routes = Router::new()
        .route("/v1/goal", post(handle_create_goal))
        .route("/v1/plan/:goal_id", post(handle_plan))
        .route_layer(middleware::from_fn(move |req, next| {
            require_scope(scope::Scope::Write, req, next)
        }));

    // `/v1/ask` is Exec, not Write: it spends provider budget and can drive
    // tool use, so it is closer to "make the host do something" than to
    // "record an intention".
    let exec_routes = Router::new()
        .route("/v1/ask", post(handle_ask))
        .route("/v1/execute/:goal_id", post(handle_execute_plan))
        .route("/v1/run", post(handle_shell))
        .route("/v1/cycle", post(handle_cycle))
        .route("/v1/subagents", post(handle_spawn_subagent))
        .route_layer(middleware::from_fn(move |req, next| {
            require_scope(scope::Scope::Exec, req, next)
        }));

    // Authentication wraps all three groups, so `require_scope` always finds a
    // principal in the extensions.
    let operator_routes = read_routes
        .merge(write_routes)
        .merge(exec_routes)
        // Ordering matters: authentication outermost, then the per-principal
        // budget, then the per-route scope. Charging the budget before
        // authenticating would let an anonymous flood exhaust a real
        // principal's allowance; charging it after the scope check would let a
        // caller spend budget on routes they cannot reach anyway.
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_rate_budget,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let webhook_routes = Router::new()
        .route("/providers/discord/webhook", post(handle_discord_webhook))
        .route("/providers/slack/webhook", post(handle_slack_webhook))
        .route("/providers/whatsapp/webhook", post(handle_whatsapp_webhook));

    Router::new()
        .route("/health", get(health))
        .merge(operator_routes)
        .merge(webhook_routes)
        // Innermost: the body bound has to be visible to the extractors that
        // consult it (`Bytes`, `Json`), so it is applied closest to the routes.
        .layer(DefaultBodyLimit::max(limits.max_body_bytes))
        // Bounds the requests being *served* at once. `Global` and not
        // `ConcurrencyLimitLayer`: the latter builds a fresh semaphore on every
        // `Layer::layer` call, so with `Router::layer` each route would get its
        // own budget of N and the real ceiling would be N × routes. The global
        // layer shares one semaphore across every route and every clone of the
        // router. Excess requests wait on it rather than erroring, so a burst
        // degrades latency instead of failing; note this bounds work in flight,
        // not the number of queued connections.
        .layer(tower::limit::GlobalConcurrencyLimitLayer::new(
            limits.max_concurrent_requests,
        ))
        // Outermost, so error responses (401, 413) carry the same origin
        // decision as successful ones.
        .layer(cors.read_write_layer())
        .with_state(state)
}

/// Lance le serveur sur l'adresse donnée et démarre les channel providers
/// configurés. Bloque jusqu'à arrêt.
pub async fn serve(state: GatewayState, addr: SocketAddr) -> std::io::Result<()> {
    serve_with_tls(state, addr, None).await
}

/// Start the gateway with optional native Rustls TLS.
pub async fn serve_with_tls(
    state: GatewayState,
    addr: SocketAddr,
    tls: Option<TlsConfig>,
) -> std::io::Result<()> {
    // Start messaging providers (Telegram, etc.) if configured.
    providers::start_all(providers::ChannelConfig {
        entity: state.entity.clone(),
        event_bus: Some(state.events.clone()),
    })
    .await;

    let connection_limit = limits::ConnectionLimiter::new(state.limits.max_connections);
    let app = router(state);
    if let Some(tls) = tls {
        tracing::info!("soul_gateway listening with TLS on {}", addr);
        axum_server::bind_rustls(addr, tls.rustls)
            .serve(app.into_make_service())
            .await
    } else {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(
            max_connections = connection_limit.capacity(),
            "soul_gateway listening on {}",
            addr
        );
        serve_bounded(listener, app, connection_limit).await
    }
}

/// Serve with a hard cap on simultaneously accepted connections.
///
/// `axum::serve` accepts every connection the OS hands it, so a slow-loris
/// style flood is bounded only by the file-descriptor limit — the existing
/// concurrency semaphore never sees a connection that never completes a
/// request. This loop refuses past the cap instead.
///
/// **Refused, not parked.** The socket is accepted and immediately dropped,
/// which sends a reset. Waiting for a slot would give the attacker exactly what
/// they want: their one socket costs us a socket *and* a queue entry.
async fn serve_bounded(
    listener: tokio::net::TcpListener,
    app: Router,
    limiter: limits::ConnectionLimiter,
) -> std::io::Result<()> {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::server::conn::auto::Builder;
    use tower::Service;

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                // A per-connection accept error (EMFILE, a client vanishing
                // between the SYN and the accept) must not kill the listener.
                tracing::warn!(%error, "gateway accept failed; continuing");
                continue;
            }
        };

        let Some(permit) = limiter.try_acquire() else {
            tracing::warn!(
                %peer,
                in_flight = limiter.in_flight(),
                capacity = limiter.capacity(),
                "refused connection: at the connection cap"
            );
            drop(stream);
            continue;
        };

        let app = app.clone();
        tokio::spawn(async move {
            // The permit lives as long as the connection task, so the slot is
            // returned when the connection ends — however it ends.
            let _permit = permit;
            let io = TokioIo::new(stream);
            let service = hyper::service::service_fn(move |req| app.clone().call(req));
            if let Err(error) = Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, service)
                .await
            {
                tracing::debug!(%peer, %error, "gateway connection ended");
            }
        });
    }
}

// ── Native TLS support ──────────────────────────────────────

use std::path::PathBuf;

/// TLS configuration for the gateway.
#[derive(Clone)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    rustls: axum_server::tls_rustls::RustlsConfig,
}

impl TlsConfig {
    /// Parse both PEM files before startup security checks accept TLS.
    pub async fn load(cert_path: PathBuf, key_path: PathBuf) -> std::io::Result<Self> {
        if !cert_path.is_file() || !key_path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "TLS certificate and private key must both be readable files",
            ));
        }

        let rustls =
            axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path).await?;
        Ok(Self {
            cert_path,
            key_path,
            rustls,
        })
    }
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TlsConfig")
            .field("cert_path", &self.cert_path)
            .field("key_path", &self.key_path)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub struct MockEntity {
        pub ask_calls: AtomicUsize,
        pub goal_calls: AtomicUsize,
        pub plan_calls: AtomicUsize,
    }

    #[async_trait]
    impl EntityHandle for MockEntity {
        async fn ask(&self, prompt: &str) -> Result<String, String> {
            self.ask_calls.fetch_add(1, Ordering::SeqCst);
            Ok(format!("echo: {prompt}"))
        }
        async fn status(&self) -> serde_json::Value {
            serde_json::json!({"healthy": true, "goals": 0})
        }
        async fn create_goal(&self, desc: &str) -> Result<String, String> {
            self.goal_calls.fetch_add(1, Ordering::SeqCst);
            Ok(format!("goal-{desc}"))
        }
        async fn plan(&self, goal_id: &str) -> Result<Vec<String>, String> {
            self.plan_calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![format!("step1-{goal_id}"), format!("step2-{goal_id}")])
        }
        async fn execute_plan(&self, _goal_id: &str) -> Result<String, String> {
            Ok("executed".into())
        }
        async fn execute_shell(&self, cmd: &str) -> Result<String, String> {
            Ok(format!("ran: {cmd}"))
        }
        async fn run_cycle(&self) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({"cycle": "ok"}))
        }
        async fn list_goals(&self) -> Vec<serde_json::Value> {
            vec![serde_json::json!({"id": "g1", "desc": "test"})]
        }
        async fn replay_events(&self, _n: usize) -> Vec<EntityEvent> {
            vec![]
        }
        async fn alert(&self, _level: &str, _msg: &str) -> Result<(), String> {
            Ok(())
        }
        async fn spawn_subagent(&self, description: &str) -> Result<String, String> {
            Ok(format!("subagent-{description}"))
        }
        async fn list_subagents(&self) -> Vec<serde_json::Value> {
            vec![serde_json::json!({"id": "subagent-1", "status": "running"})]
        }
    }

    // ── EventHub tests ───────────────────────────────────────

    #[test]
    fn event_hub_publish_and_recent() {
        let hub = EventHub::new(10);
        hub.publish(EntityEvent::Heartbeat { ts: Utc::now() });
        hub.publish(EntityEvent::Heartbeat { ts: Utc::now() });
        assert_eq!(hub.len(), 2);
        assert!(!hub.is_empty());
        let recent = hub.recent(5);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn event_hub_capacity() {
        let hub = EventHub::new(3);
        for _i in 0..5 {
            hub.publish(EntityEvent::Heartbeat { ts: Utc::now() });
        }
        assert_eq!(hub.len(), 3);
    }

    #[test]
    fn event_hub_recent_zero() {
        let hub = EventHub::new(10);
        hub.publish(EntityEvent::Heartbeat { ts: Utc::now() });
        assert_eq!(hub.recent(0).len(), 0);
    }

    #[test]
    fn event_hub_recent_overflow() {
        let hub = EventHub::new(5);
        hub.publish(EntityEvent::Heartbeat { ts: Utc::now() });
        assert_eq!(hub.recent(100).len(), 1);
    }

    // ── GatewayState tests ───────────────────────────────────

    #[test]
    fn gateway_state_creation() {
        let entity = Arc::new(MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        });
        let state = GatewayState::new(entity);
        assert!(state.events.is_empty());
        assert_eq!(state.events.len(), 0);
    }

    // ── EntityEvent serialization ────────────────────────────

    #[test]
    fn entity_event_serialization() {
        let ev = EntityEvent::GoalCreated {
            id: "g1".into(),
            description: "test goal".into(),
            ts: Utc::now(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("GoalCreated"));
        assert!(json.contains("g1"));
        assert!(json.contains("test goal"));
    }

    #[test]
    fn entity_event_error_serialization() {
        let ev = EntityEvent::Error {
            message: "something broke".into(),
            ts: Utc::now(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("something broke"));
    }

    // ── MockEntity tests ─────────────────────────────────────

    #[tokio::test]
    async fn mock_entity_ask() {
        let entity = MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        };
        let resp = entity.ask("hello").await.unwrap();
        assert_eq!(resp, "echo: hello");
        assert_eq!(entity.ask_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn mock_entity_create_goal() {
        let entity = MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        };
        let id = entity.create_goal("learn rust").await.unwrap();
        assert_eq!(id, "goal-learn rust");
    }

    #[tokio::test]
    async fn mock_entity_plan() {
        let entity = MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        };
        let steps = entity.plan("g1").await.unwrap();
        assert_eq!(steps.len(), 2);
        assert!(steps[0].contains("g1"));
    }

    #[tokio::test]
    async fn mock_entity_status() {
        let entity = MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        };
        let status = entity.status().await;
        assert_eq!(status["healthy"], true);
    }

    // ── GatewayError tests ───────────────────────────────────

    #[test]
    fn gateway_error_display() {
        let err = GatewayError::BadRequest("invalid".into());
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn gateway_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let gw_err: GatewayError = io_err.into();
        assert!(gw_err.to_string().contains("file missing"));
    }

    #[tokio::test]
    async fn tls_config_rejects_invalid_pem_before_serving() {
        let directory = tempfile::tempdir().unwrap();
        let certificate = directory.path().join("certificate.pem");
        let key = directory.path().join("key.pem");
        std::fs::write(&certificate, "not a certificate").unwrap();
        std::fs::write(&key, "not a key").unwrap();
        assert!(TlsConfig::load(certificate, key).await.is_err());
    }

    // ── GatewayAuth ───────────────────────────────────────────

    #[test]
    fn auth_unconfigured_rejects_everything() {
        let auth = GatewayAuth::unconfigured();
        assert!(!auth.is_configured());
        assert!(!auth.verify(None));
        assert!(!auth.verify(Some("anything")));
    }

    #[test]
    fn auth_configured_requires_exact_match() {
        let auth = GatewayAuth::configured("s3cret");
        assert!(auth.is_configured());
        assert!(auth.verify(Some("s3cret")));
        assert!(!auth.verify(Some("wrong")));
        assert!(!auth.verify(Some("s3cret-extra")));
        assert!(!auth.verify(None));
    }

    #[test]
    fn auth_supports_multiple_named_users_without_debugging_secrets() {
        let auth =
            GatewayAuth::configured_users([("alice", "alice-secret"), ("bob", "bob-secret")]);
        assert_eq!(auth.principal_count(), 2);
        assert_eq!(auth.authenticate(Some("bob-secret")).unwrap().name(), "bob");
        assert!(!auth.verify(Some("charlie-secret")));
        let debug = format!("{auth:?}");
        assert!(!debug.contains("alice-secret"));
        assert!(!debug.contains("bob-secret"));
    }

    #[test]
    fn constant_time_eq_rejects_length_mismatch() {
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    // ── Router / CRIT-007: mandatory authentication ──────────

    fn mock_state(auth: GatewayAuth) -> GatewayState {
        let entity = Arc::new(MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        });
        GatewayState::with_auth(entity, auth)
    }

    async fn send(
        app: Router,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
        body: &str,
    ) -> axum::http::StatusCode {
        use tower::ServiceExt;
        let mut builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = builder
            .body(axum::body::Body::from(body.to_string()))
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn health_is_reachable_without_auth() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "GET", "/health", None, "").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn operator_route_rejects_missing_auth() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "GET", "/v1/status", None, "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn operator_route_rejects_wrong_token() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "GET", "/v1/status", Some("nope"), "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn operator_route_accepts_correct_token() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "GET", "/v1/status", Some("tok"), "").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn operator_route_rejects_when_auth_unconfigured() {
        // Fail closed: with no token configured at all, even a request that
        // supplies SOME bearer value is rejected — there is no implicit
        // "open" state (CRIT-007 / INV-NET-1).
        let app = router(mock_state(GatewayAuth::unconfigured()));
        let status = send(app, "GET", "/v1/status", Some("anything"), "").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn state_changing_operator_route_also_requires_auth() {
        // Not just status/read routes — the state-changing ones too.
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "POST", "/v1/goal", None, r#"{"description":"x"}"#).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn subagent_routes_are_authenticated_and_reach_the_entity() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        assert_eq!(
            send(
                app.clone(),
                "POST",
                "/v1/subagents",
                None,
                r#"{"description":"research"}"#,
            )
            .await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                app.clone(),
                "POST",
                "/v1/subagents",
                Some("tok"),
                r#"{"description":"research"}"#,
            )
            .await,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            send(app, "GET", "/v1/subagents", Some("tok"), "").await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn goals_and_events_disclosure_routes_require_auth() {
        let app = router(mock_state(GatewayAuth::configured("tok")));
        assert_eq!(
            send(app.clone(), "GET", "/v1/goals", None, "").await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(app, "GET", "/v1/events", None, "").await,
            StatusCode::UNAUTHORIZED
        );
    }

    // ── Router / HIGH-007: webhooks fail closed when unset ───

    #[tokio::test]
    async fn discord_webhook_fails_closed_when_secret_unset() {
        assert!(std::env::var("DISCORD_PUBLIC_KEY").is_err());
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "POST", "/providers/discord/webhook", None, "{}").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn slack_webhook_fails_closed_when_secret_unset() {
        assert!(std::env::var("SLACK_SIGNING_SECRET").is_err());
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "POST", "/providers/slack/webhook", None, "{}").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn whatsapp_webhook_fails_closed_when_secret_unset() {
        assert!(std::env::var("WHATSAPP_WEBHOOK_SECRET").is_err());
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "POST", "/providers/whatsapp/webhook", None, "{}").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn webhook_routes_do_not_require_bearer_auth() {
        // Webhooks come from external providers, not an operator — they must
        // not be rejected merely for lacking a Bearer header (they fail
        // closed on their OWN secret instead, tested above).
        let app = router(mock_state(GatewayAuth::configured("tok")));
        let status = send(app, "POST", "/providers/discord/webhook", None, "{}").await;
        assert_ne!(status, StatusCode::UNAUTHORIZED);
    }

    // ── Router / INV-NET-4: CORS is an explicit allowlist ────

    async fn send_with_origin(app: Router, uri: &str, origin: &str) -> axum::response::Response {
        use tower::ServiceExt;
        let req = axum::http::Request::builder()
            .method("GET")
            .uri(uri)
            .header(header::ORIGIN, origin)
            .body(axum::body::Body::empty())
            .unwrap();
        app.oneshot(req).await.unwrap()
    }

    fn allow_origin_header(resp: &axum::response::Response) -> Option<String> {
        resp.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    }

    #[tokio::test]
    async fn default_router_grants_no_cross_origin_access() {
        // The regression this closes: the router used to apply
        // `CorsLayer::permissive()` to the *merged* router, so every origin
        // got `Access-Control-Allow-Origin: *` on the authenticated operator
        // routes. With no allowlist configured the answer is now "no origin".
        let app = router_with_cors(
            mock_state(GatewayAuth::configured("tok")),
            limits::CorsPolicy::Disabled,
        );
        let resp = send_with_origin(app, "/health", "https://evil.example.com").await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "the request itself is served"
        );
        assert_eq!(
            allow_origin_header(&resp),
            None,
            "no Access-Control-Allow-Origin means the browser refuses the response"
        );
    }

    #[tokio::test]
    async fn allowlisted_origin_is_echoed_and_others_are_not() {
        let app = router_with_cors(
            mock_state(GatewayAuth::configured("tok")),
            limits::CorsPolicy::parse("https://ops.example.com"),
        );

        let allowed = send_with_origin(app.clone(), "/health", "https://ops.example.com").await;
        assert_eq!(
            allow_origin_header(&allowed).as_deref(),
            Some("https://ops.example.com")
        );

        let denied = send_with_origin(app, "/health", "https://evil.example.com").await;
        assert_eq!(
            allow_origin_header(&denied),
            None,
            "an origin outside the allowlist gets no header, and never `*`"
        );
    }

    #[tokio::test]
    async fn authenticated_operator_routes_are_covered_by_the_cors_policy() {
        // The layer sits on the merged router, so it must apply to `/v1/*` and
        // not only to `/health` — that breadth is what made `permissive()`
        // dangerous, and it is the same breadth the allowlist has to cover.
        let app = router_with_cors(
            mock_state(GatewayAuth::configured("tok")),
            limits::CorsPolicy::parse("https://ops.example.com"),
        );

        let denied = send_with_origin(app.clone(), "/v1/status", "https://evil.example.com").await;
        assert_eq!(allow_origin_header(&denied), None);

        let allowed = send_with_origin(app, "/v1/status", "https://ops.example.com").await;
        // Unauthenticated, so 401 — but the CORS decision is still made, which
        // is what "outermost layer" buys us.
        assert_eq!(allowed.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            allow_origin_header(&allowed).as_deref(),
            Some("https://ops.example.com")
        );
    }

    // ── Router / INV-NET-5: body, message and concurrency ────

    fn limited_state(max_body: usize, max_concurrent: usize) -> GatewayState {
        mock_state(GatewayAuth::configured("tok")).with_limits(limits::GatewayLimits {
            max_body_bytes: max_body,
            max_concurrent_requests: max_concurrent,
            ..limits::GatewayLimits::default()
        })
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_on_an_authenticated_route() {
        let app = router_with_cors(limited_state(256, 8), limits::CorsPolicy::Disabled);
        let body = format!(r#"{{"description":"{}"}}"#, "x".repeat(4096));
        assert_eq!(
            send(app, "POST", "/v1/goal", Some("tok"), &body).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[tokio::test]
    async fn a_body_within_the_limit_is_still_served() {
        // The bound must not be so eager that it breaks ordinary requests.
        let app = router_with_cors(limited_state(4096, 8), limits::CorsPolicy::Disabled);
        assert_eq!(
            send(
                app,
                "POST",
                "/v1/goal",
                Some("tok"),
                r#"{"description":"ship it"}"#
            )
            .await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_before_the_webhook_handler_runs() {
        // This is the unauthenticated surface, so it is the one that matters:
        // the extractor's bound is reached before any handler logic, which is
        // why the answer is 413 and not the handler's own 503.
        assert!(std::env::var("SLACK_SIGNING_SECRET").is_err());
        let app = router_with_cors(limited_state(256, 8), limits::CorsPolicy::Disabled);
        let status = send(
            app,
            "POST",
            "/providers/slack/webhook",
            None,
            &"x".repeat(4096),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// An entity whose `ask` parks inside the handler until the test releases
    /// it, so "how many requests are in flight" is directly observable.
    struct GateEntity {
        entered: Arc<AtomicUsize>,
        gate: Arc<tokio::sync::Semaphore>,
    }

    #[async_trait]
    impl EntityHandle for GateEntity {
        async fn ask(&self, _prompt: &str) -> Result<String, String> {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let permit = self.gate.acquire().await.map_err(|e| e.to_string())?;
            permit.forget();
            Ok("released".into())
        }
        async fn status(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn create_goal(&self, _desc: &str) -> Result<String, String> {
            Ok("g".into())
        }
        async fn plan(&self, _goal_id: &str) -> Result<Vec<String>, String> {
            Ok(vec![])
        }
        async fn execute_plan(&self, _goal_id: &str) -> Result<String, String> {
            Ok(String::new())
        }
        async fn execute_shell(&self, _cmd: &str) -> Result<String, String> {
            Ok(String::new())
        }
        async fn run_cycle(&self) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({}))
        }
        async fn list_goals(&self) -> Vec<serde_json::Value> {
            vec![]
        }
        async fn replay_events(&self, _n: usize) -> Vec<EntityEvent> {
            vec![]
        }
        async fn alert(&self, _level: &str, _msg: &str) -> Result<(), String> {
            Ok(())
        }
        async fn spawn_subagent(&self, _description: &str) -> Result<String, String> {
            Ok("s".into())
        }
        async fn list_subagents(&self) -> Vec<serde_json::Value> {
            vec![]
        }
    }

    #[tokio::test]
    async fn concurrency_limit_bounds_requests_in_flight() {
        use std::time::Duration;

        let entered = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let entity = Arc::new(GateEntity {
            entered: Arc::clone(&entered),
            gate: Arc::clone(&gate),
        });
        let state = GatewayState::with_auth(entity, GatewayAuth::configured("tok")).with_limits(
            limits::GatewayLimits {
                max_concurrent_requests: 1,
                ..limits::GatewayLimits::default()
            },
        );
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        let ask = |app: Router| async move {
            send(app, "POST", "/v1/ask", Some("tok"), r#"{"prompt":"hi"}"#).await
        };

        let first = tokio::spawn(ask(app.clone()));
        for _ in 0..200 {
            if entered.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            entered.load(Ordering::SeqCst),
            1,
            "the first request should have reached the handler"
        );

        let second = tokio::spawn(ask(app.clone()));
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            entered.load(Ordering::SeqCst),
            1,
            "the second request must wait for a permit, not run concurrently"
        );

        // Releasing the first frees its permit, so the second proceeds — the
        // limit throttles, it does not drop.
        gate.add_permits(2);
        assert_eq!(first.await.unwrap(), StatusCode::OK);
        assert_eq!(second.await.unwrap(), StatusCode::OK);
        assert_eq!(entered.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn the_concurrency_budget_is_shared_across_routes() {
        // `ConcurrencyLimitLayer` would hand each route its own semaphore under
        // `Router::layer`, making the real ceiling N × routes. Two different
        // routes must therefore contend for the same single permit.
        use std::time::Duration;

        let entered = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let entity = Arc::new(GateEntity {
            entered: Arc::clone(&entered),
            gate: Arc::clone(&gate),
        });
        let state = GatewayState::with_auth(entity, GatewayAuth::configured("tok")).with_limits(
            limits::GatewayLimits {
                max_concurrent_requests: 1,
                ..limits::GatewayLimits::default()
            },
        );
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        let blocking = tokio::spawn({
            let app = app.clone();
            async move { send(app, "POST", "/v1/ask", Some("tok"), r#"{"prompt":"hi"}"#).await }
        });
        for _ in 0..200 {
            if entered.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(entered.load(Ordering::SeqCst), 1);

        // A *different* route, which would be instant if it had its own budget.
        let other = tokio::spawn({
            let app = app.clone();
            async move { send(app, "GET", "/health", None, "").await }
        });
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !other.is_finished(),
            "a second route shares the same permit"
        );

        gate.add_permits(2);
        assert_eq!(blocking.await.unwrap(), StatusCode::OK);
        assert_eq!(other.await.unwrap(), StatusCode::OK);
    }

    #[test]
    fn state_carries_the_websocket_message_bound() {
        // `handle_ws` reads the bound off the state at upgrade time; this
        // guards the plumbing that makes that possible.
        let st = mock_state(GatewayAuth::configured("tok"))
            .with_limits(limits::GatewayLimits::default());
        assert_eq!(
            st.limits.max_ws_message_bytes,
            limits::DEFAULT_MAX_WS_MESSAGE_BYTES
        );
        assert!(st.limits.max_ws_message_bytes > 0);
    }

    // ── Router / CRIT-007 residual: per-scope authorization ──

    fn scoped_state(token: &str, scopes: scope::ScopeSet) -> GatewayState {
        let entity = Arc::new(MockEntity {
            ask_calls: AtomicUsize::new(0),
            goal_calls: AtomicUsize::new(0),
            plan_calls: AtomicUsize::new(0),
        });
        GatewayState::with_auth(
            entity,
            GatewayAuth::configured_scoped("scoped", token, scopes),
        )
    }

    /// Every `/v1/*` route, with the scope it is expected to require.
    ///
    /// Kept as data so `every_operator_route_declares_a_scope` can compare it
    /// against the routes actually declared in the source.
    const SCOPED_ROUTES: &[(&str, &str, scope::Scope)] = &[
        ("GET", "/v1/status", scope::Scope::Read),
        ("GET", "/v1/goals", scope::Scope::Read),
        ("GET", "/v1/events", scope::Scope::Read),
        ("GET", "/v1/subagents", scope::Scope::Read),
        // A plain GET without upgrade headers: the scope layer runs before the
        // WebSocket upgrade, so an under-scoped principal is refused with 403
        // rather than getting as far as a handshake.
        ("GET", "/v1/stream", scope::Scope::Read),
        ("POST", "/v1/goal", scope::Scope::Write),
        ("POST", "/v1/plan/g1", scope::Scope::Write),
        ("POST", "/v1/ask", scope::Scope::Exec),
        ("POST", "/v1/execute/g1", scope::Scope::Exec),
        ("POST", "/v1/run", scope::Scope::Exec),
        ("POST", "/v1/cycle", scope::Scope::Exec),
        ("POST", "/v1/subagents", scope::Scope::Exec),
    ];

    fn body_for(uri: &str) -> &'static str {
        if uri.starts_with("/v1/goal") || uri.starts_with("/v1/subagents") {
            r#"{"description":"x"}"#
        } else if uri.starts_with("/v1/ask") {
            r#"{"prompt":"x"}"#
        } else if uri.starts_with("/v1/run") {
            r#"{"command":"ls"}"#
        } else {
            "{}"
        }
    }

    #[tokio::test]
    async fn a_principal_holding_only_read_cannot_write_or_exec() {
        let state = scoped_state("tok", scope::ScopeSet::from_scopes([scope::Scope::Read]));
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        assert_eq!(
            send(app.clone(), "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::OK
        );
        for (method, uri, required) in SCOPED_ROUTES {
            if *required == scope::Scope::Read {
                continue;
            }
            assert_eq!(
                send(app.clone(), method, uri, Some("tok"), body_for(uri)).await,
                StatusCode::FORBIDDEN,
                "{method} {uri} requires {required} and must be refused"
            );
        }
    }

    #[tokio::test]
    async fn a_principal_holding_only_write_cannot_exec_or_read() {
        let state = scoped_state("tok", scope::ScopeSet::from_scopes([scope::Scope::Write]));
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        assert_eq!(
            send(
                app.clone(),
                "POST",
                "/v1/goal",
                Some("tok"),
                r#"{"description":"x"}"#
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(app.clone(), "POST", "/v1/run", Some("tok"), r#"{"command":"ls"}"#).await,
            StatusCode::FORBIDDEN,
            "shell execution is the scope that turns a leaked token into host              compromise; write must not imply it"
        );
        assert_eq!(
            send(app, "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::FORBIDDEN,
            "scopes do not imply one another in either direction"
        );
    }

    #[tokio::test]
    async fn a_principal_holding_exec_reaches_the_execution_routes() {
        let state = scoped_state("tok", scope::ScopeSet::from_scopes([scope::Scope::Exec]));
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        for (method, uri, required) in SCOPED_ROUTES {
            if *required != scope::Scope::Exec {
                continue;
            }
            assert_ne!(
                send(app.clone(), method, uri, Some("tok"), body_for(uri)).await,
                StatusCode::FORBIDDEN,
                "{method} {uri} requires {required}, which this principal holds"
            );
        }
    }

    #[tokio::test]
    async fn an_unscoped_legacy_credential_still_reaches_everything() {
        // The recorded product decision: scopes are opt-in, so an existing
        // deployment's token keeps working exactly as it did.
        let app = router_with_cors(
            mock_state(GatewayAuth::configured("tok")),
            limits::CorsPolicy::Disabled,
        );
        for (method, uri, _) in SCOPED_ROUTES {
            assert_ne!(
                send(app.clone(), method, uri, Some("tok"), body_for(uri)).await,
                StatusCode::FORBIDDEN,
                "{method} {uri} must stay reachable for an unscoped credential"
            );
        }
    }

    #[tokio::test]
    async fn insufficient_scope_is_403_not_401() {
        // 401 would invite the client to retry with fresh credentials that
        // would fail identically. The caller authenticated fine.
        let state = scoped_state("tok", scope::ScopeSet::from_scopes([scope::Scope::Read]));
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);
        assert_eq!(
            send(app, "POST", "/v1/run", Some("tok"), r#"{"command":"ls"}"#).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn a_credential_holding_no_scopes_reaches_nothing() {
        let state = scoped_state("tok", scope::ScopeSet::none());
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);
        for (method, uri, _) in SCOPED_ROUTES {
            assert_eq!(
                send(app.clone(), method, uri, Some("tok"), body_for(uri)).await,
                StatusCode::FORBIDDEN,
                "{method} {uri}"
            );
        }
        assert_eq!(
            send(app, "GET", "/health", None, "").await,
            StatusCode::OK,
            "the unauthenticated liveness probe is unaffected"
        );
    }

    /// A route added to `router_with_cors` outside every scoped group would
    /// silently have no authorization requirement. This reads the source rather
    /// than trusting anyone to remember to update `SCOPED_ROUTES`.
    #[test]
    fn every_operator_route_declares_a_scope() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
            .expect("own source is readable");

        let declared: std::collections::BTreeSet<String> = source
            .match_indices(".route(\"/v1/")
            .map(|(index, _)| {
                let rest = &source[index + ".route(\"".len()..];
                let end = rest.find('"').expect("route literal is terminated");
                rest[..end].to_owned()
            })
            .collect();

        // SCOPED_ROUTES carries concrete paths (`/v1/plan/g1`); the source
        // carries patterns (`/v1/plan/:goal_id`). Compare on the first segment
        // pair, which is what identifies the route.
        let prefix =
            |path: &str| -> String { path.split('/').take(3).collect::<Vec<_>>().join("/") };
        let covered: std::collections::BTreeSet<String> = SCOPED_ROUTES
            .iter()
            .map(|(_, uri, _)| prefix(uri))
            .collect();

        for route in &declared {
            assert!(
                covered.contains(&prefix(route)),
                "route {route} is declared in router_with_cors but is not in \
                 SCOPED_ROUTES, so nothing proves it requires a scope"
            );
        }
        assert!(
            !declared.is_empty(),
            "the scan found no routes at all — the extraction is broken, not \
             the router"
        );
    }

    // ── Per-client rate limiting on the router (P1-11, INV-NET-5) ────────

    #[tokio::test]
    async fn a_client_exceeding_its_budget_is_throttled_with_429() {
        let limits = limits::GatewayLimits {
            rate_per_second: 1,
            rate_burst: 2,
            ..Default::default()
        };
        let state = mock_state(GatewayAuth::configured("tok")).with_limits(limits);
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        assert_eq!(
            send(app.clone(), "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::OK
        );
        assert_eq!(
            send(app.clone(), "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::OK
        );
        assert_eq!(
            send(app, "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::TOO_MANY_REQUESTS,
            "the burst is spent"
        );
    }

    #[tokio::test]
    async fn throttling_one_principal_does_not_throttle_another() {
        // The P1-11 acceptance test: one operator token saturating its budget
        // must not starve everyone else.
        let limits = limits::GatewayLimits {
            rate_per_second: 1,
            rate_burst: 1,
            ..Default::default()
        };
        let state = mock_state(GatewayAuth::configured_users([
            ("noisy", "tok-noisy"),
            ("quiet", "tok-quiet"),
        ]))
        .with_limits(limits);
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        assert_eq!(
            send(app.clone(), "GET", "/v1/status", Some("tok-noisy"), "").await,
            StatusCode::OK
        );
        assert_eq!(
            send(app.clone(), "GET", "/v1/status", Some("tok-noisy"), "").await,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            send(app, "GET", "/v1/status", Some("tok-quiet"), "").await,
            StatusCode::OK,
            "a different principal has its own budget"
        );
    }

    #[tokio::test]
    async fn the_budget_is_charged_after_authentication_not_before() {
        // Charging before authenticating would let an anonymous flood exhaust
        // a real principal's allowance — the limiter would become the attack.
        let limits = limits::GatewayLimits {
            rate_per_second: 1,
            rate_burst: 1,
            ..Default::default()
        };
        let state = mock_state(GatewayAuth::configured("tok")).with_limits(limits);
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        for _ in 0..20 {
            assert_eq!(
                send(app.clone(), "GET", "/v1/status", None, "").await,
                StatusCode::UNAUTHORIZED,
                "anonymous requests are rejected and cost no budget"
            );
        }
        assert_eq!(
            send(app, "GET", "/v1/status", Some("tok"), "").await,
            StatusCode::OK,
            "the real principal's allowance survived the anonymous flood"
        );
    }

    #[tokio::test]
    async fn a_throttled_response_carries_retry_after() {
        let limits = limits::GatewayLimits {
            rate_per_second: 1,
            rate_burst: 1,
            ..Default::default()
        };
        let state = mock_state(GatewayAuth::configured("tok")).with_limits(limits);
        let app = router_with_cors(state, limits::CorsPolicy::Disabled);

        let request = |token: &str| {
            axum::http::Request::builder()
                .method("GET")
                .uri("/v1/status")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .unwrap()
        };
        use tower::ServiceExt;
        let _ = app.clone().oneshot(request("tok")).await.unwrap();
        let throttled = app.oneshot(request("tok")).await.unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            throttled.headers().contains_key(header::RETRY_AFTER),
            "a client that is told to back off needs to know for how long"
        );
    }

    #[test]
    fn the_gateway_is_served_through_the_bounded_accept_loop() {
        // `axum::serve` accepts every connection the OS offers, so reverting
        // to it would silently remove the connection cap.
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
            .expect("own source is readable");
        let serve_fn = source
            .split("pub async fn serve_with_tls(")
            .nth(1)
            .expect("serve_with_tls is present");
        let serve_fn = &serve_fn[..serve_fn
            .find("\n/// Serve with a hard cap")
            .unwrap_or(serve_fn.len())];
        assert!(
            serve_fn.contains("serve_bounded"),
            "the plain-HTTP path must use the bounded accept loop"
        );
        assert!(
            !serve_fn.contains("axum::serve("),
            "axum::serve accepts every connection offered; the cap would be lost"
        );
    }
}
