//! Messaging channel implementations.
//!
//! Each channel implements a common trait for message send/receive.
//! Currently:
//! - Telegram (full client in `src/telegram/`)
//! - Signal (via signal-cli REST API)
//! - WhatsApp (WhatsApp Business Cloud API — send, inbound webhook parsing,
//!   subscription handshake, and HMAC signature verification)

pub mod signal;
pub mod whatsapp;
