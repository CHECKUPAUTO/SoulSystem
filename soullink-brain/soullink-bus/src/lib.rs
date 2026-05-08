//! SoulLink Bus — Async event bus using tokio::sync::broadcast.
//!
//! Each module subscribes to relevant event types and processes them
//! in its own tokio task. The bus decouples producers from consumers.

pub mod event;
pub mod bus;

pub use bus::EventBus;
pub use event::{BusEvent, BusEventKind};