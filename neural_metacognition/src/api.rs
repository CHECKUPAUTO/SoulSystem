use crate::metacognition::auditor::SystemAuditor;
use std::sync::Arc;

/// Initialise et retourne un `SystemAuditor` prêt à l'emploi.
///
/// L'auditor est le composant central de la métacognition : il enregistre les
/// `TelemetryFrame` dans un ring buffer lock-free pour le monitoring en temps réel.
pub fn init_auditor() -> Arc<SystemAuditor> {
    let auditor = Arc::new(SystemAuditor::new());
    tracing::info!(
        buffer_capacity = 4096,
        "SystemAuditor initialisé (ring buffer lock-free)"
    );
    auditor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_auditor_returns_valid_instance() {
        let auditor = init_auditor();
        let frame = crate::metacognition::auditor::TelemetryFrame {
            timestamp_ns: 1000,
            memory_throughput_bytes_per_sec: 2048,
            active_synapse_count: 42,
            current_meta_loss: 0.5,
        };
        auditor.record(frame);
        let latest = auditor.get_latest();
        assert_eq!(latest.timestamp_ns, 1000);
        assert_eq!(latest.active_synapse_count, 42);
    }
}
