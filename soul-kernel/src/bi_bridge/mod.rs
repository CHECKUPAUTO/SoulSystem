//! Bi-Bridge — Connexion bidirectionnelle HTTP avec SoulLink Orchestrator

pub mod http_server;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UplinkMessage {
    StatusReport {
        energy: f64,
        cycles: u64,
        tasks: u64,
        version: String,
    },
    ActionResult {
        action: String,
        success: bool,
        output: String,
    },
    Alert {
        severity: String,
        message: String,
    },
    Evolution {
        patch_count: u64,
        last_result: String,
    },
    GoalUpdate {
        current: String,
        queue: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownlinkMessage {
    SetGoal { goal: String, priority: u8 },
    ExecuteCommand { command: String },
    SetEnergy { value: f64 },
    RequestStatus,
    InjectCode { file: String, code: String },
    Pause,
    Resume,
}

pub struct BiBridge {
    pub uplink_tx: mpsc::Sender<UplinkMessage>,
    pub downlink_rx: mpsc::Receiver<DownlinkMessage>,
    soullink_api: String,
    client: reqwest::Client,
    session_id: String,
}

impl BiBridge {
    pub fn new(
        uplink_tx: mpsc::Sender<UplinkMessage>,
        downlink_rx: mpsc::Receiver<DownlinkMessage>,
        soullink_api: &str,
        session_id: &str,
    ) -> Self {
        Self {
            uplink_tx,
            downlink_rx,
            soullink_api: soullink_api.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            session_id: session_id.to_string(),
        }
    }

    pub async fn send_status(&self, energy: f64, cycles: u64, tasks: u64, version: &str) {
        let msg = UplinkMessage::StatusReport {
            energy,
            cycles,
            tasks,
            version: version.to_string(),
        };
        let _ = self.uplink_tx.send(msg).await;

        // Also POST to SoulLink API
        let payload = serde_json::json!({
            "source": "soul-kernel",
            "type": "status",
            "session_id": self.session_id,
            "energy": energy,
            "cycles": cycles,
            "tasks": tasks,
            "version": version,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.post_to_soullink("api/notify", payload).await;
    }

    pub async fn send_alert(&self, severity: &str, message: &str) {
        let msg = UplinkMessage::Alert {
            severity: severity.to_string(),
            message: message.to_string(),
        };
        let _ = self.uplink_tx.send(msg).await;

        let payload = serde_json::json!({
            "source": "soul-kernel",
            "type": "alert",
            "session_id": self.session_id,
            "severity": severity,
            "message": message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.post_to_soullink("api/notify", payload).await;
        info!("📡 ALERT → SoulLink [{}]: {}", severity, message);
    }

    #[allow(dead_code)]
    pub async fn send_goal_update(&self, current: &str, queue: &[String]) {
        let msg = UplinkMessage::GoalUpdate {
            current: current.to_string(),
            queue: queue.to_vec(),
        };
        let _ = self.uplink_tx.send(msg).await;

        let payload = serde_json::json!({
            "source": "soul-kernel",
            "type": "goals",
            "session_id": self.session_id,
            "current": current,
            "queue": queue,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = self.post_to_soullink("api/notify", payload).await;
    }

    pub async fn poll_downlink(&mut self) -> Option<DownlinkMessage> {
        // Try channel first (fast path)
        if let Ok(Some(msg)) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            self.downlink_rx.recv(),
        )
        .await
        {
            return Some(msg);
        }

        // Then poll SoulLink HTTP API for commands
        match self.poll_soullink_commands().await {
            Ok(Some(cmd)) => Some(cmd),
            _ => None,
        }
    }

    async fn poll_soullink_commands(&self) -> Result<Option<DownlinkMessage>, String> {
        let url = format!(
            "{}/api/commands?session={}",
            self.soullink_api, self.session_id
        );
        let resp = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("HTTP: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON: {}", e))?;

        if let Some(cmds) = body.get("commands").and_then(|c| c.as_array()) {
            for cmd in cmds {
                if let Some(action) = cmd.get("action").and_then(|a| a.as_str()) {
                    let parsed = match action {
                        "set_goal" => {
                            let goal = cmd
                                .get("goal")
                                .and_then(|g| g.as_str())
                                .unwrap_or("")
                                .to_string();
                            let priority =
                                cmd.get("priority").and_then(|p| p.as_u64()).unwrap_or(5) as u8;
                            Some(DownlinkMessage::SetGoal { goal, priority })
                        }
                        "execute" => {
                            let command = cmd
                                .get("command")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(DownlinkMessage::ExecuteCommand { command })
                        }
                        "set_energy" => {
                            let value = cmd.get("value").and_then(|v| v.as_f64()).unwrap_or(5.0);
                            Some(DownlinkMessage::SetEnergy { value })
                        }
                        "pause" => Some(DownlinkMessage::Pause),
                        "resume" => Some(DownlinkMessage::Resume),
                        "inject_code" => {
                            let file = cmd
                                .get("file")
                                .and_then(|f| f.as_str())
                                .unwrap_or("")
                                .to_string();
                            let code = cmd
                                .get("code")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(DownlinkMessage::InjectCode { file, code })
                        }
                        "status" => Some(DownlinkMessage::RequestStatus),
                        _ => None,
                    };
                    if parsed.is_some() {
                        return Ok(parsed);
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn post_to_soullink(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let url = format!("{}/{}", self.soullink_api, endpoint);
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP: {}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }

    #[allow(dead_code)]
    pub async fn notify_telegram(&self, message: &str) {
        let payload = serde_json::json!({
            "session_id": self.session_id,
            "message": message,
            "source": "soul-kernel",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        match self.post_to_soullink("api/notify", payload).await {
            Ok(_) => info!("📨 Telegram notification sent"),
            Err(e) => warn!("❌ Telegram notify failed: {}", e),
        }
    }
}
