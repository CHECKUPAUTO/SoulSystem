//! POST /api/channels/{provider}/send — dispatch an outbound message.
//!
//! The outbound half of the messaging bridge: given a provider, a target
//! address, and text, route the message through the configured channel
//! ([`ChannelRegistry`](crate::channels::ChannelRegistry)). This is how the
//! gateway replies on whatever channel an inbound message arrived from.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::api::ApiState;

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    /// Channel-specific address (phone number, Slack/Discord channel ID, …).
    pub target: String,
    /// Message body.
    pub text: String,
}

pub async fn send_handler(
    State(state): State<ApiState>,
    Path(provider): Path<String>,
    Json(req): Json<SendRequest>,
) -> impl IntoResponse {
    match state
        .channels
        .dispatch(&provider, &req.target, &req.text)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => {
            warn!(provider = %provider, err = %e, "channel dispatch failed");
            // No such channel → 404; a disabled/failed send → 502.
            let status = if e.contains("no channel configured") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_GATEWAY
            };
            (status, Json(json!({ "ok": false, "error": e })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::registry::ProviderRegistry;
    use std::sync::Arc;

    #[tokio::test]
    async fn send_to_unconfigured_channel_is_404() {
        let state = ApiState::new(Arc::new(ProviderRegistry::new()));
        let resp = send_handler(
            State(state),
            Path("nope".into()),
            Json(SendRequest {
                target: "x".into(),
                text: "hi".into(),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
