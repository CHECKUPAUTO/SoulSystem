//! Kubernetes diagnoser — extracted from avid-core for dependency hygiene.
//!
//! Consumers who don't need K8s introspection no longer pay the
//! transitive cost of pulling `kube` + `k8s-openapi` (~150 crates).
//!
//! # Example
//!
//! ```no_run
//! # async fn ex() -> anyhow::Result<()> {
//! use avid_k8s::K8sDiagnoser;
//! let d = K8sDiagnoser::try_new().await?;
//! let summary = d.diagnose_pods("default").await?;
//! println!("{summary}");
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]

use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client, ResourceExt};
use tracing::{error, info};

#[derive(Debug, thiserror::Error)]
pub enum K8sError {
    #[error("kubeconfig: {0}")]
    Config(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("operation timed out after {0}ms")]
    Timeout(u64),
}

/// Lightweight Kubernetes diagnoser.
#[derive(Clone)]
pub struct K8sDiagnoser {
    client: Client,
}

impl std::fmt::Debug for K8sDiagnoser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("K8sDiagnoser").finish_non_exhaustive()
    }
}

impl K8sDiagnoser {
    /// Build a diagnoser from the ambient kubeconfig.
    ///
    /// # Errors
    /// Returns `K8sError::Config` if no kubeconfig is reachable.
    pub async fn try_new() -> Result<Self, K8sError> {
        let client = Client::try_default()
            .await
            .map_err(|e| K8sError::Config(e.to_string()))?;
        Ok(Self { client })
    }

    /// Diagnose pods in `namespace` and return a human-readable summary.
    ///
    /// # Errors
    /// Returns an error if pod listing fails or times out.
    pub async fn diagnose_pods(&self, namespace: &str) -> Result<String, K8sError> {
        info!(namespace, "diagnosing pods");
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let pod_list =
            match tokio::time::timeout(Duration::from_secs(10), pods.list(&ListParams::default()))
                .await
            {
                Ok(Ok(list)) => list,
                Ok(Err(e)) => {
                    error!("failed to list pods: {e}");
                    return Err(K8sError::Api(e.to_string()));
                }
                Err(_) => {
                    error!("pod listing timed out");
                    return Err(K8sError::Timeout(10_000));
                }
            };

        let mut summary = Vec::new();
        for p in pod_list {
            let name = p.name_any();
            let phase = p
                .status
                .as_ref()
                .and_then(|s| s.phase.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            summary.push(format!("- {name}: {phase}"));
        }

        Ok(if summary.is_empty() {
            format!("no pods in namespace {namespace}")
        } else {
            summary.join("\n")
        })
    }
}

#[cfg(test)]
mod tests {
    // Note: real K8s integration tests would require a kind/minikube
    // cluster in CI. We only test that the type system holds together
    // here — runtime behavior is exercised via manual smoke tests.

    use super::*;

    #[test]
    fn error_types_implement_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<K8sError>();
    }
}
