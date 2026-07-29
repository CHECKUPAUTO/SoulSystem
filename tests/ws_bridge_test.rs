//! Tests d'integration du bridge WebSocket.
//!
//! Verifie la connexion, l'auth, la publication/souscription,
//! et le routage des messages Telegram.
use futures_util::{SinkExt, StreamExt};

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Test: connexion WebSocket sans auth (mode dev).
#[tokio::test]
async fn test_ws_connect_no_auth() {
    use soulsystem::bus::Bus;
    use tokio_tungstenite::connect_async;

    let bus = Arc::new(Bus::new(256));
    // Exercising the relay itself, not authentication: opt in explicitly to the
    // unauthenticated path, which is no longer the default (CRIT-007).
    let config = soulsystem::ws_bridge::WsBridgeConfig {
        listen: "127.0.0.1:19023".to_string(),
        shared_secret: None,
        unauthenticated_access: soulsystem::ws_bridge::UnauthenticatedAccess::Allow,
    };
    tokio::spawn(async move {
        soulsystem::ws_bridge::run_ws_bridge(config, bus).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let url = "ws://127.0.0.1:19023";
    let (mut ws, _) = connect_async(url).await.expect("connexion WS");

    // Envoyer un message auth vide (dev mode = pas d'auth)
    let msg = r#"{"type":"publish","topic":"test","payload":{}}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(msg.into()))
        .await
        .expect("envoi");

    // Le serveur ne doit pas fermer la connexion
    tokio::time::sleep(Duration::from_millis(100)).await;
    ws.close(None).await.ok();
}

/// Test: publication et reception via le bus.
#[tokio::test]
async fn test_ws_publish_subscribe() {
    use soulsystem::bus::{Bus, Message};
    use std::sync::Arc;
    use tokio_tungstenite::connect_async;

    let bus = Arc::new(Bus::new(256));
    let bus_test = bus.clone();

    // Demarrer le bridge en local
    let config = soulsystem::ws_bridge::WsBridgeConfig {
        listen: "127.0.0.1:19020".to_string(),
        shared_secret: None,
        unauthenticated_access: soulsystem::ws_bridge::UnauthenticatedAccess::Allow,
    };
    tokio::spawn(async move {
        soulsystem::ws_bridge::run_ws_bridge(config, bus).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let url = "ws://127.0.0.1:19020";
    let (mut ws, _) = connect_async(url).await.expect("connexion WS");

    // S'abonner au topic test
    let sub = r#"{"type":"subscribe","topic":"test.*"}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(sub.into()))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publier un message sur le bus
    bus_test.publish(Message::Custom {
        topic: "test.hello".to_string(),
        payload: serde_json::json!({"msg": "world"}),
    });

    // Attendre la reception
    let recv = timeout(Duration::from_secs(2), ws.next()).await;
    assert!(recv.is_ok(), "timeout reception");

    if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = recv.unwrap() {
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json valide");
        assert_eq!(parsed["topic"], "test.hello");
        assert_eq!(parsed["payload"]["msg"], "world");
    } else {
        panic!("message texte attendu");
    }

    ws.close(None).await.ok();
}

/// Test: authentification avec token valide.
#[tokio::test]
async fn test_ws_auth_valid() {
    use tokio_tungstenite::connect_async;

    let bus = Arc::new(soulsystem::bus::Bus::new(256));
    let config = soulsystem::ws_bridge::WsBridgeConfig {
        listen: "127.0.0.1:19021".to_string(),
        shared_secret: Some("secret123".to_string()),
        ..Default::default()
    };
    tokio::spawn(async move {
        soulsystem::ws_bridge::run_ws_bridge(config, bus).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let url = "ws://127.0.0.1:19021";
    let (mut ws, _) = connect_async(url).await.expect("connexion");

    // Envoyer le token
    let auth = r#"{"token":"secret123"}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(auth.into()))
        .await
        .unwrap();

    // Attendre auth_ok
    let recv = timeout(Duration::from_secs(2), ws.next()).await;
    assert!(recv.is_ok());

    if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = recv.unwrap() {
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "auth_ok");
    } else {
        panic!("auth_ok attendu");
    }

    ws.close(None).await.ok();
}

/// Test: authentification avec token invalide -> rejet.
#[tokio::test]
async fn test_ws_auth_invalid() {
    use tokio_tungstenite::connect_async;

    let bus = Arc::new(soulsystem::bus::Bus::new(256));
    let config = soulsystem::ws_bridge::WsBridgeConfig {
        listen: "127.0.0.1:19024".to_string(),
        shared_secret: Some("secret123".to_string()),
        ..Default::default()
    };
    tokio::spawn(async move {
        soulsystem::ws_bridge::run_ws_bridge(config, bus).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let url = "ws://127.0.0.1:19024";
    let (mut ws, _) = connect_async(url).await.expect("connexion");

    // Envoyer un mauvais token
    let auth = r#"{"token":"wrong"}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(auth.into()))
        .await
        .unwrap();

    // Attendre auth_error
    let recv = timeout(Duration::from_secs(2), ws.next()).await;
    assert!(recv.is_ok());

    if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = recv.unwrap() {
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "auth_error");
    } else {
        panic!("auth_error attendu");
    }

    // La connexion doit etre fermee — n'importe quel type de fermeture est OK
    let closed = timeout(Duration::from_secs(2), ws.next()).await;
    assert!(closed.is_ok(), "timeout attente fermeture");
    match closed.unwrap() {
        None | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {}
        Some(Err(_)) => {} // reset TCP sans handshake = OK aussi
        other => panic!("attendu fermeture, got {:?}", other),
    }
}

/// Test: commande /status via mock bot (integration clawd).
#[tokio::test]
async fn test_clawd_status_command() {
    // Ce test necessite un mock du bot Telegram (hors scope pour l'instant)
    // On verifie juste que la commande existe et compile
    let _ = soulsystem::clawd::Settings {
        bot_token: "mock".to_string(),
        avid_endpoint: "http://localhost:7878".to_string(),
    };
    // Si on arrive ici, la struct compile
}

/// The default posture — no shared secret configured — must refuse to serve.
///
/// This is the regression CRIT-007 turned on: the handshake used to initialise
/// `authenticated = true` whenever no secret was set, so an unconfigured
/// deployment relayed the whole internal bus to anyone who connected.
#[tokio::test]
async fn test_ws_default_refuses_when_no_secret_configured() {
    use soulsystem::bus::Bus;
    use tokio_tungstenite::connect_async;

    let bus = Arc::new(Bus::new(256));
    let config = soulsystem::ws_bridge::WsBridgeConfig {
        listen: "127.0.0.1:19025".to_string(),
        ..Default::default()
    };
    assert_eq!(
        config.unauthenticated_access,
        soulsystem::ws_bridge::UnauthenticatedAccess::Deny,
        "the default must be Deny"
    );
    tokio::spawn(async move {
        soulsystem::ws_bridge::run_ws_bridge(config, bus).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The listener still binds (so the misconfiguration is visible), but the
    // connection must not become a usable WebSocket session.
    match timeout(
        Duration::from_secs(2),
        connect_async("ws://127.0.0.1:19025"),
    )
    .await
    {
        // Refused before/at the handshake: the expected outcome.
        Ok(Err(_)) => {}
        // If the handshake somehow completed, the stream must be dead
        // immediately — no message may be exchanged.
        Ok(Ok((mut ws, _))) => {
            let next = timeout(Duration::from_secs(2), ws.next()).await;
            match next {
                Ok(None) | Ok(Some(Err(_))) => {}
                Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))) => {}
                other => panic!(
                    "unconfigured bridge must not serve a live session, got {:?}",
                    other
                ),
            }
        }
        Err(_) => panic!("connect attempt hung; expected a prompt refusal"),
    }
}
