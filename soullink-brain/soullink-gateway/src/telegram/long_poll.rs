//! Telegram long-poll loop.
//!
//! Reads updates with `getUpdates`, routes text messages through an
//! `OrchestratorBridge` to get a reply, and posts the reply back.
//!
//! Design points:
//!
//! * **No streaming in 6a** — the orchestrator reply is awaited in full,
//!   then a single `sendMessage` goes out. Streaming/edits come in 6b.
//! * **Allow-list enforcement** — if `allowed_chat_ids` is set, messages
//!   from other chats are silently ignored (no 403, no bot reply). Avoids
//!   leaking that the bot exists in private chats.
//! * **Per-update error isolation** — a failure on one message never kills
//!   the loop. The update_id is still advanced so we don't reprocess a bad
//!   message in an infinite loop.
//! * **Backoff on API errors** — on repeated `getUpdates` failures (429 rate
//!   limit, network drops), exponential backoff capped at 30 s.

use std::{collections::HashSet, sync::Arc, time::Duration};

use futures::future::BoxFuture;
use tokio::{sync::broadcast, time};
use tracing::{debug, error, info, warn};

use super::{
    client::TelegramClient,
    types::{Message, Update},
};

/// Pluggable orchestrator call. The real implementation POSTs to the
/// orchestrator; tests inject a closure that returns canned replies.
pub type OrchestratorBridge =
    Arc<dyn Fn(IncomingMessage) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

/// What we pass to the orchestrator. Keeps the bridge trait cheap to swap.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub chat_id: i64,
    pub chat_type: String,
    pub message_id: i64,
    pub text: String,
    pub user_id: Option<i64>,
    pub username: Option<String>,
}

pub struct LongPollLoop {
    client: TelegramClient,
    bridge: OrchestratorBridge,
    long_poll_timeout_s: u32,
    allow_list: Option<HashSet<i64>>,
    typing_pre_reply: Option<Duration>,
    /// Phase 6b: when `Some`, use SSE streaming instead of blocking bridge.
    streaming: Option<Arc<crate::streaming_consumer::StreamingConsumer>>,
    shutdown: broadcast::Receiver<()>,
    /// Internal: next `offset` to pass to `getUpdates`.
    next_offset: i64,
}

