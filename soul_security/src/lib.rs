use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Rate limit exceeded for: {0}")]
    RateLimitExceeded(String),
    #[error("Intrusion detected: {0}")]
    IntrusionDetected(String),
}

// ═══════════════════════════════════════════════════════════════
// INTRUSION DETECTOR
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrusionEvent {
    pub id: String,
    pub event_type: String,
    pub source_ip: String,
    pub details: String,
    pub severity: IntrusionSeverity,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntrusionSeverity {
    Low,
    Medium,
    High,
    Critical,
}

pub struct IntrusionDetector {
    events: Vec<IntrusionEvent>,
    blocked_ips: Vec<String>,
    suspicious_patterns: Vec<String>,
}

impl IntrusionDetector {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            blocked_ips: Vec::new(),
            suspicious_patterns: vec![
                "SQL injection".to_string(),
                "XSS attack".to_string(),
                "brute force".to_string(),
                "unauthorized access".to_string(),
            ],
        }
    }

    pub fn analyze_log(&mut self, log_line: &str, source_ip: &str) -> Option<IntrusionEvent> {
        let log_lower = log_line.to_lowercase();
        for pattern in &self.suspicious_patterns {
            if log_lower.contains(&pattern.to_lowercase()) {
                let event = IntrusionEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    event_type: "suspicious_activity".to_string(),
                    source_ip: source_ip.to_string(),
                    details: log_line.to_string(),
                    severity: IntrusionSeverity::Medium,
                    timestamp: chrono::Utc::now(),
                };
                self.events.push(event.clone());
                return Some(event);
            }
        }
        None
    }

    pub fn block_ip(&mut self, ip: &str) {
        if !self.blocked_ips.contains(&ip.to_string()) {
            self.blocked_ips.push(ip.to_string());
        }
    }

    pub fn is_blocked(&self, ip: &str) -> bool {
        self.blocked_ips.contains(&ip.to_string())
    }

    pub fn recent_events(&self, n: usize) -> &[IntrusionEvent] {
        let len = self.events.len();
        let start = len.saturating_sub(n);
        &self.events[start..]
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_events": self.events.len(),
            "blocked_ips": self.blocked_ips.len(),
            "by_severity": self.count_by_severity(),
        })
    }

    fn count_by_severity(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for event in &self.events {
            let key = format!("{:?}", event.severity);
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }
}

impl Default for IntrusionDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// SECRET MANAGER
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub id: String,
    pub name: String,
    pub value_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct SecretManager {
    secrets: HashMap<String, Secret>,
}

impl SecretManager {
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }

    pub fn store_secret(&mut self, name: &str, value: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let hash = format!("{:x}", sha2::Sha256::digest(value.as_bytes()));
        let secret = Secret {
            id: id.clone(),
            name: name.to_string(),
            value_hash: hash,
            created_at: chrono::Utc::now(),
            last_accessed: None,
        };
        self.secrets.insert(id.clone(), secret);
        id
    }

    pub fn verify_secret(&mut self, name: &str, value: &str) -> bool {
        let hash = format!("{:x}", sha2::Sha256::digest(value.as_bytes()));
        if let Some(secret) = self.secrets.values_mut().find(|s| s.name == name) {
            secret.last_accessed = Some(chrono::Utc::now());
            secret.value_hash == hash
        } else {
            false
        }
    }

    pub fn list_secrets(&self) -> Vec<&Secret> {
        self.secrets.values().collect()
    }

    pub fn delete_secret(&mut self, name: &str) -> bool {
        let len = self.secrets.len();
        self.secrets.retain(|_, s| s.name != name);
        self.secrets.len() < len
    }
}

impl Default for SecretManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// AUDIT TRAIL
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub action: String,
    pub actor: String,
    pub target: String,
    pub details: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrail {
    entries: Vec<AuditEntry>,
    max_entries: usize,
}

impl AuditTrail {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn log(&mut self, action: &str, actor: &str, target: &str, details: serde_json::Value) {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            actor: actor.to_string(),
            target: target.to_string(),
            details,
            timestamp: chrono::Utc::now(),
        };
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn search(&self, query: &str) -> Vec<&AuditEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.action.to_lowercase().contains(&query_lower)
                    || e.actor.to_lowercase().contains(&query_lower)
                    || e.target.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn recent(&self, n: usize) -> &[AuditEntry] {
        let len = self.entries.len();
        let start = len.saturating_sub(n);
        &self.entries[start..]
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let data = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }
}

impl Default for AuditTrail {
    fn default() -> Self {
        Self::new(1000)
    }
}

// ═══════════════════════════════════════════════════════════════
// RATE LIMITER
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: usize,
    window_start: chrono::DateTime<chrono::Utc>,
}

