//! SoulSystem Bus — Message bus central unifié.
//!
//! Merge les deux buses historiques :
//! - `bus` original (Message enum + bincode)
//! - `soullink-bus` (BusEvent struct + BusEventKind)
//!
//! Le `Message` enum est étendu avec les variantes SoulLink.
//! Le type `BusEvent` est une alias pour `Message::Event`.

#![allow(unused_imports)]
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::info;

// ── BusEventKind (depuis soullink-bus) ────────────────────────────────────

/// Types d'événements du bus SoulLink.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BusEventKind {
    QueryReceived,
    ResponseGenerated,
    EvaluationCompleted,
    ConceptUpdated,
    EdgeWeightAdjusted,
    ActionExecuted,
    SenateCompleted,
    ReasoningEvaluated,
    RandomWalkDiscovery,
    Heartbeat,
}

// ── Message unifié ────────────────────────────────────────────────────────

/// Message unifié du bus SoulSystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    // ── Variantes historiques (bus original) ─────────────────────────────
    HnnStatus {
        organ: String,
        status: String,
        energy: f32,
    },
    SynergyDetection {
        module: String,
        description: String,
    },
    AvidDiscovery {
        topic: String,
        summary: String,
    },
    EvolveOptimization {
        generation: u32,
        best_fitness: f32,
    },

    // ── Variante SoulLink (depuis soullink-bus) ─────────────────────────
    /// Événement structuré SoulLink avec kind, source, payload, timestamp.
    Event {
        kind: BusEventKind,
        source: String,
        payload: serde_json::Value,
        timestamp: String,
    },

    // ── Variante générique ──────────────────────────────────────────────
    Custom {
        topic: String,
        payload: serde_json::Value,
    },
}

impl Message {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(data: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(data)
    }

    /// Crée un événement structuré SoulLink.
    pub fn event(kind: BusEventKind, source: impl Into<String>, payload: serde_json::Value) -> Self {
        Self::Event {
            kind,
            source: source.into(),
            payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Raccourci pour créer un événement Heartbeat.
    pub fn heartbeat(source: impl Into<String>) -> Self {
        Self::event(BusEventKind::Heartbeat, source, serde_json::Value::Null)
    }
}

// ── Bus central ───────────────────────────────────────────────────────────

pub struct Bus {
    tx: broadcast::Sender<Message>,
}

impl Bus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, msg: Message) {
        let _ = self.tx.send(msg);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.tx.subscribe()
    }

    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_roundtrip_json() {
        let msg = Message::Custom {
            topic: "test".into(),
            payload: serde_json::json!({"key": "value"}),
        };
        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::Custom { topic, .. } => assert_eq!(topic, "test"),
            _ => panic!("expected Custom"),
        }
    }

    #[test]
    fn test_event_creation() {
        let msg = Message::event(
            BusEventKind::QueryReceived,
            "gateway",
            serde_json::json!({"query": "hello"}),
        );
        match &msg {
            Message::Event { kind, source, payload, timestamp } => {
                assert_eq!(*kind, BusEventKind::QueryReceived);
                assert_eq!(source, "gateway");
                assert!(payload.get("query").is_some());
                assert!(!timestamp.is_empty());
            }
            _ => panic!("expected Event"),
        }
    }

    #[test]
    fn test_heartbeat() {
        let msg = Message::heartbeat("system");
        match &msg {
            Message::Event { kind, source, .. } => {
                assert_eq!(*kind, BusEventKind::Heartbeat);
                assert_eq!(source, "system");
            }
            _ => panic!("expected Event"),
        }
    }

    #[test]
    fn test_bus_publish_subscribe() {
        let bus = Bus::new(16);
        let mut rx = bus.subscribe();
        bus.publish(Message::heartbeat("test"));
        let received = rx.try_recv().unwrap();
        match received {
            Message::Event { kind, .. } => assert_eq!(kind, BusEventKind::Heartbeat),
            _ => panic!("expected Event"),
        }
    }

    #[test]
    fn test_bus_event_json() {
        let msg = Message::event(
            BusEventKind::EvaluationCompleted,
            "eval",
            serde_json::json!({"score": 0.95}),
        );
        let json = msg.to_json().unwrap();
        let decoded = Message::from_json(&json).unwrap();
        match decoded {
            Message::Event { kind, source, .. } => {
                assert_eq!(kind, BusEventKind::EvaluationCompleted);
                assert_eq!(source, "eval");
            }
            _ => panic!("expected Event"),
        }
    }
}

