//! WASM Organ Sandbox Interface for SoulLink Neural Mesh.
//!
//! Ported from SoulSystem Integration's WASM sandbox architecture. Each organ runs in
//! an isolated sandbox with resource limits, network policies, and
//! capability-based permissions.
//!
//! Architecture:
//! - OrganSandbox: Isolated execution environment for each organ
//! - OrganPolicy: Network allowlist, resource limits, capability grants
//! - OrganManager: Lifecycle management (spawn, health, kill, restart)
//! - CircuitBreaker integration for organ health monitoring

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use soullink_circuit::{CircuitBreaker, CircuitBreakerConfig, CircuitState};

// ---------------------------------------------------------------------------
// Organ Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrganId {
    pub name: String,
    pub port: u16,
}

impl OrganId {
    pub fn new(name: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            port,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl std::fmt::Display for OrganId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(:{})", self.name, self.port)
    }
}

// ---------------------------------------------------------------------------
// Organ Policy (from SoulSystem Integration's sandbox/proxy/policy.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganPolicy {
    /// Maximum memory in MB
    pub max_memory_mb: u32,
    /// Maximum CPU time per request in ms
    pub max_cpu_ms: u32,
    /// Maximum concurrent requests
    pub max_concurrent: u32,
    /// Network allowlist (host:port or just host)
    pub network_allowlist: Vec<String>,
    /// Allowed capabilities
    pub capabilities: Vec<OrganCapability>,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Whether the organ can spawn sub-processes
    pub allow_subprocess: bool,
    /// Whether the organ can access the filesystem
    pub allow_filesystem: bool,
}

impl Default for OrganPolicy {
    fn default() -> Self {
        Self {
            max_memory_mb: 128,
            max_cpu_ms: 5000,
            max_concurrent: 4,
            network_allowlist: vec!["127.0.0.1".into()],
            capabilities: vec![OrganCapability::HttpOut, OrganCapability::HealthCheck],
            request_timeout_secs: 30,
            allow_subprocess: false,
            allow_filesystem: false,
        }
    }
}

impl OrganPolicy {
    /// Strict policy for untrusted organs (no network, no filesystem).
    pub fn strict() -> Self {
        Self {
            max_memory_mb: 64,
            max_cpu_ms: 1000,
            max_concurrent: 1,
            network_allowlist: vec![],
            capabilities: vec![OrganCapability::HealthCheck],
            request_timeout_secs: 10,
            allow_subprocess: false,
            allow_filesystem: false,
        }
    }

    /// Trusted policy for core organs (full local access).
    pub fn trusted() -> Self {
        Self {
            max_memory_mb: 512,
            max_cpu_ms: 30000,
            max_concurrent: 16,
            network_allowlist: vec!["127.0.0.1".into(), "localhost".into()],
            capabilities: vec![
                OrganCapability::HttpOut,
                OrganCapability::HealthCheck,
                OrganCapability::FileSystem,
                OrganCapability::Subprocess,
            ],
            request_timeout_secs: 120,
            allow_subprocess: true,
            allow_filesystem: true,
        }
    }

    /// Policy for inference organs (GPU access, relaxed memory).
    pub fn inference() -> Self {
        Self {
            max_memory_mb: 2048,
            max_cpu_ms: 60000,
            max_concurrent: 8,
            network_allowlist: vec![
                "127.0.0.1".into(),
                "localhost".into(),
                "api.openai.com".into(),
                "api.anthropic.com".into(),
            ],
            capabilities: vec![
                OrganCapability::HttpOut,
                OrganCapability::HealthCheck,
                OrganCapability::GpuAccess,
            ],
            request_timeout_secs: 300,
            allow_subprocess: false,
            allow_filesystem: false,
        }
    }

    /// Check if a network target is allowed.
    pub fn is_network_allowed(&self, target: &str) -> bool {
        self.network_allowlist
            .iter()
            .any(|allowed| target.starts_with(allowed.as_str()))
    }

