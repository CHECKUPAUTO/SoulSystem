//! HNN Bridge — lecture du blackboard SoulLink (ports V13)

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnnState {
    pub timestamp: String,
    pub organs: HashMap<String, serde_json::Value>,
    pub blackboard: serde_json::Value,
}

impl HnnState {
    pub async fn fetch() -> Self {
        // Bolt ⚡: Use shared client and JoinSet for concurrent fetching.
        // Replaces ~15 sequential HTTP calls with parallel ones.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        let mut set = tokio::task::JoinSet::new();

        // 1. Fetch orchestrator blackboard (port 9020)
        let c = client.clone();
        set.spawn(async move {
            let res = c.get("http://127.0.0.1:9020/api/stats").send().await;
            if let Ok(r) = res {
                if r.status().is_success() {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        return ("blackboard", json);
                    }
                }
            }
            ("blackboard", serde_json::Value::Null)
        });

        // 2. Fetch HNN V13 organs (ports 9010-9015)
        for (port, name) in [
            (9010, "science"), (9011, "mind"), (9012, "engineer"),
            (9013, "crypto"), (9014, "creative"), (9015, "meta"),
        ] {
            let c = client.clone();
            set.spawn(async move {
                let res = c.get(format!("http://127.0.0.1:{}/api/stats", port)).send().await;
                if let Ok(r) = res {
                    if r.status().is_success() {
                        if let Ok(json) = r.json::<serde_json::Value>().await {
                            return (name, json);
                        }
                    }
                }
                (name, serde_json::Value::Null)
            });
        }

        // 3. Fetch V14 organs (ports 9095, 9786, etc.)
        for (port, name) in [
            (9095, "v14_fusion"), (9786, "chronos"),
            (9040, "foresight"), (9041, "homeostasis"), (9042, "creativity"),
            (9043, "social"), (9044, "validation"), (9047, "nla_explain"),
        ] {
            let c = client.clone();
            set.spawn(async move {
                let res = c.get(format!("http://127.0.0.1:{}/api/stats", port)).send().await;
                if let Ok(r) = res {
                    if r.status().is_success() {
                        if let Ok(json) = r.json::<serde_json::Value>().await {
                            return (name, json);
                        }
                    }
                }
                (name, serde_json::Value::Null)
            });
        }

        // 4. Fetch memory (port 9030)
        let c = client.clone();
        set.spawn(async move {
            let res = c.get("http://127.0.0.1:9030/api/stats").send().await;
            if let Ok(r) = res {
                if r.status().is_success() {
                    if let Ok(json) = r.json::<serde_json::Value>().await {
                        return ("neural_memory", json);
                    }
                }
            }
            ("neural_memory", serde_json::Value::Null)
        });

        let mut organs = HashMap::new();
        let mut blackboard = serde_json::Value::Null;

        while let Some(Ok((name, json))) = set.join_next().await {
            if json.is_null() { continue; }
            if name == "blackboard" {
                blackboard = json;
            } else {
                organs.insert(name.to_string(), json);
            }
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

    #[allow(dead_code)]
    pub fn is_healthy(&self) -> bool {
        self.organs.len() >= 10
    }
}
