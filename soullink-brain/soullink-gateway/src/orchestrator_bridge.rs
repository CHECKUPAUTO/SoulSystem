//! HTTP bridge to the orchestrator.
//!
//! Posts the inbound Telegram message to `/api/mesh/think` and returns the
//! textual reply. In 6a we use the simple synthesis endpoint (reply text +
//! optional sources). Streaming & SSE come in 6b.
//!
//! Auth: the gateway runs on localhost and the orchestrator is in
//! cohabitation mode, so no Bearer token is needed for the loopback path.
//! If the orchestrator is later moved off-box, we'll add Bearer support
//! (secrets module already handles it).

use std::{sync::Arc, time::Duration};

use futures::future::BoxFuture;
use serde_json::{json, Value};
use tracing::{debug, warn};

use soullink_core::http::shared_client;

use crate::telegram::long_poll::{IncomingMessage, OrchestratorBridge};

pub fn build_bridge(orchestrator_url: String, timeout: Duration) -> OrchestratorBridge {
    Arc::new(
        move |incoming: IncomingMessage| -> BoxFuture<'static, Result<String, String>> {
            let url = format!("{}/api/mesh/think", orchestrator_url.trim_end_matches('/'));
            let timeout = timeout;
            Box::pin(async move {
                let client = shared_client();
                let body = json!({
                    "query":   incoming.text,
                    "source":  "telegram",
                    "chat_id": incoming.chat_id,
                    "user":    incoming.username,
                });
                debug!(url = %url, chat_id = incoming.chat_id, "bridge → orchestrator");

                let resp = client
                    .post(&url)
                    .timeout(timeout)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("request failed: {e}"))?;

                let status = resp.status();
                if !status.is_success() {
                    return Err(format!("orchestrator returned HTTP {status}"));
                }

                let body: Value = resp
                    .json()
                    .await
                    .map_err(|e| format!("orchestrator returned non-JSON: {e}"))?;

                // The orchestrator /api/mesh/think handler returns {"reply": "...", ...}.
                // We accept either "reply" or "synthesis" for forward compatibility.
                let reply = body
                    .get("reply")
                    .or_else(|| body.get("synthesis"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();

                if reply.is_empty() {
                    warn!("orchestrator returned empty reply field");
                }
                Ok(reply)
            })
        },
    )
}