    /// Check if a capability is granted.
    pub fn has_capability(&self, cap: &OrganCapability) -> bool {
        self.capabilities.contains(cap)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrganCapability {
    HttpOut,
    HealthCheck,
    FileSystem,
    Subprocess,
    GpuAccess,
    Crypto,
    Database,
}

// ---------------------------------------------------------------------------
// Organ Health (from SoulSystem Integration's sandbox/detect.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrganHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganStatus {
    pub id: OrganId,
    pub health: OrganHealth,
    pub uptime_secs: u64,
    pub requests_total: u64,
    pub requests_failed: u64,
    pub last_response_ms: u64,
    pub memory_used_mb: u32,
    #[serde(skip)]
    pub circuit_state: CircuitState,
}

// ---------------------------------------------------------------------------
// Organ Sandbox
// ---------------------------------------------------------------------------

pub struct OrganSandbox {
    id: OrganId,
    policy: OrganPolicy,
    circuit: CircuitBreaker,
    started_at: Instant,
    requests_total: u64,
    requests_failed: u64,
}

impl OrganSandbox {
    pub fn new(id: OrganId, policy: OrganPolicy) -> Self {
        let circuit = CircuitBreaker::with_config(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(30),
            half_open_successes_needed: 2,
            call_timeout: Duration::from_secs(policy.request_timeout_secs),
        });

        Self {
            id,
            policy,
            circuit,
            started_at: Instant::now(),
            requests_total: 0,
            requests_failed: 0,
        }
    }

    /// Convenience constructors for common organ types.
    pub fn core_organ(name: &str, port: u16) -> Self {
        Self::new(OrganId::new(name, port), OrganPolicy::trusted())
    }

    pub fn inference_organ(name: &str, port: u16) -> Self {
        Self::new(OrganId::new(name, port), OrganPolicy::inference())
    }

    pub fn strict_organ(name: &str, port: u16) -> Self {
        Self::new(OrganId::new(name, port), OrganPolicy::strict())
    }

    /// Execute a request within the sandbox, respecting circuit breaker and policy.
    pub async fn execute<F, T>(&mut self, f: F) -> Result<T, OrganError>
    where
        F: FnOnce() -> T,
    {
        let state = self.circuit.state().await;
        if state == CircuitState::Open {
            return Err(OrganError::CircuitOpen(self.id.clone()));
        }

        self.requests_total += 1;
        let start = Instant::now();

        let result = f();

        let elapsed = start.elapsed();
        if elapsed > Duration::from_secs(self.policy.request_timeout_secs) {
            self.requests_failed += 1;
            self.circuit.record_failure().await;
            return Err(OrganError::Timeout {
                organ: self.id.clone(),
                elapsed_ms: elapsed.as_millis() as u64,
                limit_ms: self.policy.request_timeout_secs * 1000,
            });
        }

        self.circuit.record_success().await;
        Ok(result)
    }

    /// Execute an async request within the sandbox.
    pub async fn execute_async<F, Fut, T>(&mut self, f: F) -> Result<T, OrganError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let state = self.circuit.state().await;
        if state == CircuitState::Open {
            return Err(OrganError::CircuitOpen(self.id.clone()));
        }

        self.requests_total += 1;
        let start = Instant::now();

        let result = f().await;

        let elapsed = start.elapsed();
        if elapsed > Duration::from_secs(self.policy.request_timeout_secs) {
            self.requests_failed += 1;
            self.circuit.record_failure().await;
            return Err(OrganError::Timeout {
                organ: self.id.clone(),
                elapsed_ms: elapsed.as_millis() as u64,
                limit_ms: self.policy.request_timeout_secs * 1000,
            });
        }

