/// Kubernetes client using kubectl CLI.
use std::process::Command;

#[derive(Debug)]
pub struct KubernetesClient;

impl KubernetesClient {
    /// Creates a new Kubernetes client.
    ///
    /// # Errors
    /// Returns an error if `kubectl` is not found in PATH.
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Verify kubectl is available
        let output = Command::new("kubectl")
            .args(["version", "--client", "--short"])
            .output()
            .map_err(|e| format!("kubectl not available: {e}"))?;
        if !output.status.success() {
            return Err("kubectl exited with error".into());
        }
        Ok(Self)
    }

    /// Diagnoses pods in the given namespace.
    ///
    /// Returns a human-readable summary of pod statuses.
    ///
    /// # Errors
    /// Returns an error if the kubectl command fails.
    pub fn diagnose_pods(
        &self,
        namespace: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let output = Command::new("kubectl")
            .args(["get", "pods", "-n", namespace, "-o", "wide", "--no-headers"])
            .output()
            .map_err(|e| format!("failed to run kubectl: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("kubectl get pods failed: {stderr}").into());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(format!("No pods found in namespace '{namespace}'."));
        }

        // Parse pod lines: NAME READY STATUS RESTARTS AGE [IP NODE ...]
        let mut summary = String::from("Pod Status Summary:");
        for line in stdout.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 {
                use std::fmt::Write;
                let name = fields[0];
                let ready = fields.get(1).unwrap_or(&"?");
                let status = fields.get(2).unwrap_or(&"?");
                let restarts = fields.get(3).unwrap_or(&"?");
                let _ = writeln!(summary);
                let _ = write!(
                    summary,
                    "  {name}: Ready={ready} Status={status} Restarts={restarts}"
                );
            }
        }
        Ok(summary)
    }
}
