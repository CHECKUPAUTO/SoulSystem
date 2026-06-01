#![allow(unused_imports)]
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
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
    Custom {
        topic: String,
        payload: serde_json::Value,
    },
}

impl Message {
    pub fn to_binary(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_binary(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

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
