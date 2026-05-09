/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! main entry point for soullink-gbrain service.

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use serde::Deserialize;
use anyhow::Result;
use soullink_gbrain::storage::{Database, Entity, Edge};
use soullink_gbrain::search::{HybridSearcher, SearchHit};
use std::path::PathBuf;
use tokio::sync::RwLock;

struct AppState {
    db: Database,
    searcher: RwLock<HybridSearcher>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    top_k: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let db_path = PathBuf::from("gbrain.db");
    let db = Database::open(&db_path)?;
    let mut searcher = HybridSearcher::new(db.clone(), "http://127.0.0.1:9030".to_string());
    searcher.rebuild_bm25()?;

    let state = Arc::new(AppState {
        db: db.clone(),
        searcher: RwLock::new(searcher),
    });

    let app = Router::new()
        .route("/search", get(handle_search))
        .route("/entities", get(handle_entities))
        .route("/graph/:id", get(handle_graph))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 9031));
    tracing::info!("soullink-gbrain listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Json<Vec<SearchHit>> {
    let top_k = query.top_k.unwrap_or(10);
    let searcher = state.searcher.read().await;
    let results = searcher.search(&query.q, top_k).await.unwrap_or_default();
    Json(results)
}

async fn handle_entities(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Entity>> {
    let entities = state.db.get_entities().unwrap_or_default();
    Json(entities)
}

async fn handle_graph(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<Edge>> {
    let edges = state.db.get_edges_for_entity(&id).unwrap_or_default();
    Json(edges)
}
