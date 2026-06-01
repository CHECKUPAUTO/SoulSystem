use std::sync::Arc;
use tokio::sync::Mutex;
use soul_core::Goal;

pub struct MonitorState {
    pub goals: Arc<Mutex<Vec<Goal>>>,
    pub recent_episodes: Arc<Mutex<Vec<String>>>,
}

impl MonitorState {
    pub fn new(
        goals: Arc<Mutex<Vec<Goal>>>,
        recent_episodes: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self { goals, recent_episodes }
    }

    pub async fn report_json(&self) -> serde_json::Value {
        let goals = self.goals.lock().await;
        let episodes = self.recent_episodes.lock().await;
        serde_json::json!({
            "goals": goals.iter().map(|g| serde_json::json!({
                "id": g.id,
                "desc": g.description,
                "priority": g.priority,
                "status": format!("{:?}", g.status),
            })).collect::<Vec<_>>(),
            "recent_episodes": episodes.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_core::GoalStatus;

    #[tokio::test]
    async fn test_monitor_report() {
        let state = MonitorState::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        );
        let report = state.report_json().await;
        assert!(report.get("goals").is_some());
    }
}