pub struct RateLimiter {
    limits: HashMap<String, (usize, u64)>,
    requests: HashMap<String, RateLimitEntry>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limits: HashMap::new(),
            requests: HashMap::new(),
        }
    }

    pub fn set_limit(&mut self, key: &str, max_requests: usize, window_seconds: u64) {
        self.limits.insert(key.to_string(), (max_requests, window_seconds));
    }

    pub fn check(&mut self, key: &str) -> bool {
        let (max_requests, window_seconds) = self.limits.get(key).copied().unwrap_or((100, 60));
        let now = chrono::Utc::now();
        let entry = self.requests.entry(key.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });

        if (now - entry.window_start).num_seconds() > window_seconds as i64 {
            entry.count = 0;
            entry.window_start = now;
        }

        if entry.count >= max_requests {
            return false;
        }

        entry.count += 1;
        true
    }

    pub fn reset(&mut self, key: &str) {
        self.requests.remove(key);
    }

    pub fn remaining(&self, key: &str) -> usize {
        let (max_requests, _) = self.limits.get(key).copied().unwrap_or((100, 60));
        let entry = self.requests.get(key);
        let used = entry.map(|e| e.count).unwrap_or(0);
        max_requests.saturating_sub(used)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// SECURITY ENGINE (ties everything together)
// ═══════════════════════════════════════════════════════════════

pub struct SecurityEngine {
    pub intrusion: IntrusionDetector,
    pub secrets: SecretManager,
    pub audit: AuditTrail,
    pub rate_limiter: RateLimiter,
}

impl SecurityEngine {
    pub fn new() -> Self {
        Self {
            intrusion: IntrusionDetector::new(),
            secrets: SecretManager::new(),
            audit: AuditTrail::new(1000),
            rate_limiter: RateLimiter::new(),
        }
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "intrusion_events": self.intrusion.stats(),
            "secrets_count": self.secrets.list_secrets().len(),
            "audit_entries": self.audit.count(),
        })
    }
}

impl Default for SecurityEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intrusion_detector() {
        let mut detector = IntrusionDetector::new();
        let event = detector.analyze_log("SQL injection attempt detected", "192.168.1.1");
        assert!(event.is_some());
        assert_eq!(detector.recent_events(10).len(), 1);
    }

    #[test]
    fn test_intrusion_detector_clean_log() {
        let mut detector = IntrusionDetector::new();
        let event = detector.analyze_log("normal log message", "192.168.1.1");
        assert!(event.is_none());
    }

    #[test]
    fn test_intrusion_detector_block_ip() {
        let mut detector = IntrusionDetector::new();
        detector.block_ip("10.0.0.1");
        assert!(detector.is_blocked("10.0.0.1"));
        assert!(!detector.is_blocked("10.0.0.2"));
    }

    #[test]
    fn test_secret_manager() {
        let mut sm = SecretManager::new();
        let id = sm.store_secret("api_key", "super_secret_123");
        assert!(!id.is_empty());
        assert!(sm.verify_secret("api_key", "super_secret_123"));
        assert!(!sm.verify_secret("api_key", "wrong_password"));
    }

    #[test]
    fn test_secret_manager_delete() {
        let mut sm = SecretManager::new();
        sm.store_secret("key", "value");
        assert!(sm.delete_secret("key"));
        assert!(!sm.delete_secret("nonexistent"));
    }

    #[test]
    fn test_audit_trail() {
        let mut audit = AuditTrail::new(100);
        audit.log("test", "user", "system", serde_json::json!({"detail": "test"}));
        assert_eq!(audit.count(), 1);
        let recent = audit.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].action, "test");
    }

    #[test]
    fn test_audit_trail_search() {
        let mut audit = AuditTrail::new(100);
        audit.log("login", "user1", "auth", serde_json::json!({}));
        audit.log("logout", "user2", "auth", serde_json::json!({}));
        let results = audit.search("login");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_rate_limiter() {
        let mut rl = RateLimiter::new();
        rl.set_limit("api", 3, 60);
        assert!(rl.check("api"));
        assert!(rl.check("api"));
        assert!(rl.check("api"));
        assert!(!rl.check("api"));
        assert_eq!(rl.remaining("api"), 0);
    }

    #[test]
    fn test_rate_limiter_reset() {
        let mut rl = RateLimiter::new();
        rl.set_limit("api", 1, 60);
        assert!(rl.check("api"));
        assert!(!rl.check("api"));
        rl.reset("api");
        assert!(rl.check("api"));
    }

    #[test]
    fn test_security_engine_status() {
        let engine = SecurityEngine::new();
        let status = engine.status();
        assert_eq!(status["secrets_count"], 0);
        assert_eq!(status["audit_entries"], 0);
    }

    #[test]
    fn test_audit_trail_save_load() {
        let mut audit = AuditTrail::new(100);
        audit.log("test_action", "user1", "target1", serde_json::json!({"key": "value"}));
        audit.log("other_action", "user2", "target2", serde_json::json!({}));
        let path = "/tmp/soulsystem_test_audit.json";
        audit.save(path).unwrap();
        let loaded = AuditTrail::load(path).unwrap();
        assert_eq!(loaded.count(), 2);
        assert_eq!(loaded.recent(10)[0].action, "test_action");
        let _ = std::fs::remove_file(path);
    }
}
