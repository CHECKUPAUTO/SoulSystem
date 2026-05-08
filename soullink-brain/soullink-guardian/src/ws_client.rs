//! OrchestratorClient — HTTP push to the Orchestrator.
//!
//! Features:
//! * Circuit breaker: 3 consecutive failures → OPEN 60 s → HALF-OPEN
//! * 10 s request timeout
//! * Correlation-ID propagation
//!
//! Phase 1: auth token header added when `SOULLINK_INTERNAL_TOKEN` is set in
//! the environment. The orchestrator in cohabitation mode accepts both
//! authed and localhost-unauthed requests.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use soullink_core::{http::shared_client, SystemSnapshot};

#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,
    Open { until: Instant },
    HalfOpen,
}

pub struct OrchestratorClient {
    url:       String,
    http_base: String,
    state:     Arc<ArcSwap<SystemSnapshot>>,
    circuit:   Arc<RwLock<CircuitState>>,
    failures:  Arc<RwLock<u32>>,
    token:     Option<String>,
}

impl OrchestratorClient {
    pub fn new(url: String, state: Arc<ArcSwap<SystemSnapshot>>) -> Self {
        let http_base = url
            .replace("ws://", "http://")
            .replace("wss://", "https://")
            .replace("/mesh/v13", "");

        let token = std::env::var("SOULLINK_INTERNAL_TOKEN").ok();
        if token.is_some() {
            info!("orchestrator client: auth token loaded from $SOULLINK_INTERNAL_TOKEN");
        } else {
            warn!("orchestrator client: $SOULLINK_INTERNAL_TOKEN not set — requests will use cohabitation mode");
        }
        Self {
            url,
            http_base,
            state,
            circuit: Arc::new(RwLock::new(CircuitState::Closed)),
            failures: Arc::new(RwLock::new(0)),
            token,
        }
    }

    pub async fn push_snapshot(&self, snapshot: &Arc<SystemSnapshot>) {
        // Circuit breaker gate
        {
            let c = self.circuit.read().await;
            if let CircuitState::Open { until } = *c {
                if Instant::now() < until {
                    debug!("circuit OPEN — skipping push");
                    return;
                }
                drop(c);
                let mut c = self.circuit.write().await;
                *c = CircuitState::HalfOpen;
            }
        }

        let payload = serde_json::to_string(snapshot.as_ref()).unwrap_or_default();
        let client = shared_client();

        let mut req = client
            .post(format!("{}/api/guardian/snapshot", self.http_base))
            .header("Content-Type", "application/json")
            .header("X-Correlation-ID", snapshot.correlation_id.to_string())
            .timeout(Duration::from_secs(10))
            .body(payload);

        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                *self.failures.write().await = 0;
                *self.circuit.write().await = CircuitState::Closed;
                debug!("snapshot pushed OK");
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "orchestrator non-2xx");
                self.record_failure().await;
            }
            Err(e) if e.is_timeout() => {
                warn!("orchestrator push timeout");
                self.record_failure().await;
            }
            Err(e) => {
                error!(%e, "orchestrator push failed");
                self.record_failure().await;
            }
        }
    }

    async fn record_failure(&self) {
        let mut f = self.failures.write().await;
        *f += 1;
        if *f >= 3 {
            warn!(failures = *f, "circuit OPEN — 3 consecutive failures");
            *self.circuit.write().await = CircuitState::Open {
                until: Instant::now() + Duration::from_secs(60),
            };
        }
    }

    #[allow(dead_code)]
    pub fn url(&self) -> &str { &self.url }
}
