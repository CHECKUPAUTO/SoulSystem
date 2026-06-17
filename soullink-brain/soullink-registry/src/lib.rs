//! soullink-registry — Service registry for multi-node mesh discovery.
//!
//! Provides dynamic service registration and discovery, replacing
//! hardcoded `127.0.0.1` ports with a living service directory.
//!
//! # Architecture
//!
//! ```text
//!   Node A                          Node B
//!   ┌──────────────┐               ┌──────────────┐
//!   │ Orchestrator │               │ Orchestrator │
//!   │  (discovery) │◄──────────────│  (discovery) │
//!   └──────┬───────┘   registry    └──────┬───────┘
//!          │                              │
//!   ┌──────┴───────┐               ┌──────┴───────┐
//!   │ Brain-Science│               │ Brain-Mind   │
//!   │ Brain-Meta   │               │ Brain-Crypto │
//!   └──────────────┘               └──────────────┘
//! ```
//!
//! Organs register on startup. The orchestrator discovers them via the registry.
//! Supports both single-node (in-memory) and multi-node (central) backends.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

// ── Types ───────────────────────────────────────────────────────────────

/// A registered service instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Unique service name (e.g., "brain-science", "organ-memory").
    pub name: String,
    /// Hostname or IP address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Service capabilities/keywords for routing.
    pub capabilities: Vec<String>,
    /// Service version.
    pub version: String,
    /// Node this service runs on.
    pub node_id: String,
    /// When this instance was registered.
    pub registered_at: String,
    /// Health check endpoint path (default: "/api/stats").
    #[serde(default = "default_health_path")]
    pub health_path: String,
}

fn default_health_path() -> String {
    "/api/stats".into()
}

impl ServiceInstance {
    /// Base URL for this service (e.g., "http://192.168.1.10:9010").
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Full health check URL.
    pub fn health_url(&self) -> String {
        format!("{}{}", self.base_url(), self.health_path)
    }
}

/// A node in the mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier (hostname or UUID).
    pub node_id: String,
    /// Hostname or IP address.
    pub host: String,
    /// Port the node's orchestrator listens on.
    pub orchestrator_port: u16,
    /// Node version.
    pub version: String,
    /// When the node was last seen.
    pub last_seen: String,
    /// GPU info if available.
    pub gpu: Option<GpuInfo>,
}

/// GPU info for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub model: String,
    pub vram_total_mib: u64,
    pub driver_version: String,
}

/// Service health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub name: String,
    pub url: String,
    pub healthy: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

// ── Registry ────────────────────────────────────────────────────────────

/// In-memory service registry.
///
/// For single-node mode, this is the authoritative registry.
/// For multi-node mode, a central instance aggregates from all nodes.
pub struct Registry {
    services: DashMap<String, ServiceInstance>,
    nodes: DashMap<String, NodeInfo>,
    /// TTL for service entries (default: 60s).
    ttl: Duration,
}

