//! Messaging channel implementations.
//!
//! Each channel implements a common trait for message send/receive.
//! Currently:
//! - Telegram (full client in `src/telegram/`)
//! - Signal (via signal-cli REST API)
//! - WhatsApp (stub)

pub mod signal;
pub mod whatsapp;