// ── Shared Memory Transport ─────────────────────────────────────────────

/// Shared memory transport for zero-copy cross-process bus communication.
///
/// Backed by `soullink_shm::ShmBus`. Messages are serialized as JSON
/// into shared memory ring buffers, avoiding TCP/HTTP overhead.
///
/// Requires the `shm` feature flag.
#[cfg(feature = "shm")]
pub mod shm_transport {
    use super::{Bus, Message};
    use anyhow::Result;
    use soullink_shm::bus::{ShmBus, DEFAULT_SLOT_SIZE, MAX_SUBSCRIBERS};
    use soullink_shm::shm::ShmRegion;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    /// A bus that uses shared memory for cross-process communication.
    pub struct ShmBusTransport {
        inner: Arc<ShmBus>,
        /// Local broadcast for in-process subscribers (same API as Bus).
        tx: broadcast::Sender<Message>,
        subscriber_slots: std::sync::Mutex<Vec<usize>>,
    }

    impl ShmBusTransport {
        /// Create a new shared memory bus.
        ///
        /// `max_slots` is the number of cross-process subscriber slots.
        /// `slot_size` is the ring buffer size per slot in bytes.
        pub fn new(max_slots: usize, slot_size: usize) -> Result<Self> {
            let inner = Arc::new(ShmBus::create(max_slots, slot_size)?);
            let (tx, _) = broadcast::channel(256);

            Ok(Self {
                inner,
                tx,
                subscriber_slots: std::sync::Mutex::new(Vec::new()),
            })
        }

        /// Open an existing shared memory bus from a received fd.
        ///
        /// # Safety
        /// The fd must be a valid shared memory region created by `ShmBus::create`.
        pub unsafe fn open(fd: std::os::unix::io::RawFd, total_size: usize) -> Result<Self> {
            use std::os::unix::io::FromRawFd;
            let owned_fd = std::os::unix::io::OwnedFd::from_raw_fd(fd);
            let inner = Arc::new(ShmBus::open(owned_fd, total_size)?);
            let (tx, _) = broadcast::channel(256);

            Ok(Self {
                inner,
                tx,
                subscriber_slots: std::sync::Mutex::new(Vec::new()),
            })
        }

        /// Subscribe this process to the shared memory bus.
        ///
        /// Returns a `ShmReceiver` that can be polled for messages.
        pub fn subscribe_process(&self) -> Result<ShmReceiver> {
            let slot = self.inner.subscribe()?;
            self.subscriber_slots.lock().unwrap().push(slot);
            Ok(ShmReceiver {
                bus: Arc::clone(&self.inner),
                slot,
            })
        }

        /// Publish a message to both shared memory and local broadcast.
        pub fn publish(&self, msg: Message) {
            // Publish to local in-process subscribers
            let _ = self.tx.send(msg.clone());

            // Serialize and publish to shared memory
            if let Ok(json) = msg.to_json() {
                if let Err(e) = self.inner.publish(json.as_bytes()) {
                    tracing::warn!("ShmBus publish failed: {}", e);
                }
            }
        }

        /// Subscribe to local in-process messages (same as Bus::subscribe).
        pub fn subscribe_local(&self) -> broadcast::Receiver<Message> {
            self.tx.subscribe()
        }

        /// Get the raw shared memory region for passing to other processes.
        pub fn region(&self) -> &ShmRegion {
            self.inner.region()
        }

        /// Total size of the shared memory region.
        pub fn total_size(&self) -> usize {
            self.inner.total_size()
        }
    }

    /// A receiver that reads messages from a shared memory slot.
    pub struct ShmReceiver {
        bus: Arc<ShmBus>,
        slot: usize,
    }

    impl ShmReceiver {
        /// Try to receive a message (non-blocking).
        pub fn try_recv(&self) -> Result<Option<Message>> {
            if let Some(data) = self.bus.consume(self.slot)? {
                let json = String::from_utf8(data)
                    .map_err(|e| anyhow::anyhow!("invalid UTF-8 in shm message: {}", e))?;
                let msg = Message::from_json(&json)
                    .map_err(|e| anyhow::anyhow!("failed to deserialize shm message: {}", e))?;
                Ok(Some(msg))
            } else {
                Ok(None)
            }
        }
    }

    impl Drop for ShmReceiver {
        fn drop(&mut self) {
            self.bus.unsubscribe(self.slot);
        }
    }
}
