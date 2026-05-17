//! Bus de messages interne pour SoulSystem.
//!
//! Utilise `tokio::sync::broadcast` pour permettre la publication et
//! la souscription a des messages entre modules de maniere decouplee.

use tokio::sync::broadcast;

/// Types de messages circulant sur le bus.
#[derive(Clone, Debug)]
pub enum Message {
    /// Detection de synergie entre modules.
    SynergyDetection { module: String, description: String },
    /// Decouverte par AVID (arXiv, web, etc.).
    AvidDiscovery { source: String, summary: String },
    /// Resultat d'optimisation par OpenEvolve.
    EvolveOptimization { crate_name: String, score: f64 },
    /// Statut du mesh HNN (ticks par seconde).
    HnnStatus { ticks_per_sec: u64 },
    /// Message generique pour le bridge WebSocket.
    Custom {
        topic: String,
        payload: serde_json::Value,
    },
}

/// Bus de messages decouple.
///
/// Les modules peuvent s'abonner (`subscribe`) et publier (`publish`)
/// sans se connaitre directement.
#[derive(Clone)]
pub struct Bus {
    sender: broadcast::Sender<Message>,
}

impl Bus {
    /// Cree un nouveau bus avec une capacite de file d'attente donnee.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Bus { sender }
    }

    /// S'abonne au bus. Les messages publies apres l'abonnement
    /// seront recus.
    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.sender.subscribe()
    }

    /// Publie un message sur le bus.
    ///
    /// Si aucun abonne n'est connecte, le message est silencieusement
    /// ignore.
    pub fn publish(&self, msg: Message) {
        let _ = self.sender.send(msg);
    }

    /// Nombre d'abonnes actuellement connectes.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(256)
    }
}
