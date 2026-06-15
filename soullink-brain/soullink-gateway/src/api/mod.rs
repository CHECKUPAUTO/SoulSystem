//! API module — axum router for REST endpoints.
//!
//! Routes:
//! - GET /health                  — health check (existing)
//! - POST /api/chat               — chat with LLM
//! - POST /api/completion         — simple completion
//! - GET  /api/providers          — list configured providers
//! - POST /api/tools/call         — tool invocation
//! - POST /api/webhooks/{provider} — webhook ingress
//! - POST /api/mcp                — Model Context Protocol

use std::sync::{Arc, Mutex, RwLock};

use avid_model_router::{OutcomeLog, RouterParams};
use axum::Router;

use crate::channels::ChannelRegistry;
use crate::provider::registry::ProviderRegistry;

pub mod channels;
pub mod chat;
pub mod completion;
pub mod models;
pub mod providers;
pub mod tools;
pub mod webhooks;

/// Shared application state for route handlers.
#[derive(Clone)]
pub struct ApiState {
    pub registry: Arc<ProviderRegistry>,
    /// Live difficulty-router parameters; retraining replaces these so future
    /// requests route with the learned model.
    pub router_params: Arc<RwLock<RouterParams>>,
    /// Logged routing outcomes, the training data for [`ApiState::retrain`].
    pub outcomes: Arc<Mutex<OutcomeLog>>,
    /// Configured outbound channels for dispatching replies.
    pub channels: Arc<ChannelRegistry>,
}

impl ApiState {
    /// Build state with default router params, an empty outcome log, and the
    /// outbound channels configured from the environment.
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        Self {
            registry,
            router_params: Arc::new(RwLock::new(RouterParams::default())),
            outcomes: Arc::new(Mutex::new(OutcomeLog::default())),
            channels: Arc::new(ChannelRegistry::from_env()),
        }
    }

    /// Record a routing outcome for later retraining.
    pub fn record_outcome(&self, query: &str, needed_strong: f64) {
        if let Ok(mut log) = self.outcomes.lock() {
            log.record(avid_model_router::RoutingOutcome::new(
                query,
                Vec::new(),
                needed_strong,
            ));
        }
    }

    /// Retrain the difficulty model on the logged outcomes and install the
    /// updated parameters for subsequent routing. Returns the final log-loss.
    pub fn retrain(&self, epochs: usize, lr: f64) -> f64 {
        let log = match self.outcomes.lock() {
            Ok(l) => l.clone(),
            Err(_) => return 0.0,
        };
        let params = self
            .router_params
            .read()
            .map(|p| p.clone())
            .unwrap_or_default();
        let mut router = avid_model_router::CostAwareRouter::new(
            avid_model_router::ModelRegistry::new(),
            params,
        );
        let loss = router.train_on_outcomes(&log, epochs, lr);
        if let Ok(mut p) = self.router_params.write() {
            *p = router.params().clone();
        }
        loss
    }
}

/// Build the axum router with all API routes.
pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/models", axum::routing::get(models::list_models))
        .route("/api/chat", axum::routing::post(chat::chat_handler))
        .route(
            "/api/completion",
            axum::routing::post(completion::completion_handler),
        )
        .route(
            "/api/providers",
            axum::routing::get(providers::list_providers),
        )
        .route(
            "/api/tools/call",
            axum::routing::post(tools::tools_call_handler),
        )
        .route(
            "/api/webhooks/{provider}",
            axum::routing::post(webhooks::webhook_handler).get(webhooks::webhook_verify),
        )
        .route(
            "/api/channels/{provider}/send",
            axum::routing::post(channels::send_handler),
        )
        .route("/api/mcp", axum::routing::post(crate::mcp::mcp_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrain_consumes_logged_outcomes() {
        let state = ApiState::new(Arc::new(ProviderRegistry::new()));
        // Empty log → no-op.
        assert!(state.retrain(10, 0.1).abs() < 1e-9);

        // Log a separable set and retrain; loss is finite and non-negative.
        for _ in 0..6 {
            state.record_outcome("hi", 0.0);
            state.record_outcome(
                "Prove the distributed deadlock root cause step by step and design a fix",
                1.0,
            );
        }
        assert_eq!(state.outcomes.lock().unwrap().len(), 12);
        let loss = state.retrain(200, 0.3);
        assert!(loss.is_finite() && loss >= 0.0);
    }
}
