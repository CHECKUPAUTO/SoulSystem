use avid_sandbox::{ExecStatus, Sandbox, SandboxConfig};
use std::collections::HashMap;

#[tokio::test]
async fn hello_world_exits_success() {
    let config = SandboxConfig::default();
    let sandbox = Sandbox::new(config);

    let files = HashMap::from([(
        "main.py".to_string(),
        "print('hello from sandbox')".to_string(),
    )]);
    let result = sandbox.run(&files, "main.py").await.expect("run");
    assert_eq!(result.status, ExecStatus::Success);
    assert!(result.stdout.contains("hello from sandbox"));
    assert_eq!(result.exit_code, Some(0));
}