        self.circuit.record_success().await;
        Ok(result)
    }

    /// Record an external failure (e.g. HTTP error from the organ).
    pub async fn record_failure(&mut self) {
        self.requests_failed += 1;
        self.circuit.record_failure().await;
    }

    /// Get current organ status.
    pub async fn status(&self) -> OrganStatus {
        let circuit_state = self.circuit.state().await;
        let health = match circuit_state {
            CircuitState::Closed => OrganHealth::Healthy,
            CircuitState::HalfOpen => OrganHealth::Degraded,
            CircuitState::Open => {
                if self.requests_failed > self.requests_total / 2 {
                    OrganHealth::Dead
                } else {
                    OrganHealth::Unhealthy
                }
            }
        };

        OrganStatus {
            id: self.id.clone(),
            health,
            uptime_secs: self.started_at.elapsed().as_secs(),
            requests_total: self.requests_total,
            requests_failed: self.requests_failed,
            last_response_ms: 0,
            memory_used_mb: 0,
            circuit_state,
        }
    }

    pub fn id(&self) -> &OrganId {
        &self.id
    }

    pub fn policy(&self) -> &OrganPolicy {
        &self.policy
    }
}

// ---------------------------------------------------------------------------
// Organ Manager
// ---------------------------------------------------------------------------

pub struct OrganManager {
    organs: HashMap<String, OrganSandbox>,
}

impl OrganManager {
    pub fn new() -> Self {
        Self {
            organs: HashMap::new(),
        }
    }

    /// Register a core organ (trusted policy).
    pub fn register_core(&mut self, name: &str, port: u16) {
        let sandbox = OrganSandbox::core_organ(name, port);
        self.organs.insert(name.to_string(), sandbox);
    }

    /// Register an inference organ (GPU access, relaxed limits).
    pub fn register_inference(&mut self, name: &str, port: u16) {
        let sandbox = OrganSandbox::inference_organ(name, port);
        self.organs.insert(name.to_string(), sandbox);
    }

    /// Register an organ with custom policy.
    pub fn register(&mut self, name: &str, port: u16, policy: OrganPolicy) {
        let sandbox = OrganSandbox::new(OrganId::new(name, port), policy);
        self.organs.insert(name.to_string(), sandbox);
    }

    /// Get a reference to an organ sandbox.
    pub fn get(&self, name: &str) -> Option<&OrganSandbox> {
        self.organs.get(name)
    }

    /// Get a mutable reference to an organ sandbox.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut OrganSandbox> {
        self.organs.get_mut(name)
    }

    /// List all organ statuses.
    pub async fn statuses(&self) -> Vec<OrganStatus> {
        let mut statuses = Vec::new();
        for sandbox in self.organs.values() {
            statuses.push(sandbox.status().await);
        }
        statuses
    }

    /// Count healthy organs.
    pub async fn healthy_count(&self) -> usize {
        let mut count = 0;
        for sandbox in self.organs.values() {
            let status = sandbox.status().await;
            if status.health == OrganHealth::Healthy {
                count += 1;
            }
        }
        count
    }

    /// Count total organs.
    pub fn total(&self) -> usize {
        self.organs.len()
    }
}

impl Default for OrganManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Organ Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum OrganError {
    CircuitOpen(OrganId),
    Timeout {
        organ: OrganId,
        elapsed_ms: u64,
        limit_ms: u64,
    },
    NetworkDenied {
        organ: OrganId,
        target: String,
    },
    CapabilityDenied {
        organ: OrganId,
        capability: OrganCapability,
    },
    NotFound(String),
}

impl std::fmt::Display for OrganError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrganError::CircuitOpen(id) => write!(f, "Circuit open for organ {}", id),
            OrganError::Timeout {
                organ,
                elapsed_ms,
                limit_ms,
            } => {
                write!(
                    f,
                    "Timeout for organ {} ({}ms > {}ms)",
                    organ, elapsed_ms, limit_ms
                )
            }
            OrganError::NetworkDenied { organ, target } => {
                write!(f, "Network denied for organ {} → {}", organ, target)
            }
            OrganError::CapabilityDenied { organ, capability } => {
                write!(f, "Capability {:?} denied for organ {}", capability, organ)
            }
            OrganError::NotFound(name) => write!(f, "Organ '{}' not found", name),
        }
    }
}

