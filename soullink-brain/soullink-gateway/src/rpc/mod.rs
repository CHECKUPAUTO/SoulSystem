//! Gateway RPC method registry — matches the SoulSystem gateway protocol.
//!
//! Each RPC method is a registered handler in a string-keyed registry.
//! Methods are grouped by category (health, config, sessions, etc.).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;

use crate::api::ApiState;
use crate::provider::registry::ProviderRegistry;

/// RPC error type.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("method not found: {0}")]
    NotFound(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
}

impl RpcError {
    pub fn code(&self) -> &str {
        match self {
            Self::NotFound(_) => "METHOD_NOT_FOUND",
            Self::InvalidParams(_) => "INVALID_PARAMS",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Unavailable(_) => "UNAVAILABLE",
        }
    }
}

/// RPC method handler.
#[async_trait]
pub trait MethodHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn call(&self, params: Value, state: &RpcState) -> Result<Value, RpcError>;
}

/// Shared RPC runtime state.
pub struct RpcState {
    pub api: ApiState,
    pub providers: Arc<ProviderRegistry>,
}

/// RPC method registry.
pub struct RpcRegistry {
    methods: RwLock<HashMap<String, Box<dyn MethodHandler>>>,
}

impl Default for RpcRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcRegistry {
    pub fn new() -> Self {
        Self {
            methods: RwLock::new(HashMap::new()),
        }
    }

    /// Register a method handler.
    pub async fn register(&self, handler: Box<dyn MethodHandler>) {
        let name = handler.name().to_string();
        self.methods.write().await.insert(name.clone(), handler);
        info!(method = %name, "RPC method registered");
    }

    /// Call a method by name.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        state: &RpcState,
    ) -> Result<Value, RpcError> {
        let methods = self.methods.read().await;
        match methods.get(method) {
            Some(handler) => handler.call(params, state).await,
            None => Err(RpcError::NotFound(method.to_string())),
        }
    }

    /// List all registered method names.
    pub async fn list_methods(&self) -> Vec<String> {
        self.methods.read().await.keys().cloned().collect()
    }

    /// Build the default registry with all core methods.
    pub async fn build_default(registry: Arc<ProviderRegistry>) -> Self {
        let rpc = Self::new();

        let _ = registry; // l'état RPC est construit par l'appelant au dispatch

        // Health / Diagnostics
        rpc.register(Box::new(HealthHandler)).await;

        // Status
        rpc.register(Box::new(StatusHandler)).await;

        // Config
        rpc.register(Box::new(ConfigGetHandler)).await;

        // Providers
        rpc.register(Box::new(ProvidersListHandler)).await;

        // Sessions
        rpc.register(Box::new(SessionsListHandler)).await;
        rpc.register(Box::new(SessionsGetHandler)).await;

        // System
        rpc.register(Box::new(SystemPresenceHandler)).await;

        // Gateway
        rpc.register(Box::new(GatewayIdentityHandler)).await;

        // Channels
        rpc.register(Box::new(ChannelsStatusHandler)).await;

        // Models
        rpc.register(Box::new(ModelsListHandler)).await;

        info!(
            count = rpc.methods.read().await.len(),
            "RPC registry initialized"
        );
        rpc
    }
}

// ── Handler implementations ──────────────────────────────────────────────

/// `health` — gateway health status.
struct HealthHandler;
#[async_trait]
impl MethodHandler for HealthHandler {
    fn name(&self) -> &str {
        "health"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }
}

/// `status` — full gateway status.
struct StatusHandler;
#[async_trait]
impl MethodHandler for StatusHandler {
    fn name(&self) -> &str {
        "status"
    }
    async fn call(&self, _params: Value, state: &RpcState) -> Result<Value, RpcError> {
        let providers = state.providers.list_providers().await;
        Ok(serde_json::json!({
            "gateway": "soullink-gateway-rs",
            "version": env!("CARGO_PKG_VERSION"),
            "providers": providers,
            "uptime_secs": 0,
        }))
    }
}

/// `config.get` — read gateway config.
struct ConfigGetHandler;
#[async_trait]
impl MethodHandler for ConfigGetHandler {
    fn name(&self) -> &str {
        "config.get"
    }
    async fn call(&self, _params: Value, state: &RpcState) -> Result<Value, RpcError> {
        let providers = state.providers.list_providers().await;
        Ok(serde_json::json!({
            "gateway": {
                "providers": providers,
            }
        }))
    }
}

