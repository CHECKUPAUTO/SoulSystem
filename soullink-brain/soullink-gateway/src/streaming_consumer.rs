//! SSE streaming consumer — Phase 6b.
//!
//! Given an `IncomingMessage`, open an SSE connection to the
//! orchestrator's `/api/mesh/stream`, parse `meta`/`token`/`done`/`error`
//! frames, and edit a single Telegram message progressively as tokens
//! arrive.
//!
//! # Edit rate limiting
//!
//! Telegram's documented limit is 1 edit per second per chat for
//! `editMessageText`. We enforce this with a simple "last edit
//! timestamp" bucket: if less than `MIN_EDIT_INTERVAL` has elapsed since
//! the last edit, we accumulate the new token into a pending buffer and
//! don't hit the wire. A final edit always goes out at end-of-stream to
//! guarantee the user sees the complete reply even if the bucket
//! swallowed the last chunks.
//!
//! # Empty-body edge case
//!
//! Telegram refuses `editMessageText` with empty text ("400: text is
//! empty"). We therefore post the initial message body with a single
//! zero-width character (`·`) as placeholder so we always have something
//! to edit. The placeholder is replaced by the first real token.

use std::time::{Duration, Instant};

use futures::StreamExt;
use tracing::{debug, info, warn};

use crate::telegram::client::TelegramClient;
use crate::telegram::long_poll::IncomingMessage;

/// Minimum wall-clock gap between Telegram `editMessageText` calls per chat.
/// Telegram's published limit is 1/sec/chat; we leave a small margin.
pub const MIN_EDIT_INTERVAL: Duration = Duration::from_millis(1100);

/// Maximum characters per Telegram message.
const TELEGRAM_MAX_BYTES: usize = 4096;

/// Placeholder text for the initial message before the first token arrives.
/// Telegram rejects empty edits, so we need something non-empty to start.
const INITIAL_PLACEHOLDER: &str = "·";

pub struct StreamingConsumer {
    tg_client:        TelegramClient,
    orchestrator_url: String,
    orch_http:        reqwest::Client,
}

impl StreamingConsumer {
    pub fn new(tg_client: TelegramClient, orchestrator_url: String) -> Self {
        // Dedicated HTTP client for SSE: no global request timeout (streams
        // can legitimately last a minute+), generous connect timeout.
        let orch_http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .expect("reqwest client build for streaming");
        Self { tg_client, orchestrator_url, orch_http }
    }

    /// Consume the SSE stream for one incoming message. Returns Ok when
    /// the full reply has been delivered (or fallback emitted). Returns
    /// Err only on catastrophic failures the long-poll loop should log —
    /// any user-facing error is already posted to the chat as a fallback
    /// message.
    pub async fn handle(&self, incoming: IncomingMessage) -> Result<(), String> {
        let url = format!(
            "{}/api/mesh/stream",
            self.orchestrator_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "query":   incoming.text,
            "source":  "telegram",
            "chat_id": incoming.chat_id,
            "user":    incoming.username,
        });

        let resp = self.orch_http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("orchestrator stream request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("orchestrator stream HTTP {}", resp.status()));
        }

        // Post the initial placeholder message so we have a message_id to edit
        let message_id = self.tg_client
            .send_message(incoming.chat_id, INITIAL_PLACEHOLDER, Some(incoming.message_id))
            .await
            .map_err(|e| format!("initial sendMessage failed: {e}"))?;

        debug!(chat_id = incoming.chat_id, message_id, "streaming: initial message posted");

        // Consume SSE and edit progressively
        let byte_stream = resp.bytes_stream();
        let events = parse_sse_events(byte_stream);

        let mut accumulated = String::new();
        let mut last_edit_at: Option<Instant> = None;
        let mut any_token = false;
        let mut had_error: Option<String> = None;

