//! SoulLink Bus — Async event bus using tokio::sync::broadcast.
//!
//! Each module subscribes to relevant event types and processes them
//! in its own tokio task. The bus decouples producers from consumers.

pub mod bus;
pub mod event;

pub use bus::EventBus;
pub use event::{BusEvent, BusEventKind};
