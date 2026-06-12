//! Metacognition — Self-model, introspection, and capability awareness.
//!
//! The system maintains a representation of its own capabilities,
//! introspects on its performance, and uses existing metrics
//! (turbulence, energy, health) as signals for self-evaluation.
//!
//! Part of SoulLink Autonomy v14 — organism roadmap item #7.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ── Self-Model ─────────────────────────────────────────────────────────────

/// A capability the system believes it possesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub confidence: f64,   // 0..1 belief in this capability
    pub success_rate: f64, // observed success rate
    pub last_evaluated_ms: i64,
    pub evidence_count: u64,
}

/// The system's model of itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModel {
    pub capabilities: Vec<Capability>,
    pub overall_health: f64,      // 0..1 aggregate health score
    pub cognitive_load: f64,      // 0..1 mental saturation
    pub learning_rate: f64,       // current learning efficiency
    pub confidence_trend: String, // "improving", "stable", "declining"
    pub last_updated_ms: i64,
}

impl Default for SelfModel {
    fn default() -> Self {
        Self {
            capabilities: Vec::new(),
            overall_health: 0.8,
            cognitive_load: 0.3,
            learning_rate: 0.05,
            confidence_trend: "stable".into(),
            last_updated_ms: 0,
        }
    }
}

// ── Introspection ──────────────────────────────────────────────────────────

/// Result of an introspection cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionReport {
    pub self_model: SelfModel,
    pub insights: Vec<String>,
    pub recommendations: Vec<String>,
    pub turbulence: f64,
    pub energy: f64,
    pub danger_level: String,
    pub timestamp_ms: i64,
}

// ── Engine ─────────────────────────────────────────────────────────────────

pub struct MetaCognition {
    model: RwLock<SelfModel>,
    introspection_count: RwLock<u64>,
    last_introspection: RwLock<Instant>,
}

