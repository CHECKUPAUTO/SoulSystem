//! Essaims collaboratifs — orchestration distribuée.
//!
//! Plusieurs instances SoulSystem s'auto-organisent pour
//! résoudre des tâches complexes en parallèle.

use crate::bus::{Bus, Message};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Tâche distribuable dans l'essaim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub description: String,
    pub assigned_to: Option<String>,
    pub status: TaskStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
}

/// Coordinateur d'essaim.
pub struct SwarmCoordinator {
    bus: Arc<Bus>,
    tasks: HashMap<String, SwarmTask>,
    peers: Vec<String>,
    is_leader: bool,
}

impl SwarmCoordinator {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            bus,
            tasks: HashMap::new(),
            peers: Vec::new(),
            is_leader: false,
        }
    }

    /// Ajoute un pair à l'essaim.
    pub fn add_peer(&mut self, peer_id: String) {
        if !self.peers.contains(&peer_id) {
            self.peers.push(peer_id.clone());
            info!(
                "Swarm: peer {} joined (total: {})",
                peer_id,
                self.peers.len()
            );
        }
    }

    /// Retire un pair.
    pub fn remove_peer(&mut self, peer_id: &str) {
        self.peers.retain(|p| p != peer_id);
        // Réassigner les tâches du pair défaillant
        for task in self.tasks.values_mut() {
            if task.assigned_to.as_deref() == Some(peer_id) {
                task.assigned_to = None;
                task.status = TaskStatus::Pending;
                warn!("Swarm: task {} reassigned (peer {} lost)", task.id, peer_id);
            }
        }
    }

    /// Distribue une tâche à un pair disponible.
    pub fn dispatch(&mut self, description: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let task = SwarmTask {
            id: id.clone(),
            description: description.to_string(),
            assigned_to: self.peers.first().cloned(),
            status: if self.peers.is_empty() {
                TaskStatus::Pending
            } else {
                TaskStatus::Assigned
            },
            result: None,
        };

        self.bus.publish(Message::SynergyDetection {
            module: "swarm".into(),
            description: format!("Task {} dispatched: {}", &id[..8], description),
        });

        self.tasks.insert(id.clone(), task);
        id
    }

    /// Élit un leader simple (premier arrivé).
    pub fn elect_leader(&mut self) {
        if !self.peers.is_empty() && !self.is_leader {
            self.is_leader = true;
            info!("Swarm: elected as leader");
        }
    }
}
