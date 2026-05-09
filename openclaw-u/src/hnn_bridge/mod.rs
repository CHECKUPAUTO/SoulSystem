//! HNN Bridge — lecture du blackboard SoulLink (ports V13)

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnnState {
    pub timestamp: String,
    pub organs: HashMap<String, serde_json::Value>,
    pub blackboard: serde_json::Value,
}

impl HnnState {
    /// Récupère l'état de tous les organes en parallèle via JoinSet.
    /// Bolt ⚡: Utilise un client partagé et parallélise les requêtes pour réduire la latence.
    pub async fn fetch(client: &reqwest::Client) -> Self {
        let mut organs = HashMap::new();
        let mut blackboard = serde_json::Value::Null;
        let mut set = JoinSet::new();

        // 1. Fetch orchestrator blackboard (port 9020)
        let c = client.clone();
        set.spawn(async move {
            let res = c.get("http://127.0.0.1:9020/api/stats")
                .timeout(std::time::Duration::from_secs(3))
                .send().await;

            match res {
                Ok(r) if r.status().is_success() => {
                    let json = r.json::<serde_json::Value>().await.ok();
                    ("blackboard".to_string(), json)
                }
                _ => ("blackboard".to_string(), None)
            }
        });

        // 2. Fetch HNN V13 organs (ports 9010-9015)
        let v13_ports = [
            (9010, "science"), (9011, "mind"), (9012, "engineer"),
            (9013, "crypto"), (9014, "creative"), (9015, "meta"),
        ];
        for (port, name) in v13_ports {
            let c = client.clone();
            let name = name.to_string();
            set.spawn(async move {
                let res = c.get(format!("http://127.0.0.1:{}/api/stats", port))
                    .timeout(std::time::Duration::from_secs(2))
                    .send().await;
                match res {
                    Ok(r) if r.status().is_success() => {
                        let json = r.json::<serde_json::Value>().await.ok();
                        (name, json)
                    }
                    _ => (name, None)
                }
            });
        }

        // 3. Fetch V14 organs (ports 9095, 9786, etc.)
        let v14_ports = [
            (9095, "v14_fusion"), (9786, "chronos"),
            (9040, "foresight"), (9041, "homeostasis"), (9042, "creativity"),
            (9043, "social"), (9044, "validation"), (9047, "nla_explain"),
        ];
        for (port, name) in v14_ports {
            let c = client.clone();
            let name = name.to_string();
            set.spawn(async move {
                let res = c.get(format!("http://127.0.0.1:{}/api/stats", port))
                    .timeout(std::time::Duration::from_secs(2))
                    .send().await;
                match res {
                    Ok(r) if r.status().is_success() => {
                        let json = r.json::<serde_json::Value>().await.ok();
                        (name, json)
                    }
                    _ => (name, None)
                }
            });
        }

        // 4. Fetch memory (port 9030)
        let c = client.clone();
        set.spawn(async move {
            let res = c.get("http://127.0.0.1:9030/api/stats")
                .timeout(std::time::Duration::from_secs(2))
                .send().await;
            match res {
                Ok(r) if r.status().is_success() => {
                    let json = r.json::<serde_json::Value>().await.ok();
                    ("neural_memory".to_string(), json)
                }
                _ => ("neural_memory".to_string(), None)
            }
        });

        // Collect results
        while let Some(res) = set.join_next().await {
            if let Ok((name, Some(json))) = res {
                if name == "blackboard" {
                    blackboard = json;
                } else {
                    organs.insert(name, json);
                }
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
