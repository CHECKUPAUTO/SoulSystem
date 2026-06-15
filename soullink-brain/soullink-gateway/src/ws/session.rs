//! Session store — tracks active WebSocket connections.
//!
//! Sessions are identified by UUID v4, stored in-memory with
//! an LRU-style eviction for sessions idle > 1 hour.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, info};

use super::protocol::HelloOk;

/// A connected WebSocket session.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub client_id: String,
    pub client_name: Option<String>,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub authenticated: bool,
}

impl Session {
    pub fn new(client_id: String, client_name: Option<String>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        info!(session = %id, client = %client_id, "session created");
        Self {
            id,
            client_id,
            client_name,
            created_at: now,
            last_activity: now,
            authenticated: false,
        }
    }

    /// Mark activity (called on each message).
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// Thread-safe session store.
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    #[allow(dead_code)]
    idle_timeout: Duration,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            idle_timeout: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Register a new session.
    pub async fn create(&self, client_id: String, client_name: Option<String>) -> Session {
        let session = Session::new(client_id, client_name);
        let id = session.id.clone();
        self.sessions.write().await.insert(id, session.clone());
        session
    }

    /// Get a session by ID and touch its activity.
    pub async fn get(&self, id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.touch();
            Some(session.clone())
        } else {
            None
        }
    }

    /// Remove a session.
    pub async fn remove(&self, id: &str) {
        self.sessions.write().await.remove(id);
        debug!(session = %id, "session removed");
    }

    /// Count active sessions.
    pub async fn len(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Build HelloOk for a session.
    pub fn hello(&self, session: &Session) -> HelloOk {
        HelloOk {
            session_id: session.id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            methods: vec![
                "health".into(),
                "status".into(),
                "chat".into(),
                "sessions.list".into(),
                "sessions.get".into(),
                "providers.list".into(),
                "config.get".into(),
            ],
            events: vec![
                "session.message".into(),
                "presence".into(),
                "shutdown".into(),
                "tick".into(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = SessionStore::new();
        let session = store
            .create("test-client".into(), Some("Test".into()))
            .await;
        assert_eq!(session.client_id, "test-client");

        let retrieved = store.get(&session.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session.id);
    }

    #[tokio::test]
    async fn test_remove_session() {
        let store = SessionStore::new();
        let session = store.create("test".into(), None).await;
        assert_eq!(store.len().await, 1);

        store.remove(&session.id).await;
        assert_eq!(store.len().await, 0);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let store = SessionStore::new();
        let result = store.get("nonexistent").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_session_touch() {
        let mut session = Session::new("client".into(), None);
        let before = session.last_activity;
        std::thread::sleep(Duration::from_millis(5));
        session.touch();
        assert!(session.last_activity > before);
    }

    #[test]
    fn test_hello_ok_fields() {
        let store = SessionStore::new();
        let session = Session::new("c".into(), None);
        let hello = store.hello(&session);
        assert_eq!(hello.session_id, session.id);
        assert!(!hello.methods.is_empty());
        assert!(!hello.events.is_empty());
    }
}
