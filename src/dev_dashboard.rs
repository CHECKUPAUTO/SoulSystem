//! Dashboard developpeur accessible via `--dev`.
//!
//! Sert une page HTML avec donnees temps reel (SSE):
//! - /events          → messages du bus
//! - /events/hnn      → ticks HNN (periodique)
//! - /events/modules  → etat des modules
//! - /events/audit    → derniers messages d'audit

use crate::audit_log::AuditLog;
use crate::bus::{Bus, Message};
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html,
    },
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::info;

#[derive(Clone)]
struct DashboardState {
    bus: Arc<Bus>,
    audit: Arc<Mutex<AuditLog>>,
    hnn_ticks: Arc<std::sync::atomic::AtomicU64>,
}

/// Demarre le serveur HTTP du dashboard sur `127.0.0.1:9090`.
pub async fn run(bus: Arc<Bus>, audit: Arc<Mutex<AuditLog>>) -> anyhow::Result<()> {
    let state = DashboardState {
        bus,
        audit,
        hnn_ticks: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Generateur de ticks HNN simules (1/sec)
    let ticks = state.hnn_ticks.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/events", get(sse_handler))
        .route("/events/hnn", get(hnn_sse_handler))
        .route("/events/modules", get(modules_sse_handler))
        .route("/events/audit", get(audit_sse_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await?;
    info!("Dashboard developpeur : http://127.0.0.1:9090");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn index_handler() -> Result<Html<String>, StatusCode> {
    Ok(Html(INDEX_HTML.to_string()))
}

/// Bus messages SSE
async fn sse_handler(
    State(state): State<DashboardState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx: broadcast::Receiver<Message> = state.bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        let event = match result {
            Ok(msg) => {
                let data = match msg {
                    Message::HnnStatus { organ: _, status, energy } => {
                        serde_json::json!({"type": "hnn_status", "status": status, "energy": energy})
                    }
                    Message::SynergyDetection { module, description } => {
                        serde_json::json!({"type": "synergy", "module": module, "description": description})
                    }
                    Message::AvidDiscovery { topic, summary } => {
                        serde_json::json!({"type": "avid", "topic": topic, "summary": summary})
                    }
                    Message::EvolveOptimization { generation, best_fitness } => {
                        serde_json::json!({"type": "evolve", "generation": generation, "best_fitness": best_fitness})
                    }
                    Message::Custom { topic, payload } => {
                        serde_json::json!({"type": "custom", "topic": topic, "payload": payload})
                    }
                };
                Event::default().data(serde_json::to_string(&data).unwrap_or_default())
            }
            Err(_) => Event::default().data(r#"{"type":"lag"}"#),
        };
        Some(Ok(event))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// HNN ticks SSE (1/sec)
async fn hnn_sse_handler(
    State(state): State<DashboardState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let ticks = state.hnn_ticks;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(8);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let t = ticks.load(std::sync::atomic::Ordering::Relaxed);
            let data = serde_json::json!({"type": "hnn_tick", "total_ticks": t});
            let event = Event::default().data(serde_json::to_string(&data).unwrap_or_default());
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

/// Module status SSE (every 5s)
async fn modules_sse_handler(
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(4);

    tokio::spawn(async move {
        let modules = vec![
            serde_json::json!({"name": "bus", "status": "online", "description": "Message bus"}),
            serde_json::json!({"name": "soul_memory", "status": "online", "description": "Vector memory"}),
            serde_json::json!({"name": "audit_log", "status": "online", "description": "Audit log"}),
            serde_json::json!({"name": "discovery", "status": "online", "description": "mDNS discovery"}),
            serde_json::json!({"name": "telemetry", "status": "online", "description": "OTLP telemetry"}),
            serde_json::json!({"name": "code_signing", "status": "online", "description": "Code signing"}),
            serde_json::json!({"name": "compute_backend", "status": "online", "description": "CPU/GPU backend"}),
            serde_json::json!({"name": "config", "status": "online", "description": "Configuration"}),
            serde_json::json!({"name": "anomaly", "status": "online", "description": "Anomaly detector"}),
        ];
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let data = serde_json::json!({"type": "modules", "modules": modules});
            let event = Event::default().data(serde_json::to_string(&data).unwrap_or_default());
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

/// Audit log SSE (every 3s)
async fn audit_sse_handler(
    State(state): State<DashboardState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let audit = state.audit;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(8);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            let entries = {
                let a = audit.lock().await;
                a.get_recent(20).unwrap_or_default()
            };
            let data = serde_json::json!({
                "type": "audit_entries",
                "entries": entries.iter().map(|e| serde_json::json!({
                    "timestamp": e.timestamp,
                    "module": e.module,
                    "action": e.action,
                    "details": e.details,
                })).collect::<Vec<_>>()
            });
            let event = Event::default().data(serde_json::to_string(&data).unwrap_or_default());
            if tx.send(Ok(event)).await.is_err() {
                break;
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SoulSystem Dev Dashboard</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: 'Courier New', monospace; background: #0a0a1a; color: #e0e0ff; padding: 1.5rem; }
        h1 { color: #22f; margin-bottom: 1rem; font-size: 1.3rem; }
        .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
        .card { background: #111133; border: 1px solid #333366; border-radius: 8px; padding: 1rem; }
        .card h2 { color: #22f; font-size: 0.95rem; margin-bottom: 0.5rem; }
        .value { font-size: 2rem; color: #66f; }
        .messages { max-height: 200px; overflow-y: auto; font-size: 0.8rem; }
        .messages p { padding: 0.2rem 0; border-bottom: 1px solid #222244; }
        .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 0.5rem; }
        .status-dot.online { background: #0f0; }
        .status-dot.offline { background: #f00; }
        table { width: 100%; border-collapse: collapse; font-size: 0.8rem; }
        th, td { text-align: left; padding: 0.3rem 0.5rem; border-bottom: 1px solid #222244; }
        th { color: #66f; }
        canvas { width: 100%; height: 120px; background: #0a0a1a; border-radius: 4px; }
        button { background: #222266; color: #e0e0ff; border: 1px solid #4444aa; padding: 0.4rem 1rem; border-radius: 4px; cursor: pointer; font-family: inherit; }
        button:hover { background: #333388; }
        .full-width { grid-column: 1 / -1; }
    </style>
</head>
<body>
    <h1>SoulSystem Dev Dashboard</h1>
    <div class="grid">
        <div class="card">
            <h2>Ticks HNN</h2>
            <div class="value" id="ticks">--</div>
            <p>ticks/seconde</p>
            <canvas id="tickChart" style="margin-top:0.5rem;"></canvas>
        </div>
        <div class="card">
            <h2>Modules</h2>
            <table id="modulesTable">
                <tr><th>Module</th><th>Etat</th><th>Description</th></tr>
            </table>
        </div>
        <div class="card full-width">
            <h2>Messages bus</h2>
            <div class="messages" id="messages">
                <p>En attente de messages...</p>
            </div>
        </div>
        <div class="card full-width">
            <h2>Audit Log</h2>
            <div class="messages" id="auditLog">
                <p>En attente...</p>
            </div>
        </div>
    </div>

    <script>
        // Tick chart
        const canvas = document.getElementById('tickChart');
        const ctx = canvas.getContext('2d');
        const tickHistory = new Array(60).fill(0);
        let tickIdx = 0;

        function drawChart() {
            const w = canvas.width = canvas.clientWidth;
            const h = canvas.height = canvas.clientHeight;
            ctx.fillStyle = '#0a0a1a';
            ctx.fillRect(0, 0, w, h);
            const barW = w / tickHistory.length;
            const maxVal = Math.max(1, ...tickHistory);
            for (let i = 0; i < tickHistory.length; i++) {
                const x = i * barW;
                const barH = (tickHistory[i] / maxVal) * h * 0.9;
                ctx.fillStyle = '#22f';
                ctx.fillRect(x + 1, h - barH, barW - 2, barH);
            }
        }

        // HNN ticks
        const hnnSource = new EventSource('/events/hnn');
        let lastTicks = 0;
        hnnSource.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                const delta = data.total_ticks - lastTicks;
                lastTicks = data.total_ticks;
                document.getElementById('ticks').textContent = delta;
                tickHistory[tickIdx % 60] = delta;
                tickIdx++;
                drawChart();
            } catch(e) {}
        };

        // Modules
        const modSource = new EventSource('/events/modules');
        modSource.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                const table = document.getElementById('modulesTable');
                table.innerHTML = '<tr><th>Module</th><th>Etat</th><th>Description</th></tr>';
                for (const m of data.modules) {
                    const dot = m.status === 'online' ? 'online' : 'offline';
                    table.innerHTML += '<tr><td><span class="status-dot ' + dot + '"></span>' + m.name + '</td><td>' + m.status + '</td><td style="color:#888">' + m.description + '</td></tr>';
                }
            } catch(e) {}
        };

        // Audit log
        const auditSource = new EventSource('/events/audit');
        auditSource.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                const div = document.getElementById('auditLog');
                div.innerHTML = '';
                for (const e of data.entries) {
                    const ts = new Date(e.timestamp * 1000).toLocaleTimeString();
                    div.innerHTML += '<p>' + ts + ' [' + e.module + '] ' + e.action + ': ' + e.details + '</p>';
                }
            } catch(e) {}
        };

        // Bus messages
        const evtSource = new EventSource('/events');
        evtSource.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                const msgDiv = document.getElementById('messages');
                const p = document.createElement('p');
                p.textContent = new Date().toLocaleTimeString() + ' — ' + event.data;
                msgDiv.prepend(p);
                while (msgDiv.children.length > 50) msgDiv.removeChild(msgDiv.lastChild);
            } catch(e) {}
        };
    </script>
</body>
</html>"#;
