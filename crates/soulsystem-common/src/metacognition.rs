//! Metacognition — Types partagés pour l'auto-évaluation des décisions.

use serde::{Deserialize, Serialize};

/// Métrique de performance d'un cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleMetrics {
    pub cycle: u64,
    pub timestamp: String,
    pub action_taken: String,
    pub energy_before: f64,
    pub energy_after: f64,
    pub cpu_before: f32,
    pub cpu_after: f32,
    pub mem_before: f32,
    pub mem_after: f32,
    pub alerts_before: usize,
    pub alerts_after: usize,
    pub action_success: bool,
    pub llm_confidence: f32,
    pub time_taken_ms: u64,
}

/// Catégorie de qualité d'une décision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityCategory {
    Excellent,
    Good,
    Neutral,
    Poor,
    Catastrophic,
}

impl QualityCategory {
    pub fn from_score(score: f32) -> Self {
        match score {
            s if s >= 0.8 => Self::Excellent,
            s if s >= 0.6 => Self::Good,
            s if s >= 0.4 => Self::Neutral,
            s if s >= 0.2 => Self::Poor,
            _ => Self::Catastrophic,
        }
    }
}

impl std::fmt::Display for QualityCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Excellent => write!(f, "Excellent"),
            Self::Good => write!(f, "Good"),
            Self::Neutral => write!(f, "Neutral"),
            Self::Poor => write!(f, "Poor"),
            Self::Catastrophic => write!(f, "Catastrophic"),
        }
    }
}

/// Évaluation de la qualité d'une décision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionQuality {
    pub cycle: u64,
    pub action: String,
    pub score: f32,
    pub category: QualityCategory,
    pub explanation: String,
    pub recommendation: String,
}

/// Rapport d'introspection (méta-cognition avancée).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionReport {
    pub self_model: String,
    pub insights: Vec<String>,
    pub recommendations: Vec<String>,
    pub turbulence: f64,
    pub energy: f64,
    pub danger_level: f64,
    pub timestamp_ms: u64,
}

/// Calcule le score d'un CycleMetrics.
pub fn score_cycle(m: &CycleMetrics) -> f32 {
    let mut score = 0.0f32;

    if m.action_success {
        score += 0.3;
    } else {
        score -= 0.3;
    }

    let energy_delta = m.energy_after - m.energy_before;
    if energy_delta > 0.0 {
        score += 0.2;
    } else if energy_delta < -0.5 {
        score -= 0.2;
    }

    let cpu_delta = m.cpu_after - m.cpu_before;
    if cpu_delta < -5.0 {
        score += 0.2;
    } else if cpu_delta > 10.0 {
        score -= 0.2;
    }

    let mem_delta = m.mem_after - m.mem_before;
    if mem_delta < -3.0 {
        score += 0.1;
    } else if mem_delta > 5.0 {
        score -= 0.1;
    }

    let alert_delta = m.alerts_after as i64 - m.alerts_before as i64;
    if alert_delta < 0 {
        score += 0.2;
    } else if alert_delta > 0 {
        score -= 0.2;
    }

    if m.llm_confidence > 0.8 && !m.action_success {
        score -= 0.2;
    }

    score.clamp(0.0, 1.0)
}

/// Génère une recommandation basée sur la catégorie.
pub fn recommendation_for(category: &QualityCategory) -> &'static str {
    match category {
        QualityCategory::Excellent => {
            "Continuer cette stratégie. Essayer d'appliquer à d'autres situations."
        }
        QualityCategory::Good => "Bonne direction. Vérifier si l'impact peut être amplifié.",
        QualityCategory::Neutral => "Pas d'impact mesurable. Changer d'approche ou attendre.",
        QualityCategory::Poor => {
            "Action contre-productive. Privilégier une autre stratégie au prochain cycle."
        }
        QualityCategory::Catastrophic => {
            "Détérioration sévère. Pause immédiate et analyse approfondie."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_category_from_score() {
        assert_eq!(
            QualityCategory::from_score(0.9),
            QualityCategory::Excellent
        );
        assert_eq!(QualityCategory::from_score(0.7), QualityCategory::Good);
        assert_eq!(QualityCategory::from_score(0.5), QualityCategory::Neutral);
        assert_eq!(QualityCategory::from_score(0.3), QualityCategory::Poor);
        assert_eq!(
            QualityCategory::from_score(0.1),
            QualityCategory::Catastrophic
        );
    }

    #[test]
    fn test_score_cycle_success() {
        let m = CycleMetrics {
            cycle: 1,
            timestamp: "2025-01-01T00:00:00Z".into(),
            action_taken: "test".into(),
            energy_before: 0.5,
            energy_after: 0.7,
            cpu_before: 60.0,
            cpu_after: 50.0,
            mem_before: 50.0,
            mem_after: 48.0,
            alerts_before: 3,
            alerts_after: 1,
            action_success: true,
            llm_confidence: 0.9,
            time_taken_ms: 1000,
        };
        let score = score_cycle(&m);
        assert!(score >= 0.8, "Expected >= 0.8, got {score}");
    }

    #[test]
    fn test_score_cycle_failure() {
        let m = CycleMetrics {
            cycle: 1,
            timestamp: "2025-01-01T00:00:00Z".into(),
            action_taken: "test".into(),
            energy_before: 0.5,
            energy_after: 0.2,
            cpu_before: 60.0,
            cpu_after: 80.0,
            mem_before: 50.0,
            mem_after: 70.0,
            alerts_before: 1,
            alerts_after: 5,
            action_success: false,
            llm_confidence: 0.95,
            time_taken_ms: 5000,
        };
        let score = score_cycle(&m);
        assert!(score <= 0.1, "Expected <= 0.1, got {score}");
    }

    #[test]
    fn test_decision_quality_serialization() {
        let dq = DecisionQuality {
            cycle: 1,
            action: "test".into(),
            score: 0.85,
            category: QualityCategory::Excellent,
            explanation: "good".into(),
            recommendation: "keep going".into(),
        };
        let json = serde_json::to_string(&dq).unwrap();
        let decoded: DecisionQuality = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.category, QualityCategory::Excellent);
    }
}
