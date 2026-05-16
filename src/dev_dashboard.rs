//! Dashboard développeur accessible via `--dev`.
//!
//! Sert une page HTML simple avec des mises à jour en temps réel
//! via Server-Sent Events (SSE) alimentées par le Bus de messages.

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
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::info;

/// État partagé du dashboard.
#[derive(Clone)]
struct DashboardState {
    bus: Arc<Bus>,
}

/// Démarre le serveur HTTP du dashboard sur `127.0.0.1:9090`.
pub async fn run(bus: Arc<Bus>) -> anyhow::Result<()> {
    let state = DashboardState { bus };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/events", get(sse_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await?;
    info!("Dashboard développeur : http://127.0.0.1:9090");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Sert la page HTML statique.
async fn index_handler() -> Result<Html<String>, StatusCode> {
    Ok(Html(INDEX_HTML.to_string()))
}

/// Flux SSE poussant les messages du bus vers le client.
async fn sse_handler(
    State(state): State<DashboardState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx: broadcast::Receiver<Message> = state.bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        let event = match result {
            Ok(msg) => {
                let data = match msg {
                    Message::HnnStatus { ticks_per_sec } => {
                        serde_json::json!({"type": "hnn_status", "ticks_per_sec": ticks_per_sec})
                    }
                    Message::SynergyDetection { module, description } => {
                        serde_json::json!({"type": "synergy", "module": module, "description": description})
                    }
                    Message::AvidDiscovery { source, summary } => {
                        serde_json::json!({"type": "avid", "source": source, "summary": summary})
                    }
                    Message::EvolveOptimization { crate_name, score } => {
                        serde_json::json!({"type": "evolve", "crate": crate_name, "score": score})
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

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SoulSystem Dev Dashboard</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: 'Courier New', monospace; background: #0a0a1a; color: #e0e0ff; padding: 2rem; }
        h1 { color: #22f; margin-bottom: 1rem; font-size: 1.5rem; }
        .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
        .card { background: #111133; border: 1px solid #333366; border-radius: 8px; padding: 1rem; }
        .card h2 { color: #22f; font-size: 1rem; margin-bottom: 0.5rem; }
        .value { font-size: 2rem; color: #66f; }
        .messages { max-height: 300px; overflow-y: auto; }
        .messages p { padding: 0.25rem 0; border-bottom: 1px solid #222244; font-size: 0.85rem; }
        .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 0.5rem; }
        .status-dot.online { background: #0f0; }
        .status-dot.offline { background: #f00; }
    </style>
</head>
<body>
    <h1>🦞 SoulSystem Dev Dashboard</h1>
    <div class="grid">
        <div class="card">
            <h2>⚡ Ticks HNN</h2>
            <div class="value" id="ticks">--</div>
            <p>ticks/seconde</p>
        </div>
        <div class="card">
            <h2>🧩 Modules</h2>
            <p><span class="status-dot online"></span> OpenClaw-U</p>
            <p><span class="status-dot online"></span> SoulLink HNN</p>
            <p><span class="status-dot online"></span> Clawd</p>
            <p><span class="status-dot offline" id="avid-status"></span> AVID</p>
            <p><span class="status-dot offline" id="evolve-status"></span> OpenEvolve</p>
        </div>
    </div>
    <div class="card" style="margin-top: 1rem;">
        <h2>📨 Derniers messages</h2>
        <div class="messages" id="messages">
            <p>En attente de messages...</p>
        </div>
    </div>

    <script>
        const evtSource = new EventSource('/events');
        evtSource.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                const msgDiv = document.getElementById('messages');
                const p = document.createElement('p');
                p.textContent = new Date().toLocaleTimeString() + ' — ' + event.data;
                msgDiv.prepend(p);

                if (data.type === 'hnn_status') {
                    document.getElementById('ticks').textContent =
                        (data.ticks_per_sec / 1000).toFixed(0) + 'k';
                }
                if (data.type === 'avid') {
                    document.getElementById('avid-status').className = 'status-dot online';
                }
                if (data.type === 'evolve') {
                    document.getElementById('evolve-status').className = 'status-dot online';
                }

                // Limiter à 50 messages
                while (msgDiv.children.length > 50) {
                    msgDiv.removeChild(msgDiv.lastChild);
                }
            } catch(e) {}
        };
    </script>
</body>
</html>"#;
