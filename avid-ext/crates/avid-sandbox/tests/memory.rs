use avid_sandbox::{ExecStatus, Sandbox, SandboxConfig};
use std::collections::HashMap;

#[tokio::test]
async fn excessive_memory_is_killed() {
    let config = SandboxConfig {
        mem_limit_bytes: 64 * 1024 * 1024,
        ..Default::default()
    };
    let sandbox = Sandbox::new(config);

    // Attempt to allocate ~1 GiB
    let files = HashMap::from([(
        "main.py".to_string(),
        "x = bytearray(1024 * 1024 * 1024)\nprint(len(x))\n".to_string(),
    )]);
    let result = sandbox.run(&files, "main.py").await;
    if let Ok(r) = result {
        assert!(
            matches!(
                r.status,
                ExecStatus::KilledBySignal | ExecStatus::NonZeroExit
            ),
            "expected killed or non-zero, got {:?}",
            r.status
        );
    }
}
