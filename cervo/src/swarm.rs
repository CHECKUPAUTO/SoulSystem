use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::Data;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmMessage {
    Data {
        from: Uuid,
        payload: Data,
    },
    MutationAnnounce {
        from: Uuid,
        algo_name: String,
        success: bool,
        divergence: f64,
        fitness: f64,
    },
    SyncRequest {
        from: Uuid,
        target_algo: String,
    },
    AlgorithmShare {
        from: Uuid,
        algo_name: String,
        algo_config: serde_json::Value,
    },
    HealthReport {
        from: Uuid,
        rsi: f64,
        saturation: f64,
        active_memory: usize,
        fitness: f64,
    },
    Shutdown {
        from: Uuid,
    },
    Heartbeat {
        from: Uuid,
        timestamp: u64,
    },
}

#[derive(Debug, Clone)]
pub struct SwarmBus {
    tx: broadcast::Sender<SwarmMessage>,
    capacity: Arc<RwLock<usize>>,
    subscriber_count: Arc<RwLock<usize>>,
}

impl SwarmBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        SwarmBus {
            tx,
            capacity: Arc::new(RwLock::new(capacity)),
            subscriber_count: Arc::new(RwLock::new(0)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SwarmMessage> {
        *self.subscriber_count.write() += 1;
        self.tx.subscribe()
    }

    pub fn publish(
        &self,
        msg: SwarmMessage,
    ) -> Result<usize, broadcast::error::SendError<SwarmMessage>> {
        self.tx.send(msg)
    }

    pub fn sender(&self) -> broadcast::Sender<SwarmMessage> {
        self.tx.clone()
    }

    pub fn capacity(&self) -> usize {
        *self.capacity.read()
    }

    pub fn subscriber_count(&self) -> usize {
        *self.subscriber_count.read()
    }

    pub fn is_full(&self) -> bool {
        self.tx.len() >= self.capacity()
    }

    pub fn len(&self) -> usize {
        self.tx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tx.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_swarm_bus_creation() {
        let bus = SwarmBus::new(16);
        assert_eq!(bus.capacity(), 16);
        assert!(bus.is_empty());
        assert_eq!(bus.len(), 0);
    }

    #[test]
    fn test_publish_and_subscribe() {
        let bus = SwarmBus::new(16);
        let mut rx = bus.subscribe();
        let msg = SwarmMessage::Heartbeat {
            from: Uuid::new_v4(),
            timestamp: 0,
        };
        bus.publish(msg.clone()).unwrap();
        let received = rx.try_recv().unwrap();
        assert!(matches!(received, SwarmMessage::Heartbeat { .. }));
    }

    #[test]
    fn test_subscriber_count() {
        let bus = SwarmBus::new(16);
        let c1 = bus.subscriber_count();
        let _rx = bus.subscribe();
        assert_eq!(bus.subscriber_count(), c1 + 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), c1 + 2);
    }

    #[test]
    fn test_publish_with_receiver() {
        let bus = SwarmBus::new(16);
        let _rx = bus.subscribe();
        let msg = SwarmMessage::Heartbeat {
            from: Uuid::new_v4(),
            timestamp: 42,
        };
        let result = bus.publish(msg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_receiver_lag() {
        let bus = SwarmBus::new(4);
        let _rx = bus.subscribe();
        let msg = SwarmMessage::Heartbeat {
            from: Uuid::new_v4(),
            timestamp: 0,
        };
        for _ in 0..4 {
            bus.publish(msg.clone()).unwrap();
        }
        assert_eq!(bus.len(), 4);
    }

    #[test]
    fn test_sender_retrieval() {
        let bus = SwarmBus::new(8);
        let sender = bus.sender();
        let msg = SwarmMessage::Shutdown {
            from: Uuid::new_v4(),
        };
        let _ = sender.send(msg);
    }
}
