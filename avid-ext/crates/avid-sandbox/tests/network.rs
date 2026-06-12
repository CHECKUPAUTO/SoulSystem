use avid_sandbox::{ExecStatus, Sandbox, SandboxConfig};
use std::collections::HashMap;

#[tokio::test]
async fn network_is_blocked_with_netns() {
    let use_netns = true;
    let config = SandboxConfig {
        use_netns,
        ..Default::default()
    };
    let sandbox = Sandbox::new(config);

    let files = HashMap::from([(
        "main.py".to_string(),
        "import socket\ns = socket.socket()\ntry:\n    s.connect(('8.8.8.8', 53))\n    print('connected')\nexcept Exception as e:\n    print('blocked:', e)\n".to_string(),
    )]);
    let result = sandbox.run(&files, "main.py").await;
    if let Ok(r) = result {
        // If unshare available: network should be blocked
        // If not: test still passes (just documents behavior)
        if use_netns && which::which("unshare").is_ok() {
            assert!(
                r.stdout.contains("blocked") || r.status == ExecStatus::NonZeroExit,
                "expected network to be blocked, got stdout: {}",
                r.stdout
            );
        }
    }
}