impl LongPollLoop {
    pub fn new(
        client: TelegramClient,
        bridge: OrchestratorBridge,
        long_poll_timeout_s: u32,
        allowed_chat_ids: Option<Vec<i64>>,
        typing_pre_reply: Option<Duration>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            client,
            bridge,
            long_poll_timeout_s,
            allow_list: allowed_chat_ids.map(|v| v.into_iter().collect()),
            typing_pre_reply,
            streaming: None,
            shutdown,
            next_offset: 0,
        }
    }

    /// Enable Phase 6b streaming. The loop will route messages through
    /// the SSE consumer (progressive `editMessageText`) instead of the
    /// bridge (single `sendMessage` at end).
    pub fn with_streaming(mut self, sc: Arc<crate::streaming_consumer::StreamingConsumer>) -> Self {
        self.streaming = Some(sc);
        self
    }

    pub async fn run(mut self) {
        info!(
            long_poll_s = self.long_poll_timeout_s,
            allow_list_size = self.allow_list.as_ref().map(|s| s.len()).unwrap_or(0),
            "Telegram long-poll loop starting"
        );

        let mut backoff_ms: u64 = 500;

        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    info!("Telegram long-poll: shutdown");
                    return;
                }
                res = self.client.get_updates(self.next_offset, self.long_poll_timeout_s) => {
                    match res {
                        Ok(updates) => {
                            backoff_ms = 500;
                            for update in updates {
                                self.next_offset = update.update_id + 1;
                                if let Err(e) = self.handle_update(update).await {
                                    warn!(error = %e, "update handler failed (loop continues)");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, backoff_ms, "getUpdates failed; backing off");
                            tokio::select! {
                                _ = time::sleep(Duration::from_millis(backoff_ms)) => {}
                                _ = self.shutdown.recv() => return,
                            }
                            backoff_ms = (backoff_ms * 2).min(30_000);
                        }
                    }
                }
            }
        }
    }

    async fn handle_update(&self, update: Update) -> Result<(), String> {
        let msg = match update.message {
            Some(m) => m,
            None => return Ok(()), // edited_message / channel_post / etc. — ignored in 6a
        };

        // Allow-list
        if let Some(allow) = &self.allow_list {
            if !allow.contains(&msg.chat.id) {
                debug!(
                    chat_id = msg.chat.id,
                    "message from chat not in allow-list; ignored"
                );
                return Ok(());
            }
        }

        // Text-only in 6a
        let text = match msg.text.as_deref() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return Ok(()), // non-text message (photo, sticker, etc.) — ignored
        };

        let Message {
            chat,
            message_id,
            from,
            ..
        } = msg;
        let incoming = IncomingMessage {
            chat_id: chat.id,
            chat_type: chat.chat_type.clone(),
            message_id,
            text,
            user_id: from.as_ref().map(|u| u.id),
            username: from.as_ref().and_then(|u| u.username.clone()),
        };

        info!(
            chat_id = incoming.chat_id,
            message_id = incoming.message_id,
            text_len = incoming.text.len(),
            "telegram: inbound message"
        );

        // Phase 6b streaming path: if a StreamingConsumer is installed,
        // delegate to it. The consumer handles the initial message post,
        // progressive edits, final edit with full text. No typing
        // indicator needed — the message itself is the "the bot is
        // working" feedback.
        if let Some(sc) = &self.streaming {
            return sc.handle(incoming.clone()).await;
        }

        // Phase 6a blocking path (unchanged). Persistent typing indicator
        // while the orchestrator is thinking. Telegram extinguishes the
        // indicator after ~5s, so we re-issue every 4s until the bridge
        // returns. Ollama cloud replies can take 10-20s; without this the
        // user sees nothing for that long.
        let typing_task = if self.typing_pre_reply.is_some() {
            let client = self.client.clone();
            let chat_id = incoming.chat_id;
            Some(tokio::spawn(async move {
                loop {
                    let _ = client.send_chat_action(chat_id, "typing").await;
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                }
            }))
        } else {
            None
        };

        // Call orchestrator. Bridge returns the reply text (or an error string).
        let reply_result = (self.bridge)(incoming.clone()).await;

        if let Some(t) = typing_task {
            t.abort();
        }

        let reply_text = match reply_result {
            Ok(text) if text.is_empty() => {
                // Orchestrator returned success but empty reply — rare but
                // possible on some edge cases. Don't spam the chat with
                // an empty message; just acknowledge silently.
                debug!(
                    chat_id = incoming.chat_id,
                    "empty reply from orchestrator — skipping send"
                );
                return Ok(());
            }
            Ok(text) => text,
            Err(e) => {
                error!(chat_id = incoming.chat_id, error = %e, "orchestrator call failed");
                // Send a user-facing fallback. Keep it short and neutral —
                // don't leak internals.
                "⚠️ Temporary error. Please try again in a moment.".to_string()
            }
        };

        // Truncate to Telegram's 4096-char hard limit for text messages.
        // In 6b we'll split into multiple messages; in 6a we chop to avoid
        // getting the whole send rejected.
        let to_send = truncate_utf8_bytes(&reply_text, 4096);

        if let Err(e) = self
            .client
            .send_message(incoming.chat_id, &to_send, Some(incoming.message_id))
            .await
        {
            return Err(format!("sendMessage failed: {e}"));
        }

        Ok(())
    }
}

/// Truncate a string to at most `max_bytes` bytes without splitting a UTF-8
/// codepoint. Returns owned `String`.
fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find the largest char boundary <= max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate_utf8_bytes("hello", 100), "hello");
    }

    #[test]
    fn truncate_exact_boundary() {
        assert_eq!(truncate_utf8_bytes("hello", 5), "hello");
    }

    #[test]
    fn truncate_cuts_at_char_boundary() {
        // "é" is 2 bytes in UTF-8. max_bytes=3 means we can fit "hé" (3 bytes)
        // but not "hé2" (4 bytes)
        let s = "héllo";
        // "h" = 1, "é" = 2, "l" = 1 → "hé" = 3 bytes, "hél" = 4 bytes
        assert_eq!(truncate_utf8_bytes(s, 4), "hél");
        // max_bytes=2 → would split "é", must back off to "h" (1 byte)
        assert_eq!(truncate_utf8_bytes(s, 2), "h");
    }

    #[test]
    fn truncate_zero_max() {
        assert_eq!(truncate_utf8_bytes("abc", 0), "");
    }
}
