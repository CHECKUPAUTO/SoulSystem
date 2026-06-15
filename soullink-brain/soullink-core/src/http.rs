//! Shared HTTP client — single `reqwest::Client` per process, kept alive
//! for the whole runtime via `OnceLock`.
//!
//! # Why
//!
//! V13.5 Phase 1 shipped with two regressions I missed:
//!
//! 1. `guardian/ws_client.rs::push_snapshot` created a new `reqwest::Client`
//!    on **every** call (every 5 s) — so the TCP pool was torn down and
//!    rebuilt every tick.
//! 2. `guardian/homeosync.rs::tick` did the same for the stimulate-homeostasis
//!    POST to `:9041`.
//!
//! That throws away keep-alive, redoes the TLS/HTTP2 negotiation, and churns
//! file descriptors pointlessly. This module provides a single process-wide
//! client tuned for the guardian↔orchestrator and guardian↔brain traffic
//! profile (short keep-alive, small JSON bodies).
//!
//! # Usage
//!
//! ```ignore
//! use soullink_core::http::shared_client;
//! let client = shared_client();
//! client.post(url).json(&body).send().await?;
//! ```
//!
//! The client is built lazily on first call and never torn down — the
//! `Arc<Client>` stays alive until process exit.

use std::{sync::OnceLock, time::Duration};

use reqwest::Client;

static SHARED: OnceLock<Client> = OnceLock::new();

/// Tuning constants — conservative defaults appropriate for an internal mesh.
/// Adjust only with measurements.
const POOL_MAX_IDLE_PER_HOST: usize = 32;
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Get the shared client, building it on first call.
///
/// Panics only if reqwest fails to build (TLS backend broken, OS limits).
/// In that case the process cannot do HTTP at all — fatal is the right
/// response.
pub fn shared_client() -> &'static Client {
    SHARED.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .tcp_keepalive(TCP_KEEPALIVE)
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("soullink/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client build failed — TLS backend broken?")
    })
}

/// Escape hatch: inject a custom client (e.g. `wiremock`-friendly) before
/// any code calls [`shared_client`]. Returns `Err(())` if the client was
/// already initialised.
///
/// Only intended for tests. Attempts in production code must go through
/// [`shared_client`].
#[cfg(any(test, feature = "testing"))]
pub fn set_shared_for_testing(c: Client) -> Result<(), Box<Client>> {
    SHARED.set(c).map_err(Box::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_returns_same_instance() {
        let a = shared_client();
        let b = shared_client();
        // Same Arc-backed handle — pointer equality is the right check
        assert!(std::ptr::eq(a, b));
    }
}
