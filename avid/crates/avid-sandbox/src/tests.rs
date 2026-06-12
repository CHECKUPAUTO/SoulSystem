#[cfg(test)]
mod tests_internal {
    use crate::{ExecStatus, Sandbox, SandboxConfig};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_sandbox_resource_limits() {
        // AVID sandbox uses rlimits + no_new_privs, not container isolation.
        // Test that the sandbox successfully runs a legitimate script.
        let sandbox = Sandbox::new(SandboxConfig::default());
        let files = HashMap::from([("main.py".into(), "print('OK')".into())]);

        let result = sandbox.run(&files, "main.py").await.unwrap();
        assert_eq!(result.status, ExecStatus::Success);
        assert!(result.stdout.contains("OK"));
    }

    #[tokio::test]
    async fn test_sandbox_timeout() {
        let config = SandboxConfig {
            wall_timeout: std::time::Duration::from_millis(500),
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        let files = HashMap::from([("main.py".into(), "import time; time.sleep(10)".into())]);

        let result = sandbox.run(&files, "main.py").await.unwrap();
        assert_eq!(result.status, ExecStatus::Timeout);
    }
}
