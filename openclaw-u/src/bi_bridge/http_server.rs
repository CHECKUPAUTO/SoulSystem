//! Bi-Bridge HTTP Server — Mini API pour recevoir les commandes de SoulLink

use axum::{
    Router,
    routing::{get, post},
    extract::State,
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::CoreState;
use crate::bi_bridge::DownlinkMessage;

#[derive(Clone)]
pub struct BridgeState {
    pub downlink_tx: mpsc::Sender<DownlinkMessage>,
    pub core_state: Arc<Mutex<CoreState>>,
}

#[derive(Debug, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    #[allow(dead_code)]
    pub priority: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct GoalRequest {
    pub goal: String,
    pub priority: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct InjectRequest {
    pub file: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub energy: f64,
    pub cycles: u64,
    pub tasks: u64,
    pub evolutions: u64,
    pub goals: Vec<String>,
    pub uptime_seconds: i64,
    pub llm_action: String,
}

#[derive(Debug, Serialize)]
pub struct SimpleResponse {
    pub success: bool,
    pub message: String,
}

pub async fn start_bridge_server(
    state: BridgeState,
    port: u16,
) {
    let app = Router::new()
        .route("/status", get(get_status))
        .route("/command", post(post_command))
        .route("/goal", post(post_goal))
        .route("/pause", post(post_pause))
        .route("/resume", post(post_resume))
        .route("/inject", post(post_inject))
        .route("/energy", post(post_energy))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    info!("🌐 Bi-Bridge HTTP serveur démarré sur http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!("❌ Impossible de binder {}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        warn!("❌ Erreur serveur Bi-Bridge: {}", e);
    }
}

async fn get_status(
    State(state): State<BridgeState>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let st = state.core_state.lock().await;
    Ok(Json(StatusResponse {
        version: st.version.clone(),
        energy: st.energy,
        cycles: st.uptime_cycles,
        tasks: st.task_count,
        evolutions: st.evolution_count,
        goals: st.goals.clone(),
        uptime_seconds: (chrono::Utc::now() - st.birth_time).num_seconds(),
        llm_action: st.last_llm_action.clone(),
    }))
}

async fn post_command(
    State(state): State<BridgeState>,
    Json(req): Json<CommandRequest>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    let msg = DownlinkMessage::ExecuteCommand {
        command: req.command.clone(),
    };
    match state.downlink_tx.send(msg).await {
        Ok(_) => {
            info!("📥 COMMAND reçue: {}", req.command);
            Ok(Json(SimpleResponse {
                success: true,
                message: format!("Commande '{}' enregistrée", req.command),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi commande: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn post_goal(
    State(state): State<BridgeState>,
    Json(req): Json<GoalRequest>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    let msg = DownlinkMessage::SetGoal {
        goal: req.goal.clone(),
        priority: req.priority.unwrap_or(5),
    };
    match state.downlink_tx.send(msg).await {
        Ok(_) => {
            info!("📥 GOAL reçu: {} (prio:{})", req.goal, req.priority.unwrap_or(5));
            Ok(Json(SimpleResponse {
                success: true,
                message: format!("But '{}' enregistré", req.goal),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi goal: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn post_pause(
    State(state): State<BridgeState>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    match state.downlink_tx.send(DownlinkMessage::Pause).await {
        Ok(_) => {
            info!("📥 PAUSE reçue");
            Ok(Json(SimpleResponse {
                success: true,
                message: "Pause demandée".to_string(),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi pause: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn post_resume(
    State(state): State<BridgeState>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    match state.downlink_tx.send(DownlinkMessage::Resume).await {
        Ok(_) => {
            info!("📥 RESUME reçu");
            Ok(Json(SimpleResponse {
                success: true,
                message: "Resume demandé".to_string(),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi resume: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn post_inject(
    State(state): State<BridgeState>,
    Json(req): Json<InjectRequest>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    let msg = DownlinkMessage::InjectCode {
        file: req.file.clone(),
        code: req.code.clone(),
    };
    match state.downlink_tx.send(msg).await {
        Ok(_) => {
            info!("📥 INJECT reçu: {}", req.file);
            Ok(Json(SimpleResponse {
                success: true,
                message: format!("Injection '{}' enregistrée", req.file),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi inject: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EnergyRequest {
    pub value: f64,
}

async fn post_energy(
    State(state): State<BridgeState>,
    Json(req): Json<EnergyRequest>,
) -> Result<Json<SimpleResponse>, StatusCode> {
    let msg = DownlinkMessage::SetEnergy {
        value: req.value.clamp(0.0, 10.0),
    };
    match state.downlink_tx.send(msg).await {
        Ok(_) => {
            info!("📥 ENERGY reçu: {:.1}", req.value);
            Ok(Json(SimpleResponse {
                success: true,
                message: format!("Énergie forcée à {:.1}", req.value),
            }))
        }
        Err(e) => {
            warn!("❌ Échec envoi energy: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}
