//! Minimal Telegram Bot API types.
//!
//! We deliberately model **only** the fields we consume. Anything else is
//! ignored by `serde(deny_unknown_fields = false)` defaults. Telegram adds
//! new fields regularly; that's fine, we don't break when they do.

use serde::{Deserialize, Serialize};

/// Top-level envelope for every Bot API response.
///
/// `Option<T>` fields are intentionally NOT marked `#[serde(default)]`:
/// serde_json treats an absent `Option` field as `None` by default, and
/// adding `#[serde(default)]` on a generic `Option<T>` incorrectly
/// propagates a `Default` requirement up to `T`. `serde_json::Value` has
/// `Default`, so earlier versions of this code compiled; `Vec<Update>`
/// also does, by accident. But a caller asking for `ApiResponse<SomeType>`
/// where `SomeType: DeserializeOwned` but `!Default` would fail to compile.
/// See the 2026-04-18 fix note in DELIVERY.md.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub ok:          bool,
    pub result:      Option<T>,
    pub description: Option<String>,
    pub error_code:  Option<i32>,
}

/// One incoming event. Telegram delivers these via `getUpdates` (long-poll).
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message:   Option<Message>,
    // edited_message, channel_post, etc. ignored in 6a — only plain new messages.
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub chat:       Chat,
    /// Wall-clock Unix timestamp of the message as reported by Telegram.
    pub date:       i64,
    #[serde(default)]
    pub text:       Option<String>,
    #[serde(default)]
    pub from:       Option<User>,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id:        i64,
    /// "private" | "group" | "supergroup" | "channel"
    #[serde(rename = "type")]
    pub chat_type: String,
    #[serde(default)]
    pub title:     Option<String>,
    #[serde(default)]
    pub username:  Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id:         i64,
    #[serde(default)]
    pub is_bot:     bool,
    #[serde(default)]
    pub username:   Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
}

// ─── Outgoing ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SendMessage<'a> {
    pub chat_id: i64,
    pub text:    &'a str,
    /// Phase 6a: always omit parse_mode (plain text only). Markdown/HTML
    /// parsing opens a small injection surface if we ever echo back user
    /// input — we'll wire it explicitly in 6b once we control what goes
    /// into replies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    /// Disable link previews by default — orchestrator answers are prose,
    /// link previews just add visual noise. (Always `true` when we
    /// construct `SendMessage` in-code; the field exists so callers can
    /// override if needed.)
    pub disable_web_page_preview: bool,
}

#[derive(Debug, Serialize)]
pub struct SendChatAction<'a> {
    pub chat_id: i64,
    /// "typing" is what we use; other values ignored in 6a.
    pub action:  &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_successful_update() {
        let raw = r#"{
            "ok": true,
            "result": [{
                "update_id": 123,
                "message": {
                    "message_id": 42,
                    "chat": {"id": -100, "type": "supergroup", "title": "test"},
                    "date": 1700000000,
                    "text": "hello bot",
                    "from": {"id": 999, "is_bot": false, "first_name": "Alice"}
                }
            }]
        }"#;
        let resp: ApiResponse<Vec<Update>> = serde_json::from_str(raw).unwrap();
        assert!(resp.ok);
        let updates = resp.result.unwrap();
        assert_eq!(updates.len(), 1);
        let msg = updates[0].message.as_ref().unwrap();
        assert_eq!(msg.chat.id, -100);
        assert_eq!(msg.text.as_deref(), Some("hello bot"));
        assert_eq!(msg.from.as_ref().unwrap().first_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn parse_error_response() {
        let raw = r#"{
            "ok": false,
            "error_code": 401,
            "description": "Unauthorized"
        }"#;
        let resp: ApiResponse<Vec<Update>> = serde_json::from_str(raw).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error_code, Some(401));
        assert_eq!(resp.description.as_deref(), Some("Unauthorized"));
    }

    #[test]
    fn parse_update_without_message() {
        // Telegram sends edited_message, poll_answer, etc. which we don't
        // handle in 6a. Must parse successfully with message = None.
        let raw = r#"{
            "ok": true,
            "result": [{
                "update_id": 555,
                "edited_message": {"message_id": 1, "chat": {"id": 1, "type": "private"}, "date": 1}
            }]
        }"#;
        let resp: ApiResponse<Vec<Update>> = serde_json::from_str(raw).unwrap();
        assert!(resp.ok);
        let updates = resp.result.unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].message.is_none());
    }

    #[test]
    fn parse_message_without_text() {
        // Photos, stickers, etc. arrive without text field.
        let raw = r#"{
            "message_id": 7,
            "chat": {"id": 42, "type": "private"},
            "date": 1700000000
        }"#;
        let msg: Message = serde_json::from_str(raw).unwrap();
        assert!(msg.text.is_none());
    }

    #[test]
    fn unknown_fields_ignored() {
        // Telegram adds fields over time — we must not break.
        let raw = r#"{
            "message_id": 1,
            "chat": {"id": 1, "type": "private", "some_new_field": "x"},
            "date": 1,
            "text": "hi",
            "entirely_new_future_field": {"nested": true}
        }"#;
        let msg: Message = serde_json::from_str(raw).unwrap();
        assert_eq!(msg.text.as_deref(), Some("hi"));
    }

    #[test]
    fn send_message_serializes_without_optional_fields() {
        let sm = SendMessage {
            chat_id: 42,
            text: "hello",
            reply_to_message_id: None,
            disable_web_page_preview: true,
        };
        let json = serde_json::to_string(&sm).unwrap();
        // reply_to_message_id MUST be absent (skip_serializing_if)
        assert!(!json.contains("reply_to_message_id"), "got: {json}");
        assert!(json.contains("\"chat_id\":42"));
        assert!(json.contains("\"text\":\"hello\""));
    }
}
