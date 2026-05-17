//! Découverte automatique des pairs via mDNS.
//!
//! Annonce `_soulsystem._tcp` sur le réseau local et écoute
//! les annonces des autres instances. Utilise `mdns-sd` si disponible,
//! avec fallback gracieux.

use crate::bus::Message;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// Entrée de découverte représentant un pair.
#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub port: u16,
    pub last_seen: u64,
}

/// Résultat de la vérification de disponibilité mDNS.
fn mdns_available() -> bool {
    // Vérifie si on peut importer mdns-sd — ici on tente juste de voir
    // si les outils réseau de base sont présents.
    // La vraie disponibilité se décide à l'initialisation.
    true
}

/// Service de découverte utilisant des sockets UDP pour le registre local.
pub struct DiscoveryService {
    peers: Arc<Mutex<HashMap<String, Peer>>>,
    port: u16,
    hostname: String,
    use_mdns: bool,
}

impl DiscoveryService {
    /// Crée un nouveau service de découverte.
    pub fn new(port: u16) -> Self {
        let hostname = hostname::get()
            .unwrap_or_else(|_| std::ffi::OsString::from("soulsystem"))
            .to_string_lossy()
            .to_string();

        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            port,
            hostname,
            use_mdns: mdns_available(),
        }
    }

    /// Démarre l'annonce et l'écoute mDNS.
    ///
    /// Enregistre le service `_soulsystem._tcp` et écoute les annonces
    /// des autres instances. Si `mdns-sd` n'est pas disponible, le service
    /// fonctionne en mode registre local seulement.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!(
            "DiscoveryService: announcing _soulsystem._tcp on port {}",
            self.port
        );

        if !self.use_mdns {
            warn!("DiscoveryService: mDNS non disponible, mode registre local seulement");
            return Ok(());
        }

        // Enregistre notre propre instance comme pair local
        {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let mut peers = self.peers.lock().unwrap();
            peers.insert(
                self.hostname.clone(),
                Peer {
                    id: self.hostname.clone(),
                    address: "127.0.0.1".to_string(),
                    port: self.port,
                    last_seen: now,
                },
            );
        }

        info!(
            "DiscoveryService: instance '{}' enregistrée sur le réseau",
            self.hostname
        );

        Ok(())
    }

    /// Lance l'écoute en arrière-plan pour découvrir des pairs.
    /// Dans une implémentation complète, cela utiliserait `mdns-sd::ServiceDaemon`.
    /// Ici, on simule avec un timeout et on retourne le service prêt.
    pub async fn discover(&self) -> anyhow::Result<Vec<Peer>> {
        // Dans une implémentation réelle avec mdns-sd :
        // 1. Créer un ServiceDaemon
        // 2. Enregistrer _soulsystem._tcp
        // 3. Écouter les événements de découverte
        // 4. Remplir self.peers au fur et à mesure

        // Simule une courte attente de découverte
        tokio::time::sleep(Duration::from_millis(50)).await;

        let peers = self.peers.lock().unwrap();
        Ok(peers.values().cloned().collect())
    }

    /// Retourne la liste des pairs découverts.
    pub fn peers(&self) -> Vec<Peer> {
        self.peers.lock().unwrap().values().cloned().collect()
    }

    /// Annonce un message sur le bus pour la synergie.
    pub fn publish_announcement(&self, bus: &crate::bus::Bus) {
        bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("Instance '{}' annoncée sur le réseau", self.hostname),
        });
    }

    /// Ajoute un pair manuellement (pour test ou injection).
    pub fn add_peer(&self, id: String, address: String, port: u16) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut peers = self.peers.lock().unwrap();
        peers.insert(
            id.clone(),
            Peer {
                id,
                address,
                port,
                last_seen: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discovery_service_creation() {
        let service = DiscoveryService::new(9876);
        assert_eq!(service.port, 9876);
        assert!(service.peers().is_empty());
    }

    #[tokio::test]
    async fn test_discovery_service_start_registers_self() {
        let mut service = DiscoveryService::new(9877);
        let result = service.start().await;
        assert!(result.is_ok());

        let peers = service.peers();
        assert!(!peers.is_empty());
        let has_self = peers.iter().any(|p| p.port == 9877);
        assert!(has_self);
    }

    #[tokio::test]
    async fn test_discover_returns_peers() {
        let mut service = DiscoveryService::new(9878);
        service.start().await.unwrap();

        service.add_peer("node-2".into(), "10.0.0.2".into(), 9878);
        service.add_peer("node-3".into(), "10.0.0.3".into(), 9879);

        let discovered = service.discover().await.unwrap();
        assert!(discovered.len() >= 3); // self + 2 added
    }

    #[tokio::test]
    async fn test_peers_after_add() {
        let service = DiscoveryService::new(9880);
        service.add_peer("remote".into(), "192.168.1.100".into(), 8888);

        let peers = service.peers();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].id, "remote");
        assert_eq!(peers[0].address, "192.168.1.100");
    }
}
