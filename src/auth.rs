use crate::protocol::Role;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: String,
    pub role: Role,
    pub scopes: Vec<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub client_id: String,
}

pub struct AuthManager {
    tokens: Arc<DashMap<String, TokenInfo>>,
    config_token: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

impl AuthManager {
    pub fn new(config_token: Option<String>) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            config_token,
        }
    }

    pub fn validate_token(&self, token: &str) -> Option<TokenInfo> {
        if let Some(ref cfg) = self.config_token {
            if token == cfg {
                return Some(TokenInfo {
                    token: token.to_string(),
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
                self.tokens.remove(token);
                return None;
            }
        }

        Some(entry.clone())
    }

    pub fn generate_device_token(&self, role: Role, scopes: Vec<String>, client_id: String) -> TokenInfo {
        let token = format!("oc_dev_{}", Uuid::new_v4().as_simple());
        let now = now_ms();
        let expires = now + 30 * 24 * 60 * 60 * 1000;

        let info = TokenInfo {
            token: token.clone(),
            role,
            scopes,
            created_at_ms: now,
            expires_at_ms: Some(expires),
            client_id,
        };

        self.tokens.insert(token.clone(), info.clone());
        info!("Generated device token {}", token);
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
            debug!("Removed expired token {}", t);
        }
    }
}
