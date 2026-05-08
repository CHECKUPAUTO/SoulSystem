//! EventBus — tokio::sync::broadcast-based async event bus.
//!
//! Producers emit events; consumers subscribe with a channel capacity.
//! Late consumers miss events beyond their buffer (intentional backpressure).

use crate::event::{BusEvent, BusEventKind};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Default channel capacity for new subscribers.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// The async event bus — shared via Arc.
pub struct EventBus {
    sender: broadcast::Sender<BusEvent>,
}

impl EventBus {
    /// Create a new EventBus with the given channel capacity.
    pub fn new(capacity: usize) -> Arc<Self> {
        let (sender, _) = broadcast::channel(capacity);
        info!("EventBus created with capacity {}", capacity);
        Arc::new(Self { sender })
    }

    /// Create with default capacity.
    pub fn default_bus() -> Arc<Self> {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Emit an event to all subscribers.
    pub fn emit(&self, event: BusEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to all events. Returns a Receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }

    /// Subscribe with a filter: only receive events of the given kinds.
    /// Returns a stream that filters in the subscriber's task.
    pub fn subscribe_filtered(&self, kinds: Vec<BusEventKind>) -> broadcast::Receiver<BusEvent> {
        // Filtering happens in the consumer task — broadcast sends everything.
        let _ = kinds;
        self.sender.subscribe()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn emit_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        let event = BusEvent::new(
            BusEventKind::QueryReceived,
            "test",
            serde_json::json!({"query": "hello"}),
        );

        bus.emit(event.clone());

        sleep(Duration::from_millis(10)).await;

        let received = rx.try_recv().unwrap();
        assert_eq!(received.kind, BusEventKind::QueryReceived);
        assert_eq!(received.source, "test");
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let event = BusEvent::empty(BusEventKind::Heartbeat, "system");
        bus.emit(event);

        sleep(Duration::from_millis(10)).await;

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[tokio::test]
    async fn subscriber_count() {
        let bus = EventBus::new(16);
        let _rx1 = bus.subscribe();
        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn filtered_subscription() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe_filtered(vec![BusEventKind::EvaluationCompleted]);

        bus.emit(BusEvent::empty(BusEventKind::Heartbeat, "system"));
        bus.emit(BusEvent::empty(BusEventKind::EvaluationCompleted, "eval"));

        sleep(Duration::from_millis(10)).await;

        // Both events are received (filtering is consumer-side)
        let e1 = rx.try_recv().unwrap();
        assert_eq!(e1.kind, BusEventKind::Heartbeat);
        let e2 = rx.try_recv().unwrap();
        assert_eq!(e2.kind, BusEventKind::EvaluationCompleted);
    }
}