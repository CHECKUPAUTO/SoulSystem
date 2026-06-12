use std::sync::Arc;
use tokio::sync::Mutex;
use soul_core::Goal;
use axum::{routing::get, Json, Router, Extension};
use std::net::SocketAddr;

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

    pub async fn start_server(self: Arc<Self>, port: u16) -> anyhow::Result<()> {
        let app = Router::new()
            .route("/report", get(handle_report))
            .layer(Extension(self));

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        println!("Monitor server listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
        Ok(())
    }
}

async fn handle_report(Extension(state): Extension<Arc<MonitorState>>) -> Json<serde_json::Value> {
    Json(state.report_json().await)
}

#[cfg(test)]
mod tests {
    use super::*;


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
