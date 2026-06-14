//! Health — Score de santé et statuts partagés.

use serde::{Deserialize, Serialize};

/// Statut de santé du système.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Critical,
    Failed,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "Healthy"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Critical => write!(f, "Critical"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Rapport de santé unifié.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub score: f64,
    pub avg_latency_ms: f64,
    pub compactions_last_hour: u64,
    pub message_rate: f64,
    pub estimated_context_tokens: u64,
    pub error_rate: f64,
    pub is_fatigued: bool,
}

/// Calcule un score de santé global (0.0 = critique, 1.0 = parfait).
///
/// Poids : énergie 25%, CPU 20%, mémoire 20%, erreurs 20%, alertes 15%.
pub fn compute_health_score(
    energy: f64,
    cpu: f64,
    memory: f64,
    error_rate: f64,
    alerts: u32,
) -> f64 {
    let energy_score = energy.clamp(0.0, 1.0);
    let cpu_score = 1.0 - (cpu / 100.0).clamp(0.0, 1.0);
    let mem_score = 1.0 - (memory / 100.0).clamp(0.0, 1.0);
    let error_score = 1.0 - error_rate.clamp(0.0, 1.0);
    let alert_penalty = (alerts as f64 * 0.05).min(0.3);

    let raw = (energy_score * 0.25)
        + (cpu_score * 0.20)
        + (mem_score * 0.20)
        + (error_score * 0.20)
        + ((1.0 - alert_penalty) * 0.15);

    raw.clamp(0.0, 1.0)
}

/// Convertit un score en statut de santé.
pub fn score_to_status(score: f64) -> HealthStatus {
    match score {
        s if s >= 0.8 => HealthStatus::Healthy,
        s if s >= 0.6 => HealthStatus::Degraded,
        s if s >= 0.3 => HealthStatus::Critical,
        _ => HealthStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healthy_score() {
        let score = compute_health_score(0.9, 30.0, 40.0, 0.05, 0);
        assert!(score >= 0.8, "Expected >= 0.8, got {score}");
        assert_eq!(score_to_status(score), HealthStatus::Healthy);
    }

    #[test]
    fn test_degraded_score() {
        let score = compute_health_score(0.5, 70.0, 70.0, 0.2, 3);
        assert!(
            (0.3..0.8).contains(&score),
            "Expected [0.3, 0.8), got {score}"
        );
    }

    #[test]
    fn test_critical_score() {
        let score = compute_health_score(0.1, 95.0, 95.0, 0.5, 10);
        assert!(score < 0.3, "Expected < 0.3, got {score}");
        assert_eq!(score_to_status(score), HealthStatus::Failed);
    }

    #[test]
    fn test_health_report_serialization() {
        let report = HealthReport {
            status: HealthStatus::Healthy,
            score: 0.95,
            avg_latency_ms: 12.5,
            compactions_last_hour: 0,
            message_rate: 5.0,
            estimated_context_tokens: 10000,
            error_rate: 0.01,
            is_fatigued: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: HealthReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.status, HealthStatus::Healthy);
        assert!((decoded.score - 0.95).abs() < 0.01);
    }
}
