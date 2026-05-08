//! HNN Bridge — lecture du blackboard SoulLink (ports V13)

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnnState {
    pub timestamp: String,
    pub organs: HashMap<String, serde_json::Value>,
    pub blackboard: serde_json::Value,
}

impl HnnState {
    pub async fn fetch() -> Self {
        let mut organs = HashMap::new();
        let mut blackboard = serde_json::Value::Null;

        // Fetch orchestrator blackboard (port 9020)
        match reqwest::Client::new()
            .get("http://127.0.0.1:9020/api/stats")
            .timeout(std::time::Duration::from_secs(3))
            .send().await
        {
            Ok(r) if r.status().is_success() => {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    blackboard = json;
                }
            }
            _ => {}
        }

        // Fetch HNN V13 organs (ports 9010-9015)
        for (port, name) in [
            (9010, "science"), (9011, "mind"), (9012, "engineer"),
            (9013, "crypto"), (9014, "creative"), (9015, "meta"),
        ] {
            match reqwest::Client::new()
                .get(format!("http://127.0.0.1:{}/api/stats", port))
                .timeout(std::time::Duration::from_secs(2))
                .send().await
            {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        organs.insert(name.to_string(), json);
                    }
                }
                _ => {}
            }
        }

        // Fetch V14 organs (ports 9095, 9786)
        for (port, name) in [
            (9095, "v14_fusion"), (9786, "chronos"),
            (9040, "foresight"), (9041, "homeostasis"), (9042, "creativity"),
            (9043, "social"), (9044, "validation"),
        ] {
            match reqwest::Client::new()
                .get(format!("http://127.0.0.1:{}/api/stats", port))
                .timeout(std::time::Duration::from_secs(2))
                .send().await
            {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        organs.insert(name.to_string(), json);
                    }
                }
                _ => {}
            }
        }

        // Fetch memory (port 9030)
        match reqwest::Client::new()
            .get("http://127.0.0.1:9030/api/stats")
            .timeout(std::time::Duration::from_secs(2))
            .send().await
        {
            Ok(r) if r.status().is_success() => {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    organs.insert("neural_memory".to_string(), json);
                }
            }
            _ => {}
        }

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            organs,
            blackboard,
        }
    }

    pub fn summary(&self) -> String {
        let organ_count = self.organs.len();
        let mut turbulence = Vec::new();
        for (name, state) in &self.organs {
            if let Some(t) = state.get("turbulence")
                .and_then(|t| t.get("value"))
                .and_then(|v| v.as_f64())
            {
                turbulence.push(format!("{}:{:.2}", name, t));
            } else if let Some(t) = state.get("hnn_state")
                .and_then(|h| h.get("turbulence"))
                .and_then(|t| t.get("value"))
                .and_then(|v| v.as_f64())
            {
                turbulence.push(format!("{}:{:.2}", name, t));
            }
        }
        format!("HNN: {}/9 organs | {}", organ_count, turbulence.join(", "))
    }

    pub fn is_healthy(&self) -> bool {
        self.organs.len() >= 6
    }
}
