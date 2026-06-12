use crate::console::server::ClinicalStreamingServer;
use neural_metacognition::SystemAuditor;
use std::sync::Arc;

/// Initialise et retourne un `ClinicalStreamingServer` prêt à être démarré.
///
/// Le serveur clinique expose des endpoints HTTP (`/health`, `/metrics`) pour
/// monitorer l'état du système en temps réel via l'auditor partagé.
pub fn init_console(auditor: Arc<SystemAuditor>, port: u16) -> Arc<ClinicalStreamingServer> {
    let server = Arc::new(ClinicalStreamingServer::new(auditor, port));
    tracing::info!(
        port,
        "ClinicalStreamingServer initialisé (endpoints /health, /metrics)"
    );
    server
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_console_returns_valid_instance() {
        let auditor = Arc::new(neural_metacognition::SystemAuditor::new());
        let server = init_console(auditor, 9090);
        // Le serveur est créé mais pas démarré (pas de tokio runtime dans ce test)
        drop(server);
    }
}
