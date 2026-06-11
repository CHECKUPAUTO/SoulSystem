//! Telegram Bot API types — enhanced with rich message support.
//!
//! Supports: text, photos, documents, inline keyboards, callback queries,
//! message editing, and HTML parse mode.

use serde::{Deserialize, Serialize};

/// Top-level envelope for every Bot API response.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
    pub error_code: Option<i32>,
}

/// One incoming event from `getUpdates`.
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub edited_message: Option<Message>,
    #[serde(default)]
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub chat: Chat,
    pub date: i64,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub from: Option<User>,
    #[serde(default)]
    pub photo: Option<Vec<PhotoSize>>,
    #[serde(default)]
    pub document: Option<Document>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub inline_message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PhotoSize {
    pub file_id: String,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Document {
    pub file_id: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub file_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_inline_query: Option<String>,
}

/// Parse mode for message text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParseMode {
    #[serde(rename = "HTML")]
    Html,
    #[serde(rename = "MarkdownV2")]
    MarkdownV2,
}

// ── Outgoing message types ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SendMessage<'a> {
    pub chat_id: i64,
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<ParseMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<&'a InlineKeyboardMarkup>,
    pub disable_web_page_preview: bool,
}

#[derive(Debug, Serialize)]
pub struct SendPhoto<'a> {
    pub chat_id: i64,
    pub photo: &'a str, // file_id or URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<ParseMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<&'a InlineKeyboardMarkup>,
}

#[derive(Debug, Serialize)]
pub struct SendDocument<'a> {
    pub chat_id: i64,
    pub document: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<ParseMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<&'a InlineKeyboardMarkup>,
}

#[derive(Debug, Serialize)]
pub struct EditMessageText<'a> {
    pub chat_id: i64,
    pub message_id: i64,
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<ParseMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<&'a InlineKeyboardMarkup>,
}

#[derive(Debug, Serialize)]
pub struct AnswerCallbackQuery<'a> {
    pub callback_query_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<&'a str>,
    #[serde(default)]
    pub show_alert: bool,
}

#[derive(Debug, Serialize)]
pub struct SendChatAction<'a> {
    pub chat_id: i64,
    pub action: &'a str,
}

// ── Helpers ──────────────────────────────────────────────────────────────

impl InlineKeyboardMarkup {
    /// Create a single-row keyboard with one button.
    pub fn single(text: &str, callback_data: &str) -> Self {
        Self {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                text: text.to_string(),
                url: None,
                callback_data: Some(callback_data.to_string()),
                switch_inline_query: None,
            }]],
        }
    }

    /// Create a multi-row keyboard from a list of button rows.
    pub fn new(rows: Vec<Vec<InlineKeyboardButton>>) -> Self {
        Self {
            inline_keyboard: rows,
        }
    }
}

impl InlineKeyboardButton {
    pub fn callback(text: &str, data: &str) -> Self {
        Self {
            text: text.to_string(),
            url: None,
            callback_data: Some(data.to_string()),
            switch_inline_query: None,
        }
    }

    pub fn url(text: &str, url: &str) -> Self {
        Self {
            text: text.to_string(),
            url: Some(url.to_string()),
            callback_data: None,
            switch_inline_query: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message_with_photo() {
        let raw = r#"{
            "message_id": 1,
            "chat": {"id": 42, "type": "private"},
            "date": 1700000000,
            "text": "see photo",
            "photo": [{"file_id": "abc123", "width": 800, "height": 600}]
        }"#;
        let msg: Message = serde_json::from_str(raw).unwrap();
        assert!(msg.photo.is_some());
        assert_eq!(msg.photo.unwrap()[0].file_id, "abc123");
    }

    #[test]
    fn parse_callback_query() {
        let raw = r#"{
            "id": "cb42",
            "from": {"id": 1, "is_bot": false},
            "data": "btn_click"
        }"#;
        let cb: CallbackQuery = serde_json::from_str(raw).unwrap();
        assert_eq!(cb.id, "cb42");
        assert_eq!(cb.data.as_deref(), Some("btn_click"));
    }

    #[test]
    fn inline_keyboard_markup_serializes() {
        let kb = InlineKeyboardMarkup::single("Click", "click_data");
        let json = serde_json::to_string(&kb).unwrap();
        assert!(json.contains("\"callback_data\":\"click_data\""));
        assert!(json.contains("\"text\":\"Click\""));
    }

    #[test]
    fn send_photo_omits_optional() {
        let sp = SendPhoto {
            chat_id: 42,
            photo: "file123",
            caption: None,
            parse_mode: None,
            reply_markup: None,
        };
        let json = serde_json::to_string(&sp).unwrap();
        assert!(!json.contains("caption"), "got: {json}");
    }

    #[test]
    fn edit_message_text_includes_message_id() {
        let em = EditMessageText {
            chat_id: 42,
            message_id: 7,
            text: "updated",
            parse_mode: None,
            reply_markup: None,
        };
        let json = serde_json::to_string(&em).unwrap();
        assert!(json.contains("\"message_id\":7"));
        assert!(json.contains("\"text\":\"updated\""));
    }

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
        assert_eq!(
            msg.from.as_ref().unwrap().first_name.as_deref(),
            Some("Alice")
        );
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
            parse_mode: None,
            reply_markup: None,
            disable_web_page_preview: true,
        };
        let json = serde_json::to_string(&sm).unwrap();
        assert!(!json.contains("reply_to_message_id"), "got: {json}");
        assert!(json.contains("\"chat_id\":42"));
        assert!(json.contains("\"text\":\"hello\""));
    }
}
