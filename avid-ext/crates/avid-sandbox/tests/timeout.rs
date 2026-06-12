use avid_sandbox::{ExecStatus, Sandbox, SandboxConfig};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::test]
async fn infinite_loop_times_out() {
    let config = SandboxConfig {
        wall_timeout: Duration::from_secs(2),
        cpu_limit_secs: 2,
        ..Default::default()
    };
    let sandbox = Sandbox::new(config);

    let files = HashMap::from([("main.py".to_string(), "while True:\n    pass\n".to_string())]);
    let result = sandbox.run(&files, "main.py").await.expect("run");
    // Either Timeout or KilledBySignal (CPU rlimit) should be triggered
    // Any of Timeout, KilledBySignal, or NonZeroExit (RLIMIT_CPU kills via signal)
    // is acceptable — the process must not return Success
    assert!(
        !matches!(result.status, ExecStatus::Success),
        "expected sandbox to kill the process, got {:?}",
        result.status
    );
}
