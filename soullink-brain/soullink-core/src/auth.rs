//! Bearer-token middleware for axum 0.7.
//!
//! ## Policy
//! * Requests carrying a valid `Authorization: Bearer <hex>` header pass.
//! * In **cohabitation mode** (`allow_localhost_unauthed = true`), requests
//!   from `127.0.0.1` or `::1` without a token are allowed with a warning log.
//!   This is ON during phase 1 for the legacy Python bot, OFF from phase 6.
//! * All other cases: `401 Unauthorized`.
//!
//! Comparison is constant-time via [`subtle::ConstantTimeEq`] to prevent
//! timing side-channels.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;
use tracing::{debug, warn};

use crate::secrets::InternalToken;

/// Configuration passed via axum `Extension<AuthConfig>` or captured in a closure.
#[derive(Clone)]
pub struct AuthConfig {
    pub token: Arc<InternalToken>,
    /// If true, requests from 127.0.0.1 / ::1 without a token are allowed
    /// (with a `warn!` log). Flip to false once all internal callers
    /// have been migrated to token auth.
    pub allow_localhost_unauthed: bool,
}

impl AuthConfig {
    pub fn new(token: InternalToken, allow_localhost_unauthed: bool) -> Self {
        Self {
            token: Arc::new(token),
            allow_localhost_unauthed,
        }
    }
}

/// Axum middleware. Use with `Router::layer(middleware::from_fn_with_state(cfg, require_auth))`
/// or `axum::middleware::from_fn` capturing the config.
pub async fn require_auth(
    axum::extract::State(cfg): axum::extract::State<AuthConfig>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // 1. Extract Authorization header
    let header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::trim))
        .unwrap_or("");

    if !header.is_empty() {
        // 2a. Validate token (constant-time)
        if let Ok(supplied) = InternalToken::from_hex(header) {
            let a = cfg.token.0.as_slice();
            let b = supplied.0.as_slice();
            if a.ct_eq(b).into() {
                debug!(peer = %peer, "auth ok (token)");
                return next.run(req).await;
            }
        }
        warn!(peer = %peer, "auth failed — bad token");
        return unauthorized();
    }

    // 2b. Cohabitation: localhost without token
    if cfg.allow_localhost_unauthed && is_loopback(peer.ip()) {
        debug!(
            peer = %peer,
            path = %req.uri().path(),
            "localhost call without token (cohabitation mode)"
        );
        return next.run(req).await;
    }

    warn!(peer = %peer, "auth failed — no token, not localhost");
    unauthorized()
}

#[inline]
fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(axum::http::header::WWW_AUTHENTICATE, "Bearer realm=\"soullink\"")],
        "unauthorized",
    )
        .into_response()
}