        tokio::pin!(events);
        while let Some(ev) = events.next().await {
            match ev {
                SseEvent::Meta(_) => {
                    // We could stash mesh telemetry for logging, but for UX
                    // the user doesn't see meta — they see tokens.
                }
                SseEvent::Token(chunk) => {
                    accumulated.push_str(&chunk);
                    any_token = true;

                    if should_edit_now(last_edit_at) {
                        let to_show = render_progress(&accumulated, /* final */ false);
                        if let Err(e) = self.tg_client.edit_message_text(
                            incoming.chat_id, message_id, &to_show,
                        ).await {
                            warn!(%e, "streaming: edit failed mid-stream (continuing)");
                        } else {
                            last_edit_at = Some(Instant::now());
                        }
                    }
                }
                SseEvent::Done(kind) => {
                    debug!(kind, "streaming: done");
                    break;
                }
                SseEvent::ErrorFrame(msg) => {
                    had_error = Some(msg);
                    break;
                }
                SseEvent::Unknown(name) => {
                    debug!(event = %name, "streaming: unknown SSE event");
                }
            }
        }

        // Final edit: always post the complete accumulated text (or error note)
        let final_text = if let Some(err_msg) = had_error {
            if any_token {
                format!("{}\n\n⚠️ Stream interrupted: {}", accumulated.trim(), err_msg)
            } else {
                format!("⚠️ Error from orchestrator: {}", err_msg)
            }
        } else if accumulated.trim().is_empty() {
            "⚠️ No reply generated.".to_string()
        } else {
            render_progress(&accumulated, /* final */ true)
        };

        // Respect Telegram's max length (4096 bytes, UTF-8 safe)
        let final_truncated = truncate_utf8_bytes(&final_text, TELEGRAM_MAX_BYTES);

        // Wait out the bucket if needed so the final edit actually lands
        if let Some(last) = last_edit_at {
            let since = Instant::now().duration_since(last);
            if since < MIN_EDIT_INTERVAL {
                tokio::time::sleep(MIN_EDIT_INTERVAL - since).await;
            }
        }

        if let Err(e) = self.tg_client.edit_message_text(
            incoming.chat_id, message_id, &final_truncated,
        ).await {
            warn!(%e, "streaming: final edit failed");
            return Err(format!("final edit failed: {e}"));
        }

        info!(
            chat_id = incoming.chat_id, message_id,
            total_len = accumulated.len(),
            "streaming: reply delivered"
        );
        Ok(())
    }
}

/// Internal: decide whether the next token should trigger an edit now.
fn should_edit_now(last_edit_at: Option<Instant>) -> bool {
    match last_edit_at {
        None => true, // first token: edit immediately to replace placeholder
        Some(last) => Instant::now().duration_since(last) >= MIN_EDIT_INTERVAL,
    }
}

/// Internal: format in-progress accumulation. Adds a trailing "…" when
/// not final so the user can see generation is ongoing.
fn render_progress(accumulated: &str, is_final: bool) -> String {
    let trimmed = accumulated.trim();
    if is_final {
        trimmed.to_string()
    } else {
        format!("{trimmed} …")
    }
}

/// SSE event after parsing.
#[derive(Debug, PartialEq)]
enum SseEvent {
    Meta(String),        // raw JSON string
    Token(String),
    Done(String),
    ErrorFrame(String),
    Unknown(String),
}

/// Parse a raw byte stream (from `response.bytes_stream()`) into SSE events.
/// Handles "event:" + "data:" fields per spec, ignores ":" comments
/// (used for keep-alive).
fn parse_sse_events(
    byte_stream: impl futures::Stream<Item = Result<impl AsRef<[u8]>, reqwest::Error>> + Send + 'static,
) -> impl futures::Stream<Item = SseEvent> + Send {
    byte_stream.scan(
        SseParserState::default(),
        |state: &mut SseParserState, chunk_res| {
            let events = match chunk_res {
                Ok(bytes) => state.feed(bytes.as_ref()),
                Err(_)    => vec![],
            };
            futures::future::ready(Some(events))
        },
    )
    .flat_map(|batch| futures::stream::iter(batch.into_iter()))
}

#[derive(Default)]
struct SseParserState {
    buf: Vec<u8>,
    current_event: Option<String>,
    current_data:  Vec<String>,
}

impl SseParserState {
    fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut emitted = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let raw_line: Vec<u8> = self.buf.drain(..=pos).collect();
            let line_str = String::from_utf8_lossy(&raw_line[..raw_line.len()-1]);
            let line = line_str.trim_end_matches('\r');

