//! SoulLink Metabolism — HTTP API server for digital energy management.
//! Port 9052

use axum::{extract::State, routing::get, Json, Router};
use soullink_metabolism::Metabolism;
use std::sync::Arc;
use tokio::net::TcpListener;

const PORT: u16 = 9052;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let metabolism = Metabolism::new();

    // Background recharge loop
    let recharge_m = metabolism.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            recharge_m.recharge(10.0).await;
        }
    });

    let app = Router::new()
        .route("/api/metabolism/status", get(status))
        .route("/api/metabolism/stats", get(stats))
        .route("/api/metabolism/history", get(history))
        .route("/api/metabolism/resources", get(resources))
        .with_state(metabolism);

    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT))
        .await
        .unwrap_or_else(|e| panic!("Metabolism bind {}: {}", PORT, e));
    tracing::info!("soullink-metabolism v0.1.0 on :{}", PORT);
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("Metabolism server: {}", e));
}

async fn status(State(m): State<Arc<Metabolism>>) -> Json<serde_json::Value> {
    let budget = m.budget().await;
    Json(serde_json::json!({
        "energy_available": budget.available,
        "energy_capacity": budget.capacity,
        "recharge_rate": budget.recharge_rate,
        "consumption_rate": budget.consumption_rate,
        "low_power_mode": budget.low_power,
        "throttling": m.is_throttling(),
        "sleeping": m.is_sleeping(),
    }))
}

async fn stats(State(m): State<Arc<Metabolism>>) -> Json<serde_json::Value> {
    let s = m.stats().await;
    Json(serde_json::json!({
        "total_operations": s.total_operations,
        "total_energy_spent": s.total_energy_spent,
        "total_tokens": s.total_tokens,
        "avg_cost_per_op": s.avg_cost_per_op,
        "throttle_events": s.throttle_events,
        "sleep_events": s.sleep_events,
    }))
}

async fn history(State(m): State<Arc<Metabolism>>) -> Json<serde_json::Value> {
    let ops = m.recent_ops(50).await;
    Json(serde_json::json!({"history": ops}))
}

async fn resources(State(m): State<Arc<Metabolism>>) -> Json<serde_json::Value> {
    let snap = m.resource_snapshot();
    Json(serde_json::json!(snap))
}
