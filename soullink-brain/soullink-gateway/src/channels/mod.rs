//! Messaging channel implementations.
//!
//! Each channel implements a common trait for message send/receive.
//! Currently:
//! - Telegram (full client in `src/telegram/`)
//! - Signal (via signal-cli REST API)
//! - WhatsApp (WhatsApp Business Cloud API — send, inbound webhook parsing,
//!   subscription handshake, and HMAC signature verification)
//! - Slack (Web API send + Events API inbound, URL-verification handshake, and
//!   request-signature verification)

pub mod signal;
pub mod slack;
pub mod whatsapp;
