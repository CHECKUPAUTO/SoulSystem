//! Integration tests for the Bearer auth middleware.
//!
//! Spin up a real axum server on an ephemeral port, then assert that:
//!   * valid token → 200
//!   * bad token → 401
//!   * no token + localhost + cohabitation ON → 200 with warn log
//!   * no token + localhost + cohabitation OFF → 401

use std::net::SocketAddr;

use axum::{extract::ConnectInfo, middleware, routing::get, Router};
use soullink_core::{
    auth::{require_auth, AuthConfig},
    secrets::InternalToken,
};
use tokio::net::TcpListener;

async fn handler(ConnectInfo(_peer): ConnectInfo<SocketAddr>) -> &'static str {
    "ok"
}

async fn spawn_server(cfg: AuthConfig) -> SocketAddr {
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn_with_state(cfg, require_auth));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    addr
}

fn token_hex(byte: u8) -> String {
    let t = InternalToken([byte; 32]);
    t.to_hex()
}

#[tokio::test]
async fn valid_token_passes() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), false);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://{}/", addr))
        .bearer_auth(token_hex(0xAA))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn bad_token_rejected() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), false);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    // Different bytes → different token → must fail
    let resp = client
        .get(format!("http://{}/", addr))
        .bearer_auth(token_hex(0xBB))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn malformed_token_rejected() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), false);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://{}/", addr))
        .bearer_auth("not-hex")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn no_token_localhost_with_cohabitation_passes() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), /* cohabitation: */ true);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://{}/", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn no_token_localhost_without_cohabitation_rejected() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), /* cohabitation: */ false);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://{}/", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn missing_authorization_header_rejected_without_cohabitation() {
    let cfg = AuthConfig::new(InternalToken([0xAA; 32]), false);
    let addr = spawn_server(cfg).await;
    let client = reqwest::Client::new();

    // Not passing bearer_auth at all
    let resp = client
        .get(format!("http://{}/", addr))
        .header("X-Other", "irrelevant")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    assert!(resp
        .headers()
        .get("WWW-Authenticate")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("Bearer"))
        .unwrap_or(false));
}
