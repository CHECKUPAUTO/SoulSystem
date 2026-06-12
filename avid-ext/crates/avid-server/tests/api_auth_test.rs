use avid_core::config::Config;
use avid_core::llm::LlmClient;
use avid_core::memory::MemoryStore;
use avid_server::api::AppState;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_auth_middleware_integration() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [{"name": "llama3"}]
        })))
        .mount(&mock_server)
        .await;

    let tmp = tempdir().unwrap();

    let token = "a".repeat(32);
    std::env::set_var("AVID_API_TOKEN", &token);
    std::env::set_var("AVID_HOME", tmp.path().to_str().unwrap());
    std::env::set_var("OLLAMA_URL", mock_server.uri());
    std::env::set_var("OLLAMA_MODEL", "llama3");

    let config = Config::from_env().unwrap();
    let config = Arc::new(config);
    let llm = Arc::new(
        LlmClient::new(config.ollama_url.clone(), config.ollama_model.clone())
            .await
            .unwrap(),
    );
    let memory = Arc::new(MemoryStore::new(&config.soullink_db_path(), 1).unwrap());

    let state = AppState {
        config: config.clone(),
        memory,
        llm,
        headless: None,
    };

    let app = avid_server::api::build_router(state);

    // 1. Test public endpoint
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. Test protected endpoint without token
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/scout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 3. Test protected endpoint with invalid token
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/scout")
                .header("Authorization", "Bearer invalid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 4. Test protected endpoint with valid token
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/scout")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}
