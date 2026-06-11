//! SynapseHook — OpenClaw message → mesh injection + memory ingestion.
//!
//! When a message arrives through OpenClaw:
//! 1. Compute "surge" from message length/urgency
//! 2. POST to orchestrator /api/mesh/reinforce (inject momentum)
//! 3. POST to orchestrator /api/memory/ingest (store in MemoryGraph)
//!
//! This is the bridge that makes the mesh respond to external stimuli.

use crate::node::NodeId;
use crate::pulse::AutonomyPulse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// A message received from OpenClaw.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseMessage {
    pub text: String,
    pub author: String,
    pub channel: String,
    pub timestamp: String,
}

/// Result of synapse processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseResult {
    pub surge: f64,
    pub reinforce_sent: bool,
    pub ingest_sent: bool,
}

/// The synapse hook — processes OpenClaw messages into neural stimuli.
pub struct SynapseHook {
    orchestrator_url: String,
    client: reqwest::Client,
    pulse: Arc<AutonomyPulse>,
}

impl SynapseHook {
    pub fn new(orchestrator_url: &str, pulse: Arc<AutonomyPulse>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            orchestrator_url: orchestrator_url.to_string(),
            client,
            pulse,
        }
    }

    /// Process a message from OpenClaw.
    ///
    /// 1. Compute surge (emotional intensity) from message content
    /// 2. Inject surge into the Reasoning node via AutonomyPulse
    /// 3. POST reinforcement to orchestrator
    /// 4. POST memory ingestion to orchestrator
    pub async fn process(&self, msg: &SynapseMessage) -> SynapseResult {
        let surge = self.compute_surge(&msg.text);

        // 1. Inject surge into local mesh
        self.pulse.surge(NodeId::Reasoning, surge);

        // 2. POST to orchestrator reinforce endpoint
        let reinforce_sent = self.post_reinforce("reasoning", surge, &msg.author).await;

        // 3. POST to orchestrator memory ingest endpoint
        let ingest_sent = self.post_ingest(&msg.text, &msg.author).await;

        info!(
            "Synapse: surge={:.2} reinforce={} ingest={}",
            surge, reinforce_sent, ingest_sent
        );

        SynapseResult {
            surge,
            reinforce_sent,
            ingest_sent,
        }
    }

    /// Compute surge from message content.
    ///
    /// Factors:
    /// - Length: longer messages → more surge (capped at 1.0)
    /// - Urgency markers: !, ?, URGENT → more surge
    /// - All caps → more surge
    fn compute_surge(&self, text: &str) -> f64 {
        let base = (text.len() as f64 / 100.0).min(1.0);

        let exclamation_count = text.matches('!').count() as f64;
        let question_count = text.matches('?').count() as f64;
        let urgency_bonus = (exclamation_count * 0.05 + question_count * 0.03).min(0.3);

        let caps_ratio =
            text.chars().filter(|c| c.is_uppercase()).count() as f64 / text.len().max(1) as f64;
        let caps_bonus = if caps_ratio > 0.5 { 0.2 } else { 0.0 };

        (base + urgency_bonus + caps_bonus).min(1.5)
    }

    async fn post_reinforce(&self, node: &str, force: f64, source: &str) -> bool {
        let url = format!("{}/api/mesh/reinforce", self.orchestrator_url);
        let body = serde_json::json!({
            "node": node,
            "force": force,
            "source": source
        });

        match self.client.post(&url).json(&body).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn post_ingest(&self, text: &str, author: &str) -> bool {
        let url = format!("{}/api/memory/ingest", self.orchestrator_url);
        let body = serde_json::json!({
            "text": text,
            "author": author,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        match self.client.post(&url).json(&body).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::afferent::AfferentNerve;
    use crate::afferent::SensorState;

    #[test]
    fn compute_surge_short_message() {
        let afferent = Arc::new(AfferentNerve::with_state(SensorState::default()));
        let pulse = Arc::new(AutonomyPulse::new(afferent));
        let hook = SynapseHook::new("http://127.0.0.1:9020", pulse);
        let surge = hook.compute_surge("hello");
        assert!(
            surge < 0.1,
            "short message should have low surge, got {}",
            surge
        );
    }

    #[test]
    fn compute_surge_long_message() {
        let afferent = Arc::new(AfferentNerve::with_state(SensorState::default()));
        let pulse = Arc::new(AutonomyPulse::new(afferent));
        let hook = SynapseHook::new("http://127.0.0.1:9020", pulse);
        let long_msg = "a".repeat(200);
        let surge = hook.compute_surge(&long_msg);
        assert!(
            surge >= 1.0,
            "200-char message should have surge >= 1.0, got {}",
            surge
        );
    }

    #[test]
    fn compute_surge_urgent_message() {
        let afferent = Arc::new(AfferentNerve::with_state(SensorState::default()));
        let pulse = Arc::new(AutonomyPulse::new(afferent));
        let hook = SynapseHook::new("http://127.0.0.1:9020", pulse);
        let surge = hook.compute_surge("URGENT!!! Fix this NOW!!!");
        assert!(
            surge > 0.5,
            "urgent message should have high surge, got {}",
            surge
        );
    }

    #[test]
    fn compute_surge_capped() {
        let afferent = Arc::new(AfferentNerve::with_state(SensorState::default()));
        let pulse = Arc::new(AutonomyPulse::new(afferent));
        let hook = SynapseHook::new("http://127.0.0.1:9020", pulse);
        let surge = hook.compute_surge(&"a".repeat(500));
        assert!(surge <= 1.5, "surge should be capped at 1.5, got {}", surge);
    }
}
