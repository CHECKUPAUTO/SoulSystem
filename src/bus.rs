//! Bus de messages interne pour SoulSystem.
//!
//! Utilise `tokio::sync::broadcast` pour permettre la publication et
//! la souscription à des messages entre modules de manière découplée.

use tokio::sync::broadcast;

/// Types de messages circulant sur le bus.
#[derive(Clone, Debug)]
pub enum Message {
    /// Détection de synergie entre modules.
    SynergyDetection { module: String, description: String },
    /// Découverte par AVID (arXiv, web, etc.).
    AvidDiscovery { source: String, summary: String },
    /// Résultat d'optimisation par OpenEvolve.
    EvolveOptimization { crate_name: String, score: f64 },
    /// Statut du mesh HNN (ticks par seconde).
    HnnStatus { ticks_per_sec: u64 },
}

/// Bus de messages découplé.
///
/// Les modules peuvent s'abonner (`subscribe`) et publier (`publish`)
/// sans se connaître directement.
#[derive(Clone)]
pub struct Bus {
    sender: broadcast::Sender<Message>,
}

impl Bus {
    /// Crée un nouveau bus avec une capacité de file d'attente donnée.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Bus { sender }
    }

    /// S'abonne au bus. Les messages publiés après l'abonnement
    /// seront reçus.
    pub fn subscribe(&self) -> broadcast::Receiver<Message> {
        self.sender.subscribe()
    }

    /// Publie un message sur le bus.
    ///
    /// Si aucun abonné n'est connecté, le message est silencieusement
    /// ignoré.
    pub fn publish(&self, msg: Message) {
        let _ = self.sender.send(msg);
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(256)
    }
}