impl Registry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(60))
    }

    /// Create a registry with a custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            services: DashMap::new(),
            nodes: DashMap::new(),
            ttl,
        }
    }

    // ── Service registration ───────────────────────────────────────────

    /// Register a service instance.
    pub fn register(&self, instance: ServiceInstance) {
        let key = format!("{}:{}:{}", instance.name, instance.node_id, instance.port);
        info!(
            "Registry: registered service {} at {}:{} (node: {})",
            instance.name, instance.host, instance.port, instance.node_id
        );
        self.services.insert(key, instance);
    }

    /// Unregister a service.
    /// Purge les services dont l'enregistrement a dépassé le TTL.
    /// Un service reste vivant en se ré-enregistrant périodiquement
    /// (le `register` agit comme heartbeat). Retourne le nombre purgé.
    pub fn purge_expired(&self) -> usize {
        let now = chrono::Utc::now();
        let ttl = chrono::Duration::from_std(self.ttl).unwrap_or(chrono::Duration::seconds(60));
        let before = self.services.len();
        self.services.retain(|_, svc| {
            match chrono::DateTime::parse_from_rfc3339(&svc.registered_at) {
                Ok(ts) => now.signed_duration_since(ts.with_timezone(&chrono::Utc)) < ttl,
                // Timestamp illisible : on conserve plutôt que de purger à tort.
                Err(_) => true,
            }
        });
        let purged = before - self.services.len();
        if purged > 0 {
            warn!("Registry: purged {} expired service(s)", purged);
        }
        purged
    }

    pub fn unregister(&self, name: &str, node_id: &str, port: u16) -> Option<ServiceInstance> {
        let key = format!("{}:{}:{}", name, node_id, port);
        self.services.remove(&key).map(|(_, v)| v)
    }

    /// Look up a service by name.
    pub fn lookup(&self, name: &str) -> Vec<ServiceInstance> {
        self.services
            .iter()
            .filter(|e| e.value().name == name)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Look up services by capability keyword.
    pub fn lookup_by_capability(&self, capability: &str) -> Vec<ServiceInstance> {
        self.services
            .iter()
            .filter(|e| e.value().capabilities.iter().any(|c| c == capability))
            .map(|e| e.value().clone())
            .collect()
    }

    /// Look up services on a specific node.
    pub fn lookup_by_node(&self, node_id: &str) -> Vec<ServiceInstance> {
        self.services
            .iter()
            .filter(|e| e.value().node_id == node_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get all registered services.
    pub fn all_services(&self) -> Vec<ServiceInstance> {
        self.services.iter().map(|e| e.value().clone()).collect()
    }

    /// Get a service name -> count map.
    pub fn service_summary(&self) -> HashMap<String, usize> {
        let mut summary = HashMap::new();
        for entry in self.services.iter() {
            *summary.entry(entry.value().name.clone()).or_insert(0) += 1;
        }
        summary
    }

    // ── Node registration ──────────────────────────────────────────────

    /// Register a node.
    pub fn register_node(&self, node: NodeInfo) {
        info!(
            "Registry: registered node {} at {}:{}",
            node.node_id, node.host, node.orchestrator_port
        );
        self.nodes.insert(node.node_id.clone(), node);
    }

    /// Look up a node by ID.
    pub fn get_node(&self, node_id: &str) -> Option<NodeInfo> {
        self.nodes.get(node_id).map(|e| e.value().clone())
    }

    /// Get all registered nodes.
    pub fn all_nodes(&self) -> Vec<NodeInfo> {
        self.nodes.iter().map(|e| e.value().clone()).collect()
    }

    /// Remove a node (and optionally its services).
    pub fn remove_node(&self, node_id: &str, remove_services: bool) -> Option<NodeInfo> {
        if remove_services {
            self.services.retain(|_, v| v.node_id != node_id);
        }
        self.nodes.remove(node_id).map(|(_, v)| v)
    }

    // ── Cleanup ────────────────────────────────────────────────────────

    /// Remove services that haven't been re-registered within the TTL.
    ///
    /// Alias for [`Self::purge_expired`], which already implements the
    /// heartbeat-based eviction (a `register` refreshes the entry).
    pub fn cleanup_stale(&self) -> usize {
        self.purge_expired()
    }

    /// Serialize the registry state (for multi-node sync).
    pub fn serialize_state(&self) -> Result<String, serde_json::Error> {
        let state: Vec<ServiceInstance> = self.all_services();
        serde_json::to_string(&state)
    }

    /// Deserialize and merge registry state from another node.
    pub fn merge_state(&self, json: &str) -> Result<usize, serde_json::Error> {
        let services: Vec<ServiceInstance> = serde_json::from_str(json)?;
        let count = services.len();
        for instance in services {
            self.register(instance);
        }
        Ok(count)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Convenience builders ────────────────────────────────────────────────

/// Create a ServiceInstance for a brain.
pub fn brain_instance(domain: &str, host: &str, port: u16, node_id: &str) -> ServiceInstance {
    ServiceInstance {
        name: format!("brain-{}", domain),
        host: host.to_string(),
        port,
        capabilities: vec![domain.to_string()],
        version: "13.5.0".into(),
        node_id: node_id.to_string(),
        registered_at: chrono::Utc::now().to_rfc3339(),
        health_path: "/api/stats".into(),
    }
}

/// Create a ServiceInstance for an organ.
pub fn organ_instance(name: &str, host: &str, port: u16, node_id: &str) -> ServiceInstance {
    ServiceInstance {
        name: format!("organ-{}", name),
        host: host.to_string(),
        port,
        capabilities: vec![name.to_string()],
        version: "13.5.0".into(),
        node_id: node_id.to_string(),
        registered_at: chrono::Utc::now().to_rfc3339(),
        health_path: "/api/stats".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let reg = Registry::new();

        let svc = ServiceInstance {
            name: "brain-science".into(),
            host: "192.168.1.10".into(),
            port: 9010,
            capabilities: vec!["physics".into(), "math".into()],
            version: "1.0".into(),
            node_id: "node-a".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        };

        reg.register(svc.clone());
        assert_eq!(reg.lookup("brain-science").len(), 1);
        assert_eq!(reg.lookup("brain-science")[0].host, "192.168.1.10");
    }

    #[test]
    fn test_cleanup_stale_purges_expired() {
        let reg = Registry::new(); // default TTL 60s
        reg.register(ServiceInstance {
            name: "old-svc".into(),
            host: "10.0.0.9".into(),
            port: 9099,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "node-z".into(),
            // Far in the past → well beyond the 60s TTL.
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/health".into(),
        });
        assert_eq!(reg.lookup("old-svc").len(), 1);
        let purged = reg.cleanup_stale();
        assert_eq!(purged, 1, "stale service should be purged, not ignored");
        assert_eq!(reg.lookup("old-svc").len(), 0);
    }

    #[test]
    fn test_lookup_by_capability() {
        let reg = Registry::new();

        reg.register(ServiceInstance {
            name: "brain-science".into(),
            host: "10.0.0.1".into(),
            port: 9010,
            capabilities: vec!["physics".into()],
            version: "1.0".into(),
            node_id: "node-a".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });

        reg.register(ServiceInstance {
            name: "brain-mind".into(),
            host: "10.0.0.2".into(),
            port: 9011,
            capabilities: vec!["neuroscience".into()],
            version: "1.0".into(),
            node_id: "node-b".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });

        let physics = reg.lookup_by_capability("physics");
        assert_eq!(physics.len(), 1);
        assert_eq!(physics[0].name, "brain-science");
    }

    #[test]
    fn test_node_registration() {
        let reg = Registry::new();

        reg.register_node(NodeInfo {
            node_id: "node-a".into(),
            host: "192.168.1.10".into(),
            orchestrator_port: 9020,
            version: "1.0".into(),
            last_seen: "2026-01-01T00:00:00Z".into(),
            gpu: None,
        });

        assert!(reg.get_node("node-a").is_some());
        assert!(reg.get_node("node-b").is_none());
    }

    #[test]
    fn test_remove_node_cascades() {
        let reg = Registry::new();

        reg.register_node(NodeInfo {
            node_id: "node-a".into(),
            host: "10.0.0.1".into(),
            orchestrator_port: 9020,
            version: "1.0".into(),
            last_seen: "2026-01-01T00:00:00Z".into(),
            gpu: None,
        });

        reg.register(ServiceInstance {
            name: "brain-science".into(),
            host: "10.0.0.1".into(),
            port: 9010,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "node-a".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });

        assert_eq!(reg.all_services().len(), 1);
        reg.remove_node("node-a", true);
        assert_eq!(reg.all_services().len(), 0);
        assert!(reg.get_node("node-a").is_none());
    }

    #[test]
    fn test_serialize_merge() {
        let reg1 = Registry::new();
        reg1.register(ServiceInstance {
            name: "svc1".into(),
            host: "10.0.0.1".into(),
            port: 8080,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "node-a".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });

        let json = reg1.serialize_state().unwrap();

        let reg2 = Registry::new();
        let merged = reg2.merge_state(&json).unwrap();
        assert_eq!(merged, 1);
        assert_eq!(reg2.all_services().len(), 1);
    }

    #[test]
    fn test_service_summary() {
        let reg = Registry::new();
        reg.register(ServiceInstance {
            name: "brain-science".into(),
            host: "10.0.0.1".into(),
            port: 9010,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "n1".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });
        reg.register(ServiceInstance {
            name: "brain-science".into(),
            host: "10.0.0.2".into(),
            port: 9010,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "n2".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });
        reg.register(ServiceInstance {
            name: "brain-mind".into(),
            host: "10.0.0.1".into(),
            port: 9011,
            capabilities: vec![],
            version: "1.0".into(),
            node_id: "n1".into(),
            registered_at: "2026-01-01T00:00:00Z".into(),
            health_path: "/api/stats".into(),
        });

        let summary = reg.service_summary();
        assert_eq!(summary.get("brain-science"), Some(&2));
        assert_eq!(summary.get("brain-mind"), Some(&1));
    }
}
