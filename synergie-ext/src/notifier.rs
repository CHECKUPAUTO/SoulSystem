//! Notificateur d'anomalies — publie sur le bus SoulSystem.

use crate::detectors::anomaly::Anomaly;

/// Publie une anomalie détectée sur le bus.
/// En production, ceci déclenche une notification Telegram via Clawd.
pub fn notify_anomaly(anomaly: &Anomaly) -> String {
    match anomaly {
        Anomaly::HnnTickDrop { current, previous_avg, drop_percent } => {
            format!(
                "⚠️ HNN Tick Drop: {} ticks/sec (prev avg: {:.0}, drop: {:.1}%)",
                current, previous_avg, drop_percent * 100.0
            )
        }
        Anomaly::EvolveDivergence { crate_name, generations_declining } => {
            format!(
                "⚠️ OpenEvolve divergence on {}: {} consecutive declining generations",
                crate_name, generations_declining
            )
        }
    }
}
