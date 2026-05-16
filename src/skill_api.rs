//! API HTTP pour la marketplace de skills.
//!
//! Endpoints :
//! - GET  /skills        — Liste les skills installés
//! - POST /skills/install — Installe un skill depuis une URL

use axum::{extract::State, routing::get, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::skill_marketplace::SkillLoader;

#[derive(Clone)]
pub struct SkillApiState {
    pub loader: Arc<Mutex<SkillLoader>>,
}

#[derive(Debug, Serialize)]
struct SkillListResponse {
    skills: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InstallRequest {
    url: String,
}

pub fn router(state: SkillApiState) -> Router {
    Router::new()
        .route("/skills", get(list_skills))
        .route("/skills/install", post(install_skill))
        .with_state(state)
}

async fn list_skills(State(state): State<SkillApiState>) -> Json<SkillListResponse> {
    let loader = state.loader.lock().await;
    Json(SkillListResponse {
        skills: loader.list(),
    })
}

async fn install_skill(
    State(state): State<SkillApiState>,
    Json(req): Json<InstallRequest>,
) -> Json<serde_json::Value> {
    let mut loader = state.loader.lock().await;
    match loader.install(&req.url) {
        Ok(()) => Json(serde_json::json!({"status": "ok"})),
        Err(e) => Json(serde_json::json!({"status": "error", "message": e.to_string()})),
    }
}