impl std::error::Error for OrganError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organ_id_url() {
        let id = OrganId::new("memory", 9030);
        assert_eq!(id.url(), "http://127.0.0.1:9030");
        assert_eq!(format!("{}", id), "memory(:9030)");
    }

    #[test]
    fn policy_default() {
        let p = OrganPolicy::default();
        assert_eq!(p.max_memory_mb, 128);
        assert!(p.has_capability(&OrganCapability::HttpOut));
        assert!(!p.has_capability(&OrganCapability::GpuAccess));
        assert!(p.is_network_allowed("127.0.0.1"));
        assert!(!p.is_network_allowed("evil.com"));
    }

    #[test]
    fn policy_strict() {
        let p = OrganPolicy::strict();
        assert!(!p.has_capability(&OrganCapability::HttpOut));
        assert!(p.network_allowlist.is_empty());
        assert!(!p.allow_subprocess);
    }

    #[test]
    fn policy_trusted() {
        let p = OrganPolicy::trusted();
        assert!(p.has_capability(&OrganCapability::FileSystem));
        assert!(p.has_capability(&OrganCapability::Subprocess));
        assert!(p.allow_filesystem);
    }

    #[test]
    fn policy_inference() {
        let p = OrganPolicy::inference();
        assert!(p.has_capability(&OrganCapability::GpuAccess));
        assert!(p.is_network_allowed("api.openai.com"));
        assert_eq!(p.max_memory_mb, 2048);
    }

    #[tokio::test]
    async fn sandbox_execute_sync() {
        let mut sandbox = OrganSandbox::core_organ("test", 9000);
        let result = sandbox.execute(|| 42).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn sandbox_execute_async() {
        let mut sandbox = OrganSandbox::core_organ("test", 9000);
        let result = sandbox.execute_async(|| async { 99 }).await;
        assert_eq!(result.unwrap(), 99);
    }

    #[tokio::test]
    async fn sandbox_circuit_open_rejects() {
        let mut sandbox = OrganSandbox::core_organ("test", 9000);
        // Trip the circuit breaker
        for _ in 0..3 {
            sandbox.record_failure().await;
        }
        let result = sandbox.execute(|| 1).await;
        assert!(matches!(result, Err(OrganError::CircuitOpen(_))));
    }

    #[tokio::test]
    async fn sandbox_status_healthy() {
        let sandbox = OrganSandbox::core_organ("memory", 9030);
        let status = sandbox.status().await;
        assert_eq!(status.health, OrganHealth::Healthy);
        assert_eq!(status.id.name, "memory");
        assert_eq!(status.id.port, 9030);
    }

    #[tokio::test]
    async fn manager_register_and_status() {
        let mut mgr = OrganManager::new();
        mgr.register_core("memory", 9030);
        mgr.register_core("reflex", 9035);
        mgr.register_inference("reasoning", 9031);

        assert_eq!(mgr.total(), 3);

        let statuses = mgr.statuses().await;
        assert_eq!(statuses.len(), 3);

        let healthy = mgr.healthy_count().await;
        assert_eq!(healthy, 3);
    }

    #[tokio::test]
    async fn manager_get_mut() {
        let mut mgr = OrganManager::new();
        mgr.register_core("memory", 9030);

        let sandbox = mgr.get("memory").unwrap();
        assert_eq!(sandbox.id().name, "memory");

        let sandbox = mgr.get_mut("memory").unwrap();
        let result = sandbox.execute(|| "hello").await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn organ_error_display() {
        let id = OrganId::new("test", 9000);
        let err = OrganError::CircuitOpen(id.clone());
        assert!(err.to_string().contains("Circuit open"));

        let err = OrganError::NetworkDenied {
            organ: id.clone(),
            target: "evil.com".into(),
        };
        assert!(err.to_string().contains("Network denied"));

        let err = OrganError::CapabilityDenied {
            organ: id,
            capability: OrganCapability::GpuAccess,
        };
        assert!(err.to_string().contains("Capability"));
    }
}
