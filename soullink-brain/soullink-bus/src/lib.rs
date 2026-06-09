//! SoulLink Bus — Wrapper backward-compatible autour du bus unifié.
//!
//! Délègue à `bus` (crate racine) pour la messagerie.
//! Les types `BusEvent`, `BusEventKind`, `EventBus` sont re-exportés.

pub use bus::{BusEventKind, Message};

/// Alias pour compatibilité : `BusEvent` = `Message`.
pub type BusEvent = Message;

/// Wrapper EventBus qui délègue au Bus central.
pub struct EventBus {
    inner: bus::Bus,
}

impl EventBus {
    pub fn new(capacity: usize) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            inner: bus::Bus::new(capacity),
        })
    }

    pub fn default_bus() -> std::sync::Arc<Self> {
        Self::new(256)
    }

    pub fn emit(&self, event: BusEvent) {
        self.inner.publish(event);
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<BusEvent> {
        self.inner.subscribe()
    }

    pub fn subscribe_filtered(&self, _kinds: Vec<BusEventKind>) -> tokio::sync::broadcast::Receiver<BusEvent> {
        self.inner.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.inner.subscriber_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        let event = BusEvent::event(
            BusEventKind::QueryReceived,
            "test",
            serde_json::json!({"query": "hello"}),
        );

        bus.emit(event);

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let received = rx.try_recv().unwrap();
        match received {
            Message::Event { kind, source, .. } => {
                assert_eq!(kind, BusEventKind::QueryReceived);
                assert_eq!(source, "test");
            }
            _ => panic!("expected Event"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(BusEvent::heartbeat("system"));

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}
