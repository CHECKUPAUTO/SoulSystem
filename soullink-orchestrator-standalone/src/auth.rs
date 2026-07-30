//! Bearer-token authentication for the orchestrator's HTTP surface (MED-013-B).
//!
//! `POST /api/mesh/spawn` turns a request body into a `python3` process. Three
//! defects in that route were already fixed — argument injection, the missing
//! sandbox, and the absent ceiling — but the route remained *unauthenticated*,
//! so anyone who could reach the port could make this host run the code it was
//! configured to run, up to `MAX_LIVE_BRAINS`.
//!
//! `soul_cors` was never a substitute. It stops a browser on another origin;
//! it stops nothing that is not a browser, and `curl` is not a browser.
//!
//! The token comparison lives in `soul-auth` rather than here, so the
//! workspace has one constant-time comparison instead of one per service.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use soul_auth::{Principal, TokenAuth};

/// Comma-separated `principal=token` pairs.
pub const TOKENS_VAR: &str = "SOULLINK_ORCHESTRATOR_TOKENS";
/// Single-token shorthand.
pub const TOKEN_VAR: &str = "SOULLINK_ORCHESTRATOR_TOKEN";
/// Named in the startup error so an operator is told what to set.
pub const AUTH_VARS: &str = "SOULLINK_ORCHESTRATOR_TOKENS or SOULLINK_ORCHESTRATOR_TOKEN";

pub const SERVICE: &str = "soullink-orchestrator";

/// Read the orchestrator's credentials from the environment.
///
/// Deliberately its own variables rather than the gateway's: sharing one token
/// would mean a leaked dashboard credential also spawns processes here.
pub fn auth_from_env() -> TokenAuth {
    TokenAuth::from_env(TOKENS_VAR, TOKEN_VAR)
}

/// Reject any request without a valid bearer token.
///
/// Applied with `route_layer`, so an unknown path still answers 404 rather
/// than 401 — a 401 on every unrouted path would tell a prober that the
/// service exists and is listening, and confirm nothing else useful.
pub async fn require_auth(State(auth): State<TokenAuth>, mut req: Request, next: Next) -> Response {
    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth.authenticate_header(header_value) {
        Some(principal) => {
            // Handlers can record who acted; the spawn route logs it.
            req.extensions_mut().insert(principal);
            next.run(req).await
        }
        None => {
            // One message for "no header" and "wrong token" alike: telling
            // them apart is an oracle for whether a token is merely stale.
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            )
                .into_response()
        }
    }
}

/// The principal that authenticated a request, if the layer ran.
pub fn principal_of(req: &Request) -> Option<&Principal> {
    req.extensions().get::<Principal>()
}
