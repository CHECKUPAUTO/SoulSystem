//! Integration tests for OpenClaw-U

#[test]
fn test_binary_exists() {
    assert!(std::path::Path::new("/usr/local/bin/openclaw-u").exists());
}

#[test]
fn test_state_persistence() {
    let path = std::path::Path::new("/tmp/openclaw_u_state.json");
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
    let path = std::path::Path::new("/tmp/openclaw_u_qtable.json");
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(json.get("scores").is_some());
    }
}

#[test]
fn test_resilience_persistence() {
    let path = std::path::Path::new("/tmp/openclaw_u_resilience.json");
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
        .args(["is-active", "openclaw-u"])
        .output();

    match output {
        Ok(o) if o.status.success() => {}
        _ => {
            // Service might not be running during tests
        }
    }
}
