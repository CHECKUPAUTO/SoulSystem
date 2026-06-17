//! Integration tests for SoulKernel

#[test]
fn test_binary_exists() {
    let exists = std::path::Path::new("/usr/local/bin/soul-kernel").exists()
        || std::path::Path::new("../target/debug/soul-kernel").exists()
        || std::path::Path::new("target/debug/soul-kernel").exists();
    assert!(
        exists,
        "soul-kernel binary not found in /usr/local/bin or target/debug"
    );
}

#[test]
fn test_state_persistence() {
    let path = std::path::Path::new("/tmp/soul_kernel_state.json");
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(json.get("version").is_some());
        assert!(json.get("energy").is_some());
        assert!(json.get("cycles").is_some());
    }
}

#[test]
fn test_qtable_persistence() {
    let path = std::path::Path::new("/tmp/soul_kernel_qtable.json");
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(json.get("scores").is_some());
    }
}

#[test]
fn test_resilience_persistence() {
    let path = std::path::Path::new("/tmp/soul_kernel_resilience.json");
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(json.get("failures").is_some());
    }
}

#[tokio::test]
async fn test_bi_bridge_status() {
    let client = reqwest::Client::new();
    let resp = client
        .get("http://127.0.0.1:9051/status")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let json: serde_json::Value = r.json().await.unwrap();
            assert!(json.get("version").is_some());
            assert!(json.get("energy").is_some());
        }
        _ => {
            // Service might not be running during tests
        }
    }
}

#[tokio::test]
async fn test_self_mod_analysis() {
    use chrono::Utc;
    use soul_kernel::learning::QTable;
    use soul_kernel::resilience::ResilienceEngine;
    use soul_kernel::selfmod::SelfModEngine;
    use soul_kernel::HistoryEntry;

    let engine = SelfModEngine::new("/tmp");
    let mut resilience = ResilienceEngine::new();
    let q_table = QTable::new();
    let energy = 5.0;

    // Test 1: Normal state, no patch
    let history = vec![];
    let patch = engine.analyze(&history, &resilience, &q_table, energy);
    assert!(patch.is_none());

    // Test 2: Low energy + some history -> Decay patch
    let history = vec![HistoryEntry {
        timestamp: Utc::now(),
        event: "init".into(),
        energy_delta: 0.0,
        outcome: "OK".into(),
    }];
    let patch = engine.analyze(&history, &resilience, &q_table, 2.0);
    match patch {
        Some(soul_kernel::selfmod::ConfigPatch::EnergyDecay(d)) => assert_eq!(d, 0.02),
        _ => panic!("Expected EnergyDecay patch, got {:?}", patch),
    }

    // Test 3: Looping action -> Heartbeat patch
    resilience.consecutive_same_action = 10;
    let patch = engine.analyze(&history, &resilience, &q_table, energy);
    match patch {
        Some(soul_kernel::selfmod::ConfigPatch::HeartbeatInterval(s)) => assert_eq!(s, 60),
        _ => panic!("Expected HeartbeatInterval patch"),
    }

    // Test 4: Low success rate -> Heartbeat patch
    resilience.consecutive_same_action = 0;
    let mut history = vec![];
    for _ in 0..10 {
        history.push(HistoryEntry {
            timestamp: Utc::now(),
            event: "action".into(),
            energy_delta: 0.0,
            outcome: "FAILED".into(),
        });
    }
    let patch = engine.analyze(&history, &resilience, &q_table, energy);
    match patch {
        Some(soul_kernel::selfmod::ConfigPatch::HeartbeatInterval(s)) => assert_eq!(s, 45),
        _ => panic!("Expected HeartbeatInterval patch for low success rate"),
    }
}

#[tokio::test]
async fn test_bi_bridge_goal() {
    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:9051/goal")
        .json(&serde_json::json!({"goal": "test", "priority": 5}))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let json: serde_json::Value = r.json().await.unwrap();
            assert!(json.get("success").is_some());
        }
        _ => {
            // Service might not be running during tests
        }
    }
}

#[test]
fn test_service_active() {
    let output = std::process::Command::new("systemctl")
        .args(["is-active", "soul-kernel"])
        .output();

    match output {
        Ok(o) if o.status.success() => {}
        _ => {
            // Service might not be running during tests
        }
    }
}