impl MetaCognition {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            model: RwLock::new(SelfModel::default()),
            introspection_count: RwLock::new(0),
            last_introspection: RwLock::new(Instant::now()),
        })
    }

    /// Register or update a capability.
    pub async fn register_capability(&self, name: &str, confidence: f64) {
        let mut model = self.model.write().await;
        if let Some(cap) = model.capabilities.iter_mut().find(|c| c.name == name) {
            cap.confidence = cap.confidence * 0.7 + confidence * 0.3; // EMA
            cap.evidence_count += 1;
        } else {
            model.capabilities.push(Capability {
                name: name.to_string(),
                confidence,
                success_rate: 0.5,
                last_evaluated_ms: chrono::Utc::now().timestamp_millis(),
                evidence_count: 1,
            });
        }
    }

    /// Record the outcome of a capability use.
    pub async fn record_outcome(&self, capability: &str, success: bool) {
        let mut model = self.model.write().await;
        if let Some(cap) = model.capabilities.iter_mut().find(|c| c.name == capability) {
            let alpha = 0.1; // smoothing factor
            cap.success_rate = cap.success_rate * (1.0 - alpha) + if success { alpha } else { 0.0 };
            cap.last_evaluated_ms = chrono::Utc::now().timestamp_millis();
            cap.evidence_count += 1;
            if success {
                cap.confidence += 0.01;
            } else {
                cap.confidence = (cap.confidence - 0.05).max(0.0);
            }
        }
    }

    /// Run an introspection cycle.
    /// Combines metrics from the neural mesh to produce a self-assessment.
    pub async fn introspect(
        &self,
        turbulence: f64,
        energy: f64,
        danger_level: &str,
        _success_rate: f64,
    ) -> IntrospectionReport {
        let mut count = self.introspection_count.write().await;
        *count += 1;
        *self.last_introspection.write().await = Instant::now();

        let mut model = self.model.write().await;

        // Update health based on turbulence and energy
        model.overall_health = calculate_health(turbulence, energy);

        // Update cognitive load
        model.cognitive_load = turbulence;

        // Compute confidence trend
        model.confidence_trend = compute_trend(&model.capabilities);

        // Generate insights
        let mut insights = Vec::new();
        let mut recommendations = Vec::new();

        if model.cognitive_load > 0.7 {
            insights.push("High cognitive load — turbulence detected".into());
            recommendations.push("Consider sleep cycle or reduced parallelism".into());
        }
        if model.overall_health < 0.3 {
            insights.push("Low health — multiple systems degraded".into());
            recommendations.push("Trigger preservation emergency protocols".into());
        }
        if model.overall_health > 0.8 && model.cognitive_load < 0.3 {
            insights.push("Optimal state — ready for complex tasks".into());
            recommendations.push("Good time for exploration and learning".into());
        }

        // Check for capability atrophy
        for cap in &model.capabilities {
            if cap.evidence_count > 10 && cap.success_rate < 0.3 {
                insights.push(format!(
                    "Capability '{}' has low success rate ({:.1}%)",
                    cap.name,
                    cap.success_rate * 100.0
                ));
                recommendations.push(format!("Re-train or deprecate capability '{}'", cap.name));
            }
        }

        model.last_updated_ms = chrono::Utc::now().timestamp_millis();

        IntrospectionReport {
            self_model: model.clone(),
            insights,
            recommendations,
            turbulence,
            energy,
            danger_level: danger_level.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Get the current self-model.
    pub async fn self_model(&self) -> SelfModel {
        self.model.read().await.clone()
    }

    /// Number of introspections performed.
    pub async fn introspection_count(&self) -> u64 {
        *self.introspection_count.read().await
    }

    /// Get top N strongest capabilities.
    pub async fn top_capabilities(&self, n: usize) -> Vec<Capability> {
        let model = self.model.read().await;
        let mut caps = model.capabilities.clone();
        caps.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        caps.truncate(n);
        caps
    }

    /// Get weak capabilities (confidence below threshold).
    pub async fn weak_capabilities(&self, threshold: f64) -> Vec<Capability> {
        let model = self.model.read().await;
        model
            .capabilities
            .iter()
            .filter(|c| c.confidence < threshold)
            .cloned()
            .collect()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn calculate_health(turbulence: f64, energy: f64) -> f64 {
    let turbulence_penalty = (1.0 - turbulence).max(0.2);
    let energy_factor = (energy / 100.0).min(1.0);
    (turbulence_penalty * 0.6 + energy_factor * 0.4).clamp(0.0, 1.0)
}

fn compute_trend(caps: &[Capability]) -> String {
    if caps.is_empty() {
        return "stable".into();
    }
    let avg: f64 = caps.iter().map(|c| c.confidence).sum::<f64>() / caps.len() as f64;
    if avg > 0.7 {
        "improving"
    } else if avg < 0.3 {
        "declining"
    } else {
        "stable"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_query_capability() {
        let mc = MetaCognition::new();
        mc.register_capability("code_generation", 0.8).await;
        let caps = mc.top_capabilities(5).await;
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "code_generation");
    }

    #[tokio::test]
    async fn record_outcome_updates_success_rate() {
        let mc = MetaCognition::new();
        mc.register_capability("test_cap", 0.5).await;
        mc.record_outcome("test_cap", true).await;
        mc.record_outcome("test_cap", false).await;
        let caps = mc.top_capabilities(5).await;
        assert!(caps[0].success_rate > 0.0);
    }

    #[tokio::test]
    async fn introspection_generates_insights() {
        let mc = MetaCognition::new();
        mc.register_capability("failing_cap", 0.3).await;
        for _ in 0..15 {
            mc.record_outcome("failing_cap", false).await;
        }
        let report = mc.introspect(0.8, 90.0, "normal", 0.5).await;
        assert!(!report.insights.is_empty());
        assert!(report.self_model.overall_health > 0.0);
    }

    #[tokio::test]
    async fn weak_capabilities_filtered() {
        let mc = MetaCognition::new();
        mc.register_capability("strong", 0.9).await;
        mc.register_capability("weak", 0.2).await;
        let weak = mc.weak_capabilities(0.5).await;
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].name, "weak");
    }

    #[test]
    fn health_calculation() {
        let h1 = calculate_health(0.5, 100.0);
        assert!(h1 > 0.5);
        let h2 = calculate_health(0.9, 10.0);
        assert!(h2 < 0.5);
    }
}
