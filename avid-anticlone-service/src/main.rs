#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

mod api;
mod auth;
mod corpus;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db_path = std::env::var("ANTICLONE_DB_PATH")
        .unwrap_or_else(|_| "/var/lib/avid/anticlone.db".to_string());
    let port: u16 = std::env::var("ANTICLONE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7470);

    let corpus = Arc::new(corpus::Corpus::open(&db_path).context("open corpus")?);
    let state = api::AppState { corpus };
    let router = api::build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("avid-anticlone-service listening on {addr}");
    axum::serve(listener, router).await?;
    Ok(())
}
