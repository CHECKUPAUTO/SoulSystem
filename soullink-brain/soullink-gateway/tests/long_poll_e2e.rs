//! End-to-end test of the long-poll loop against a mocked Telegram server.
//!
//! We spin up a wiremock instance that simulates the Bot API, inject an
//! in-memory orchestrator bridge that returns canned replies, and verify
//! that one `getUpdates` cycle → reply is posted back through `sendMessage`.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::future::BoxFuture;
use soullink_gateway::{
    telegram::{
        client::TelegramClient,
        long_poll::{IncomingMessage, LongPollLoop, OrchestratorBridge},
    },
};
use tokio::sync::broadcast;
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn test_bridge(
    received: Arc<Mutex<Vec<IncomingMessage>>>,
    reply: String,
) -> OrchestratorBridge {
    Arc::new(move |inc: IncomingMessage| -> BoxFuture<'static, Result<String, String>> {
        let received = received.clone();
        let reply = reply.clone();
        Box::pin(async move {
            received.lock().unwrap().push(inc);
            Ok(reply)
        })
    })
}

#[tokio::test]
async fn happy_path_incoming_message_triggers_reply() {
    let server = MockServer::start().await;

    // One getUpdates call returns one message
    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 100,
                "message": {
                    "message_id": 7,
                    "chat": {"id": 42, "type": "private"},
                    "date": 1700000000,
                    "text": "what is 2+2",
                    "from": {"id": 999, "is_bot": false, "first_name": "Alice"}
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Subsequent getUpdates (offset 101+) return empty — keeps the loop alive
    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "result": []
        })))
        .mount(&server)
        .await;

    // sendMessage must be called with the bridge reply
    let send_received = Arc::new(Mutex::new(Vec::new()));
    let send_received_clone = send_received.clone();
    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/sendMessage"))
        .and(body_partial_json(serde_json::json!({ "chat_id": 42 })))
        .respond_with(move |req: &wiremock::Request| {
            send_received_clone
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&req.body).into_owned());
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {"message_id": 8, "chat": {"id": 42, "type": "private"}, "date": 1}
            }))
        })
        .mount(&server)
        .await;

    let bridge_log = Arc::new(Mutex::new(Vec::<IncomingMessage>::new()));
    let bridge = test_bridge(bridge_log.clone(), "The answer is 4".to_string());

    let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let loop_handle = tokio::spawn(async move {
        let lp = LongPollLoop::new(client, bridge, 1, None, None, shutdown_rx);
        lp.run().await;
    });

    // Let the loop consume the one update and reply
    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle).await;

    let bridge_calls = bridge_log.lock().unwrap();
    assert_eq!(bridge_calls.len(), 1, "bridge should have received one message");
    assert_eq!(bridge_calls[0].text, "what is 2+2");
    assert_eq!(bridge_calls[0].chat_id, 42);
    assert_eq!(bridge_calls[0].username, None);

    let sends = send_received.lock().unwrap();
    assert!(!sends.is_empty(), "sendMessage should have been called");
    assert!(sends[0].contains("The answer is 4"), "reply body: {:?}", sends[0]);
    assert!(sends[0].contains("\"reply_to_message_id\":7"));
}

#[tokio::test]
async fn allow_list_rejects_non_listed_chat() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 200,
                "message": {
                    "message_id": 1,
                    "chat": {"id": 666, "type": "group", "title": "stranger group"},
                    "date": 1700000000,
                    "text": "hi"
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "result": []
        })))
        .mount(&server)
        .await;

    // No sendMessage mock — if it gets called, the request will 404 and we'll see it.
    let bridge_log = Arc::new(Mutex::new(Vec::<IncomingMessage>::new()));
    let bridge = test_bridge(bridge_log.clone(), "should not send".to_string());

    let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let handle = tokio::spawn(async move {
        let lp = LongPollLoop::new(
            client,
            bridge,
            1,
            Some(vec![42, 43]),  // 666 NOT in list
            None,
            shutdown_rx,
        );
        lp.run().await;
    });

    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    let bridge_calls = bridge_log.lock().unwrap();
    assert_eq!(bridge_calls.len(), 0,
               "bridge must not be called for chats outside the allow-list");
}

#[tokio::test]
async fn non_text_message_is_ignored() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 300,
                "message": {
                    "message_id": 1,
                    "chat": {"id": 42, "type": "private"},
                    "date": 1700000000
                    // No text — simulates a photo / sticker / voice note
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "result": []
        })))
        .mount(&server)
        .await;

    let bridge_log = Arc::new(Mutex::new(Vec::<IncomingMessage>::new()));
    let bridge = test_bridge(bridge_log.clone(), "never called".to_string());

    let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let handle = tokio::spawn(async move {
        let lp = LongPollLoop::new(client, bridge, 1, None, None, shutdown_rx);
        lp.run().await;
    });

    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    assert_eq!(bridge_log.lock().unwrap().len(), 0,
               "non-text messages must not reach the orchestrator bridge");
}

#[tokio::test]
async fn orchestrator_failure_produces_user_friendly_reply() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 400,
                "message": {
                    "message_id": 1,
                    "chat": {"id": 42, "type": "private"},
                    "date": 1700000000,
                    "text": "anything"
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/getUpdates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true, "result": []
        })))
        .mount(&server)
        .await;

    let send_captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let sc_clone = send_captured.clone();
    Mock::given(method("POST"))
        .and(path("/botTEST_TOKEN/sendMessage"))
        .respond_with(move |req: &wiremock::Request| {
            sc_clone.lock().unwrap().push(String::from_utf8_lossy(&req.body).into());
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {"message_id": 1, "chat": {"id": 42, "type": "private"}, "date": 1}
            }))
        })
        .mount(&server)
        .await;

    // Bridge returns error
    let bridge: OrchestratorBridge = Arc::new(|_inc| -> BoxFuture<'static, Result<String, String>> {
        Box::pin(async { Err("orchestrator timeout".to_string()) })
    });

    let client = TelegramClient::new(server.uri(), "TEST_TOKEN");
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let handle = tokio::spawn(async move {
        let lp = LongPollLoop::new(client, bridge, 1, None, None, shutdown_rx);
        lp.run().await;
    });

    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    let sends = send_captured.lock().unwrap();
    assert!(!sends.is_empty(), "fallback message should have been sent");
    assert!(
        sends[0].contains("Temporary error") || sends[0].contains("try again"),
        "expected user-friendly fallback, got: {:?}",
        sends[0]
    );
}
