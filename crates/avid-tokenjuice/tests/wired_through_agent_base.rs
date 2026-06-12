//! Anti-regression: TokenJuicedLlm intercepts LLM calls via ChatBackend trait.
//! Proves that TokenJuice counters increment when chat() is called through the trait.
use avid_core::llm::{ChatBackend, LlmClient, LlmError};
use avid_tokenjuice::TokenJuice;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn tokenjuice_tracks_chat_via_trait() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "content": "hello from mock" },
            "done": true
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .mount(&server)
        .await;

    let llm = LlmClient::new(server.uri(), "test-model".into())
        .await
        .unwrap();
    let juice = TokenJuice::new(1_000_000);
    let tracked = juice.wrap_llm(Arc::new(llm));
    let backend: &dyn ChatBackend = &tracked;

    let before = juice.stats();
    assert_eq!(before.call_count, 0);

    // Call via trait — this is what AgentBase::chat_and_parse does internally
    let result = backend
        .chat("sys", "user msg", false, Duration::from_secs(5))
        .await;
    assert!(result.is_ok());

    let after = juice.stats();
    assert!(
        after.call_count >= 1,
        "TokenJuice did not see the call (count={})",
        after.call_count
    );
    assert!(after.total_input > 0, "zero input tokens tracked");
    assert!(after.total_output > 0, "zero output tokens tracked");
    assert_eq!(after.total_tokens, after.total_input + after.total_output);
}

#[tokio::test]
async fn budget_exhausted_returns_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .mount(&server)
        .await;

    let llm = LlmClient::new(server.uri(), "test-model".into())
        .await
        .unwrap();
    let juice = TokenJuice::new(1); // tiny budget
    let tracked = juice.wrap_llm(Arc::new(llm));
    let backend: &dyn ChatBackend = &tracked;

    let result = backend
        .chat(
            "sys",
            "this input is longer than one token budget",
            false,
            Duration::from_secs(5),
        )
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        LlmError::BudgetExceeded(_) => {} // expected
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}
