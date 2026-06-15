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

use std::sync::Arc;

use axum::Router;

use crate::provider::registry::ProviderRegistry;

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
        .route("/api/mcp", axum::routing::post(crate::mcp::mcp_handler))
        .with_state(state)
}
