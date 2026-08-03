//! Connect-handshake authentication — strict replica of the SoulSystem gateway
//! `AuthManager` (see `soulsystem-gateway/src/auth.rs`).
//!
//! A static operator token (from `GATEWAY_TOKEN` / `--token`) authenticates
//! `connect`; on success the gateway mints a 30-day device token that is
//! returned in `hello-ok` and accepted on subsequent reconnects.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use tracing::{debug, info};
use uuid::Uuid;

use super::protocol::Role;
use soulsystem_common::secrets::SecretString;

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: SecretString,
    pub role: Role,
    pub scopes: Vec<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub client_id: String,
}

/// Issues and validates tokens for the connect handshake.
pub struct AuthManager {
    tokens: Arc<DashMap<String, TokenInfo>>,
    config_token: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

impl AuthManager {
    pub fn new(config_token: Option<String>) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            config_token,
        }
    }

    /// True when a static operator token is configured. When false the gateway
    /// runs open (matching SoulSystem's warn-and-allow posture).
    pub fn requires_token(&self) -> bool {
        self.config_token.is_some()
    }

    /// Validate a presented token: the static operator token grants
    /// `operator.admin`; otherwise it must be an unexpired issued device token.
    pub fn validate_token(&self, token: &str) -> Option<TokenInfo> {
        if let Some(ref cfg) = self.config_token {
            if token == cfg {
                return Some(TokenInfo {
                    token: SecretString::new(token),
                    role: Role::Operator,
                    scopes: vec!["operator.admin".into()],
                    created_at_ms: now_ms(),
                    expires_at_ms: None,
                    client_id: "config".into(),
                });
            }
        }

        let entry = self.tokens.get(token)?;
        if let Some(exp) = entry.expires_at_ms {
            if now_ms() > exp {
                drop(entry);
                self.tokens.remove(token);
                return None;
            }
        }
        Some(entry.clone())
    }

    /// Mint a 30-day device token bound to the connecting client.
    pub fn generate_device_token(
        &self,
        role: Role,
        scopes: Vec<String>,
        client_id: String,
    ) -> TokenInfo {
        let token = format!("oc_dev_{}", Uuid::new_v4().as_simple());
        let now = now_ms();
        let info = TokenInfo {
            token: SecretString::new(token.clone()),
            role,
            scopes,
            created_at_ms: now,
            expires_at_ms: Some(now + 30 * 24 * 60 * 60 * 1000),
            client_id,
        };
        self.tokens.insert(token.clone(), info.clone());
        info!("generated device token {}", token);
        info
    }

    pub fn cleanup_expired_tokens(&self) {
        let now = now_ms();
        let expired: Vec<_> = self
            .tokens
            .iter()
            .filter(|e| e.value().expires_at_ms.map(|x| x < now).unwrap_or(false))
            .map(|e| e.key().clone())
            .collect();
        for t in expired {
            self.tokens.remove(&t);
            debug!("removed expired token {}", t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_mode_requires_no_token() {
        let auth = AuthManager::new(None);
        assert!(!auth.requires_token());
    }

    #[test]
    fn config_token_grants_operator() {
        let auth = AuthManager::new(Some("secret".into()));
        let info = auth.validate_token("secret").expect("valid");
        assert!(matches!(info.role, Role::Operator));
        assert!(auth.validate_token("wrong").is_none());
    }

    #[test]
    fn issued_device_token_round_trips() {
        let auth = AuthManager::new(Some("secret".into()));
        let dev = auth.generate_device_token(Role::Operator, vec![], "client-1".into());
        assert!(dev.token.expose().starts_with("oc_dev_"));
        assert!(auth.validate_token(dev.token.expose()).is_some());
    }
}
