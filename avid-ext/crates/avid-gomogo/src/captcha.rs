use crate::config::GomogoConfig;
use actix_web::{web, App, HttpResponse, HttpRequest, HttpServer, Error};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SolveRequest {
    pub image: String,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SolveResponse {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResultResponse {
    pub status: String,
    pub solution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HealthResponse {
    pub status: String,
    pub workers_connected: usize,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct CaptchaTask {
    pub id: String,
    pub image_base64: String,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

/// Ordonnancement par priorité (max-heap)
impl std::cmp::Ord for CaptchaTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}
impl std::cmp::PartialOrd for CaptchaTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl std::cmp::Eq for CaptchaTask {}
impl std::cmp::PartialEq for CaptchaTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

struct ResultEntry {
    solution: String,
    expires_at: DateTime<Utc>,
}

pub struct AppState {
    pending_tasks: Mutex<BinaryHeap<CaptchaTask>>,
    results: Mutex<HashMap<String, ResultEntry>>,
    workers: Mutex<HashMap<String, WorkerHandle>>,
    worker_last_seen: Mutex<HashMap<String, DateTime<Utc>>>,
    worker_counter: Mutex<usize>,
    started_at: DateTime<Utc>,
    task_timeout: Duration,
    result_ttl: Duration,
    max_pending: usize,
}

type WorkerHandle = mpsc::UnboundedSender<CaptchaTask>;

impl AppState {
    pub fn new(task_timeout: Duration, result_ttl: Duration, max_pending: usize) -> Self {
        AppState {
            pending_tasks: Mutex::new(BinaryHeap::new()),
            results: Mutex::new(HashMap::new()),
            workers: Mutex::new(HashMap::new()),
            worker_last_seen: Mutex::new(HashMap::new()),
            worker_counter: Mutex::new(0),
            started_at: Utc::now(),
            task_timeout,
            result_ttl,
            max_pending,
        }
    }

    pub async fn submit_task(&self, image_base64: String, priority: i32) -> String {
        // Validation basique de l'image
        if image_base64.is_empty() || image_base64.len() > 1_000_000 {
            warn!("Image base64 invalide (taille: {})", image_base64.len());
            return String::new();
        }

        let task_id = Uuid::new_v4().to_string();
        let task = CaptchaTask {
            id: task_id.clone(),
            image_base64,
            priority,
            created_at: Utc::now(),
        };
        self.try_assign_or_queue(task).await;
        task_id
    }

    async fn try_assign_or_queue(&self, task: CaptchaTask) {
        let mut workers = self.workers.lock().await;
        let mut dead = Vec::new();
        let mut assigned = false;

        for (wid, sender) in workers.iter() {
            match sender.send(task.clone()) {
                Ok(_) => {
                    assigned = true;
                    self.worker_last_seen
                        .lock()
                        .await
                        .insert(wid.clone(), Utc::now());
                    break;
                }
                Err(_) => {
                    dead.push(wid.clone());
                }
            }
        }

        for wid in dead {
            workers.remove(&wid);
            self.worker_last_seen.lock().await.remove(&wid);
        }
        drop(workers);

        if !assigned {
            let mut pending = self.pending_tasks.lock().await;
            if pending.len() < self.max_pending {
                pending.push(task);
            } else {
                warn!("File d'attente pleine ({}), tâche rejetée", self.max_pending);
            }
        }
    }

    pub async fn distribute_pending(&self) {
        let tasks: Vec<CaptchaTask> = {
            let mut p = self.pending_tasks.lock().await;
            std::mem::take(&mut *p).into_sorted_vec()
        };
        for task in tasks {
            self.try_assign_or_queue(task).await;
        }
    }

    pub async fn register_worker(&self) -> (String, mpsc::UnboundedReceiver<CaptchaTask>) {
        let mut counter = self.worker_counter.lock().await;
        *counter += 1;
        let worker_id = format!("worker-{}", *counter);
        drop(counter);

        let (tx, rx) = mpsc::unbounded_channel();
        {
            self.workers.lock().await.insert(worker_id.clone(), tx);
            self.worker_last_seen
                .lock()
                .await
                .insert(worker_id.clone(), Utc::now());
        }
        self.distribute_pending().await;
        (worker_id, rx)
    }

    pub async fn unregister_worker(&self, worker_id: &str) {
        self.workers.lock().await.remove(worker_id);
        self.worker_last_seen.lock().await.remove(worker_id);
    }

    pub async fn update_worker_heartbeat(&self, worker_id: &str) {
        if let Some(last_seen) = self.worker_last_seen.lock().await.get_mut(worker_id) {
            *last_seen = Utc::now();
        }
    }

    pub async fn store_result(&self, task_id: &str, solution: &str) {
        let expires = Utc::now() + chrono::Duration::from_std(self.result_ttl).unwrap();
        self.results.lock().await.insert(
            task_id.to_string(),
            ResultEntry {
                solution: solution.to_string(),
                expires_at: expires,
            },
        );
    }

    /// Récupère un résultat avec gestion du TTL
    pub async fn get_result(&self, task_id: &str) -> Option<String> {
        let mut results = self.results.lock().await;
        if let Some(entry) = results.get(task_id) {
            if entry.expires_at > Utc::now() {
                return Some(entry.solution.clone());
            } else {
                results.remove(task_id);
            }
        }
        // Vérifier si la tâche est en attente
        let pending = self.pending_tasks.lock().await;
        if pending.iter().any(|t| t.id == task_id) {
            return Some(String::new()); // Indique "processing"
        }
        None
    }

    /// Nettoie les résultats expirés et les tâches trop anciennes
    pub async fn cleanup_expired(&self) {
        let now = Utc::now();

        // Nettoyer les résultats expirés
        self.results.lock().await.retain(|_, v| v.expires_at > now);

        // Nettoyer les tâches trop anciennes (optimisé)
        let timeout_ago = now - chrono::Duration::from_std(self.task_timeout).unwrap();
        let mut pending = self.pending_tasks.lock().await;
        let valid_tasks: Vec<CaptchaTask> = pending
            .drain()
            .filter(|t| t.created_at > timeout_ago)
            .collect();
        *pending = BinaryHeap::from(valid_tasks);

        // Nettoyer les workers zombies (pas de heartbeat depuis 2x task_timeout)
        let zombie_timeout = now - chrono::Duration::from_std(self.task_timeout * 2).unwrap();
        let mut workers = self.workers.lock().await;
        let last_seen = self.worker_last_seen.lock().await;
        let zombie_ids: Vec<String> = last_seen
            .iter()
            .filter(|(_, ts)| **ts < zombie_timeout)
            .map(|(id, _)| id.clone())
            .collect();
        drop(last_seen);
        for id in zombie_ids {
            workers.remove(&id);
            self.worker_last_seen.lock().await.remove(&id);
        }
    }

    pub async fn worker_count(&self) -> usize {
        self.workers.lock().await.len()
    }

    pub async fn pending_count(&self) -> usize {
        self.pending_tasks.lock().await.len()
    }

    pub async fn completed_count(&self) -> usize {
        self.results.lock().await.len()
    }

    pub fn uptime_seconds(&self) -> u64 {
        (Utc::now() - self.started_at).num_seconds() as u64
    }
}

// ═══════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════

async fn worker_ws(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse, Error> {
    let (response, session, mut stream) = actix_ws::handle(&req, body)?;
    let (worker_id, mut task_rx) = state.register_worker().await;
    let mut ws_sender = session.clone();

    // Tâche d'envoi des CAPTCHAs au worker
    let state_send = state.clone();
    let wid_send = worker_id.clone();
    let sender_handle = actix_web::rt::spawn(async move {
        while let Some(task) = task_rx.recv().await {
            let msg = serde_json::json!({
                "type": "captcha",
                "task_id": task.id,
                "image": task.image_base64,
                "priority": task.priority,
            });
            if ws_sender.text(msg.to_string()).await.is_err() {
                break;
            }
        }
        state_send.unregister_worker(&wid_send).await;
    });

    // Tâche de réception des solutions
    let state_recv = state.clone();
    let wid_recv = worker_id.clone();
    let reader_handle = actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = stream.recv().await {
            match msg {
                actix_ws::Message::Text(text) => {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        match data["type"].as_str() {
                            Some("solution") => {
                                let task_id = data["task_id"].as_str().unwrap_or("").to_string();
                                let solution = data["solution"].as_str().unwrap_or("").to_string();
                                state_recv.store_result(&task_id, &solution).await;
                                state_recv.distribute_pending().await;
                            }
                            Some("heartbeat") => {
                                state_recv.update_worker_heartbeat(&wid_recv).await;
                            }
                            _ => {}
                        }
                    }
                }
                actix_ws::Message::Ping(_) => {
                    state_recv.update_worker_heartbeat(&wid_recv).await;
                }
                actix_ws::Message::Close(_) => break,
                _ => {}
            }
        }
        state_recv.unregister_worker(&wid_recv).await;
    });

    tokio::select! {
        _ = sender_handle => {},
        _ = reader_handle => {},
    }

    Ok(response)
}

async fn worker_page() -> HttpResponse {
    let html = r#"<!DOCTYPE html>
<html><head><title>GOMOGO Captcha Worker</title><meta charset="utf-8">
<style>
body{font-family:system-ui;max-width:600px;margin:auto;padding:20px;background:#1a1a2e;color:#e0e0e0}
#captcha-box{border:1px solid #444;padding:15px;margin-top:20px;border-radius:8px;display:none}
#captcha-img{max-width:100%;border-radius:4px}
input{padding:8px;border-radius:4px;border:1px solid #555;background:#2a2a4a;color:#fff;width:200px}
button{padding:8px 20px;border-radius:4px;border:none;background:#00d2ff;color:#000;cursor:pointer;font-weight:bold}
#status{color:#888}
h1{color:#00d2ff}
</style></head><body>
<h1>GOMOGO Captcha Worker</h1>
<p id="status">Connecting...</p>
<div id="captcha-box">
<img id="captcha-img" src="" alt="CAPTCHA"><br><br>
<input type="text" id="solution" placeholder="Enter solution">
<button id="submit-btn">Submit</button></div>
<script>
let ws=new WebSocket(`ws://${location.host}/worker/ws`);
let currentTaskId=null;
ws.onopen=()=>{document.getElementById("status").textContent="Waiting for tasks...";setInterval(()=>ws.send(JSON.stringify({type:"heartbeat"})),30000);};
ws.onmessage=(e)=>{let d=JSON.parse(e.data);
if(d.type==="captcha"){currentTaskId=d.task_id;
document.getElementById("captcha-img").src="data:image/png;base64,"+d.image;
document.getElementById("captcha-box").style.display="block";
document.getElementById("solution").value="";
document.getElementById("status").textContent="Please solve (priority: "+(d.priority||0)+")";}};
ws.onclose=()=>document.getElementById("status").textContent="Disconnected";
document.getElementById("submit-btn").addEventListener("click",()=>{
let s=document.getElementById("solution").value.trim();
if(s&&currentTaskId){ws.send(JSON.stringify({type:"solution",task_id:currentTaskId,solution:s}));
document.getElementById("captcha-box").style.display="none";
document.getElementById("status").textContent="Sent. Waiting...";currentTaskId=null;}});
</script></body></html>"#;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

async fn solve(
    req: web::Json<SolveRequest>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let task_id = state.submit_task(req.image.clone(), req.priority).await;
    if task_id.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Image base64 invalide ou trop volumineuse"
        }));
    }
    HttpResponse::Ok().json(SolveResponse { task_id })
}

async fn get_result(
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let task_id = path.into_inner();
    match state.get_result(&task_id).await {
        Some(solution) if solution.is_empty() => {
            // Tâche en cours
            HttpResponse::Ok().json(ResultResponse {
                status: "processing".to_string(),
                solution: None,
            })
        }
        Some(solution) => {
            // Tâche terminée
            HttpResponse::Ok().json(ResultResponse {
                status: "completed".to_string(),
                solution: Some(solution),
            })
        }
        None => {
            // Tâche inexistante ou expirée
            HttpResponse::NotFound().json(ResultResponse {
                status: "not_found".to_string(),
                solution: None,
            })
        }
    }
}

async fn health(state: web::Data<Arc<AppState>>) -> HttpResponse {
    let workers = state.worker_count().await;
    let pending = state.pending_count().await;
    let completed = state.completed_count().await;
    let uptime = state.uptime_seconds();

    HttpResponse::Ok().json(HealthResponse {
        status: "ok".to_string(),
        workers_connected: workers,
        pending_tasks: pending,
        completed_tasks: completed,
        uptime_seconds: uptime,
    })
}

pub async fn run_server(
    bind: &str,
    port: u16,
    config: &GomogoConfig,
) -> std::io::Result<()> {
    let task_timeout = Duration::from_secs(config.captcha.task_timeout_secs);
    let result_ttl = Duration::from_secs(config.captcha.result_ttl_secs);
    let max_pending = config.captcha.max_pending_tasks;

    let state = Arc::new(AppState::new(task_timeout, result_ttl, max_pending));
    let addr = format!("{bind}:{port}");
    info!("Lancement Captcha Farm sur http://{}", addr);

    // Tâche de nettoyage périodique
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            cleanup_state.cleanup_expired().await;
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/solve", web::post().to(solve))
            .route("/result/{task_id}", web::get().to(get_result))
            .route("/health", web::get().to(health))
            .route("/worker", web::get().to(worker_page))
            .route("/worker/ws", web::get().to(worker_ws))
    })
    .bind(&addr)?
    .run()
    .await
}
