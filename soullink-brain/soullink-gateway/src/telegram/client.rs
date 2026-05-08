//! Thin HTTP client around the Telegram Bot API.
//!
//! Uses the process-wide shared `reqwest::Client` from `soullink-core::http`.
//! **No token in logs** — `Debug` on `TelegramClient` redacts it; the token
//! only appears in URL path parameters and we never log request URLs.

use std::time::Duration;

use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use tracing::{debug, trace};

use soullink_core::http::shared_client;

use super::types::{ApiResponse, SendChatAction, SendMessage, Update};

/// Telegram Bot API client.
///
/// Clone is cheap (`Arc` inside `reqwest::Client`). Shared across tasks.
#[derive(Clone)]
pub struct TelegramClient {
    base_url: String,
    token:    String,
    client:   Client,
}

impl std::fmt::Debug for TelegramClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("telegram api error {code}: {description}")]
    Api { code: i32, description: String },
    #[error("invalid api response: ok=false without error_code")]
    InvalidResponse,
}

impl TelegramClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            client: shared_client().clone(),
        }
    }

    /// Long-poll for updates. `offset` is the `update_id + 1` of the last
    /// processed update; Telegram uses this to ack previously-delivered
    /// updates. `timeout_s` is how long Telegram holds the connection
    /// waiting for a new update (max 50 per docs).
    ///
    /// Our local HTTP timeout is `timeout_s + 5 s` to avoid racing with
    /// Telegram's own long-poll cutoff.
    pub async fn get_updates(
        &self,
        offset: i64,
        timeout_s: u32,
    ) -> Result<Vec<Update>, TelegramError> {
        #[derive(Serialize)]
        struct Req {
            offset:  i64,
            timeout: u32,
            /// Restrict to "message" updates for 6a — ignore edits, polls, etc.
            allowed_updates: [&'static str; 1],
        }
        let body = Req {
            offset,
            timeout: timeout_s,
            allowed_updates: ["message"],
        };

        let local_timeout = Duration::from_secs(timeout_s as u64 + 5);
        trace!(offset, timeout_s, "getUpdates");
        self.post_json("getUpdates", &body, local_timeout).await
    }

    /// Send a plain text message. Returns the `message_id` of the sent
    /// message so the caller can edit it later (streaming path).
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_to: Option<i64>,
    ) -> Result<i64, TelegramError> {
        let body = SendMessage {
            chat_id,
            text,
            reply_to_message_id: reply_to,
            disable_web_page_preview: true,
        };
        let resp: serde_json::Value = self
            .post_json("sendMessage", &body, Duration::from_secs(10))
            .await?;
        let message_id = resp.get("message_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        debug!(chat_id, text_len = text.len(), message_id, "sendMessage ok");
        Ok(message_id)
    }

    /// Edit a previously-sent message. Used by the streaming consumer to
    /// update the displayed reply as Ollama emits tokens.
    ///
    /// Telegram silently ignores `editMessageText` when the new text is
    /// **identical** to the existing text; that's fine — we just treat
    /// "400 Bad Request: message is not modified" as success.
    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
    ) -> Result<(), TelegramError> {
        #[derive(serde::Serialize)]
        struct Req<'a> {
            chat_id: i64,
            message_id: i64,
            text: &'a str,
            disable_web_page_preview: bool,
        }
        let body = Req {
            chat_id,
            message_id,
            text,
            disable_web_page_preview: true,
        };
        let res: Result<serde_json::Value, _> = self
            .post_json("editMessageText", &body, Duration::from_secs(10))
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(TelegramError::Api { code: 400, description }) if description.contains("not modified") => {
                // No-op: Telegram rejects edits where text hasn't changed
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// "typing…" indicator — keeps the UI alive during slow orchestrator work.
    /// Telegram shows this for ~5 s then clears; repeat if needed.
    pub async fn send_chat_action(
        &self,
        chat_id: i64,
        action: &str,
    ) -> Result<(), TelegramError> {
        let body = SendChatAction { chat_id, action };
        let _: serde_json::Value = self
            .post_json("sendChatAction", &body, Duration::from_secs(5))
            .await?;
        Ok(())
    }

    async fn post_json<Req, Resp>(
        &self,
        method: &str,
        body: &Req,
        timeout: Duration,
    ) -> Result<Resp, TelegramError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let url = format!("{}/bot{}/{}", self.base_url, self.token, method);
        let resp = self
            .client
            .post(&url)
            .timeout(timeout)
            .json(body)
            .send()
            .await?;

        let status = resp.status();
        let bytes = resp.bytes().await?;

        let api: ApiResponse<Resp> = serde_json::from_slice(&bytes).map_err(|e| {
            // Include the status in the error since JSON parsing failed
            TelegramError::Api {
                code: status.as_u16() as i32,
                description: format!("JSON parse failed: {e}"),
            }
        })?;

        if !api.ok {
            return Err(TelegramError::Api {
                code: api.error_code.unwrap_or(status.as_u16() as i32),
                description: api.description.unwrap_or_else(|| "unknown".into()),
            });
        }
        api.result.ok_or(TelegramError::InvalidResponse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    async fn mock_server() -> MockServer {
        MockServer::start().await
    }

    #[tokio::test]
    async fn get_updates_happy_path() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path("/botTEST_TOKEN/getUpdates"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": [{
                    "update_id": 1,
                    "message": {
                        "message_id": 1,
                        "chat": {"id": 42, "type": "private"},
                        "date": 1700000000,
                        "text": "hi"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
        let updates = client.get_updates(0, 1).await.unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].update_id, 1);
    }

    #[tokio::test]
    async fn api_error_surfaces_as_typed_error() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path("/botTEST_TOKEN/sendMessage"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 400,
                "description": "Bad Request: chat not found"
            })))
            .mount(&server)
            .await;

        let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
        let err = client.send_message(999, "hello", None).await.unwrap_err();
        match err {
            TelegramError::Api { code, description } => {
                assert_eq!(code, 400);
                assert!(description.contains("chat not found"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_updates_sends_expected_body() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path("/botTEST_TOKEN/getUpdates"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "offset": 42,
                "timeout": 25,
                "allowed_updates": ["message"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": []
            })))
            .mount(&server)
            .await;

        let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
        let updates = client.get_updates(42, 25).await.unwrap();
        assert!(updates.is_empty());
    }

    #[tokio::test]
    async fn send_message_omits_reply_to_when_none() {
        let server = mock_server().await;
        Mock::given(method("POST"))
            .and(path("/botTEST_TOKEN/sendMessage"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "chat_id": 42,
                "text": "hello",
                "disable_web_page_preview": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {"message_id": 1, "chat": {"id": 42, "type": "private"}, "date": 1}
            })))
            .mount(&server)
            .await;

        let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
        client.send_message(42, "hello", None).await.unwrap();
    }

    #[tokio::test]
    async fn debug_does_not_leak_token() {
        let client = TelegramClient::new("https://example.com", "SECRET_BOT_TOKEN");
        let debug = format!("{:?}", client);
        assert!(!debug.contains("SECRET_BOT_TOKEN"));
        assert!(debug.contains("redacted"));
    }
}
