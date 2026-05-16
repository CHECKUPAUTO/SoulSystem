//! Découverte automatique des pairs via mDNS.
//!
//! Annonce `_soulsystem._tcp` sur le réseau local et écoute
//! les annonces des autres instances.

use crate::bus::{Bus, Message};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Entrée de découverte représentant un pair.
#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub port: u16,
}

/// Service de découverte mDNS.
pub struct DiscoveryService {
    bus: Arc<Bus>,
    peers: HashMap<String, Peer>,
    port: u16,
}

impl DiscoveryService {
    /// Crée un nouveau service de découverte.
    pub fn new(bus: Arc<Bus>, port: u16) -> Self {
        Self {
            bus,
            peers: HashMap::new(),
            port,
        }
    }

    /// Démarre l'annonce et l'écoute mDNS.
    /// En production: utilise mdns-sd crate. Pour l'instant: simulation.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let hostname = hostname::get()
            .unwrap_or_else(|_| std::ffi::OsString::from("soulsystem"))
            .to_string_lossy()
            .to_string();

        info!(
            "DiscoveryService: announcing _soulsystem._tcp on port {}",
            self.port
        );

        // Publie l'annonce sur le bus pour que SYNERGIE la voie
        self.bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("Instance {} annoncée sur le réseau", hostname),
        });

        // Simulation : après 2 secondes, "découvre" des pairs
        // En production: écouter les réponses mDNS
        tokio::spawn({
            let bus = self.bus.clone();
            async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                bus.publish(Message::SynergyDetection {
                    module: "discovery".into(),
                    description: "Recherche de pairs terminée".into(),
                });
            }
        });

        Ok(())
    }

    /// Retourne la liste des pairs découverts.
    pub fn peers(&self) -> Vec<Peer> {
        self.peers.values().cloned().collect()
    }
}
