use crate::protocol::Role;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

pub type SessionId = String;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub client_id: String,
    pub role: Role,
    pub scopes: Vec<String>,
    pub caps: Vec<String>,
    pub connected_at_ms: i64,
    pub last_activity_ms: Arc<RwLock<i64>>,
    pub event_tx: Option<mpsc::UnboundedSender<Value>>,
}

impl Session {
    pub fn new(client_id: String, role: Role) -> Self {
        let now = now_ms();
        Self {
            id: Uuid::new_v4().to_string(),
            client_id,
            role,
            scopes: vec![],
            caps: vec![],
            connected_at_ms: now,
            last_activity_ms: Arc::new(RwLock::new(now)),
            event_tx: None,
        }
    }

    pub async fn update_activity(&self) {
        *self.last_activity_ms.write().await = now_ms();
    }

    pub async fn is_stale(&self, timeout: Duration) -> bool {
        now_ms() - *self.last_activity_ms.read().await > timeout.as_millis() as i64
    }
}

pub struct SessionManager {
    pub sessions: Arc<DashMap<SessionId, Arc<Session>>>,
    by_client_id: Arc<DashMap<String, SessionId>>,
    stale_timeout: Duration,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            by_client_id: Arc::new(DashMap::new()),
            stale_timeout: Duration::from_secs(300),
        }
    }

    pub fn insert(&self, session: Arc<Session>) {
        if let Some(old) = self.by_client_id.insert(session.client_id.clone(), session.id.clone()) {
            self.sessions.remove(&old);
            info!("Replaced old session {}", old);
        }
        self.sessions.insert(session.id.clone(), session.clone());
    }

    pub async fn cleanup_stale_sessions(&self) {
        let stale: Vec<_> = {
            let sessions = self.sessions.clone();
            let mut stale = Vec::new();
            for entry in sessions.iter() {
                let session = Arc::clone(entry.value());
                if session.is_stale(self.stale_timeout).await {
                    stale.push(entry.key().clone());
                }
            }
            stale
        };

        for id in stale {
            self.sessions.remove(&id);
            warn!("Removed stale session {}", id);
        }
    }

    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.cleanup_stale_sessions().await;
            }
        });
    }
}