/// `providers.list` — list LLM providers.
struct ProvidersListHandler;
#[async_trait]
impl MethodHandler for ProvidersListHandler {
    fn name(&self) -> &str {
        "providers.list"
    }
    async fn call(&self, _params: Value, state: &RpcState) -> Result<Value, RpcError> {
        let providers = state.providers.list_providers().await;
        Ok(serde_json::json!({ "providers": providers }))
    }
}

/// `sessions.list` — list active sessions.
struct SessionsListHandler;
#[async_trait]
impl MethodHandler for SessionsListHandler {
    fn name(&self) -> &str {
        "sessions.list"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Ok(serde_json::json!({ "sessions": [] }))
    }
}

/// `sessions.get` — get session details.
struct SessionsGetHandler;
#[async_trait]
impl MethodHandler for SessionsGetHandler {
    fn name(&self) -> &str {
        "sessions.get"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Err(RpcError::Unavailable(
            "session store not yet implemented".into(),
        ))
    }
}

/// `system-presence` — heartbeat / presence.
struct SystemPresenceHandler;
#[async_trait]
impl MethodHandler for SystemPresenceHandler {
    fn name(&self) -> &str {
        "system-presence"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Ok(serde_json::json!({ "present": true }))
    }
}

/// `gateway.identity.get` — gateway identity.
struct GatewayIdentityHandler;
#[async_trait]
impl MethodHandler for GatewayIdentityHandler {
    fn name(&self) -> &str {
        "gateway.identity.get"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "name": "soullink-gateway",
            "version": env!("CARGO_PKG_VERSION"),
            "language": "rust",
        }))
    }
}

/// `channels.status` — list channel statuses.
struct ChannelsStatusHandler;
#[async_trait]
impl MethodHandler for ChannelsStatusHandler {
    fn name(&self) -> &str {
        "channels.status"
    }
    async fn call(&self, _params: Value, _state: &RpcState) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "channels": {
                "telegram": { "status": "configured" },
                "signal": { "status": "stub" },
            }
        }))
    }
}

/// `models.list` — list available models.
struct ModelsListHandler;
#[async_trait]
impl MethodHandler for ModelsListHandler {
    fn name(&self) -> &str {
        "models.list"
    }
    async fn call(&self, _params: Value, state: &RpcState) -> Result<Value, RpcError> {
        let providers = state.providers.list_providers().await;
        Ok(serde_json::json!({ "models": providers }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::registry::ProviderRegistry;

    #[tokio::test]
    async fn test_registry_empty() {
        let rpc = RpcRegistry::new();
        let methods = rpc.list_methods().await;
        assert!(methods.is_empty());
    }

    #[tokio::test]
    async fn test_health_method() {
        let reg = Arc::new(ProviderRegistry::new());
        let rpc = RpcRegistry::build_default(reg.clone()).await;

        let state = RpcState {
            api: ApiState::new(reg.clone()),
            providers: reg.clone(),
        };

        let result = rpc.call("health", serde_json::json!({}), &state).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["status"], "ok");
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let reg = Arc::new(ProviderRegistry::new());
        let rpc = RpcRegistry::build_default(reg.clone()).await;
        let state = RpcState {
            api: ApiState::new(reg.clone()),
            providers: reg.clone(),
        };

        let result = rpc.call("nonexistent", serde_json::json!({}), &state).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RpcError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_methods() {
        let reg = Arc::new(ProviderRegistry::new());
        let rpc = RpcRegistry::build_default(reg.clone()).await;
        let methods = rpc.list_methods().await;

        assert!(methods.contains(&"health".to_string()));
        assert!(methods.contains(&"status".to_string()));
        assert!(methods.contains(&"providers.list".to_string()));
        assert!(methods.contains(&"sessions.list".to_string()));
    }

    #[tokio::test]
    async fn test_config_get() {
        let reg = Arc::new(ProviderRegistry::new());
        let rpc = RpcRegistry::build_default(reg.clone()).await;
        let state = RpcState {
            api: ApiState::new(reg.clone()),
            providers: reg,
        };

        let result = rpc.call("config.get", serde_json::json!({}), &state).await;
        assert!(result.is_ok());
    }
}