            if line.is_empty() {
                // Dispatch the pending event
                if self.current_event.is_some() || !self.current_data.is_empty() {
                    let event_name = self.current_event.take().unwrap_or_else(|| "message".into());
                    let data = self.current_data.join("\n");
                    self.current_data.clear();

                    let ev = match event_name.as_str() {
                        "meta"  => SseEvent::Meta(data),
                        "token" => SseEvent::Token(data),
                        "done"  => SseEvent::Done(data),
                        "error" => SseEvent::ErrorFrame(data),
                        other   => SseEvent::Unknown(other.to_string()),
                    };
                    emitted.push(ev);
                }
                continue;
            }
            if line.starts_with(':') {
                // SSE comment — keep-alive, ignore
                continue;
            }
            if let Some(rest) = line.strip_prefix("event:") {
                self.current_event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                let v = rest.trim_start();
                self.current_data.push(v.to_string());
            }
            // Other fields (id:, retry:) ignored
        }
        emitted
    }
}

/// UTF-8-safe byte truncation. Same as long_poll.rs. Duplicated here to
/// keep streaming_consumer self-contained.
fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes { return s.to_string(); }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_first_edit_always_allowed() {
        assert!(should_edit_now(None));
    }

    #[test]
    fn bucket_blocks_within_interval() {
        let just_now = Instant::now();
        assert!(!should_edit_now(Some(just_now)));
    }

    #[test]
    fn bucket_allows_after_interval() {
        let long_ago = Instant::now().checked_sub(Duration::from_secs(5)).unwrap();
        assert!(should_edit_now(Some(long_ago)));
    }

    #[test]
    fn render_progress_adds_ellipsis_only_when_streaming() {
        assert_eq!(render_progress("hello", false), "hello …");
        assert_eq!(render_progress("hello", true), "hello");
        assert_eq!(render_progress("  hello  ", true), "hello");
    }

    #[test]
    fn sse_parser_standard_flow() {
        let mut state = SseParserState::default();
        let input = b"event: meta\ndata: {\"brains\":3}\n\nevent: token\ndata: hello\n\nevent: token\ndata:  world\n\nevent: done\ndata: ok\n\n";
        let events = state.feed(input);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0], SseEvent::Meta("{\"brains\":3}".to_string()));
        assert_eq!(events[1], SseEvent::Token("hello".to_string()));
        // Leading space after "data:" is stripped per spec
        assert_eq!(events[2], SseEvent::Token("world".to_string()));
        assert_eq!(events[3], SseEvent::Done("ok".to_string()));
    }

    #[test]
    fn sse_parser_chunked_across_byte_boundaries() {
        let mut state = SseParserState::default();
        // Split the input across two arbitrary boundaries
        let ev1 = state.feed(b"event: to");
        assert_eq!(ev1.len(), 0, "no complete event yet");
        let ev2 = state.feed(b"ken\ndata: hel");
        assert_eq!(ev2.len(), 0, "still no blank-line terminator");
        let ev3 = state.feed(b"lo\n\n");
        assert_eq!(ev3.len(), 1);
        assert_eq!(ev3[0], SseEvent::Token("hello".to_string()));
    }

    #[test]
    fn sse_parser_ignores_comments_and_keepalive() {
        let mut state = SseParserState::default();
        let input = b": keep-alive\n\nevent: token\ndata: hi\n\n";
        let events = state.feed(input);
        // Comment-only "event" should NOT dispatch (no event/data set between
        // the first blank line and the next). One event total: "token: hi".
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], SseEvent::Token("hi".to_string()));
    }

    #[test]
    fn sse_parser_multi_line_data() {
        // Per SSE spec, multiple data: lines in one event are joined with \n
        let mut state = SseParserState::default();
        let input = b"event: token\ndata: line1\ndata: line2\n\n";
        let events = state.feed(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], SseEvent::Token("line1\nline2".to_string()));
    }

    #[test]
    fn sse_parser_unknown_event_type_preserved() {
        let mut state = SseParserState::default();
        let input = b"event: heartbeat\ndata: ping\n\n";
        let events = state.feed(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], SseEvent::Unknown("heartbeat".to_string()));
    }
}
