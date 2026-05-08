//! Domain allowlist for network access control.
//! Ported from IronClaw sandbox/proxy/allowlist.rs.
//!
//! Supports exact matches and wildcard patterns like `*.example.com`.

use std::collections::HashSet;
use serde::{Deserialize, Serialize};

/// Pattern for matching allowed domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPattern {
    pattern: String,
    is_wildcard: bool,
    base_domain: String,
}

impl DomainPattern {
    pub fn new(pattern: &str) -> Self {
        let is_wildcard = pattern.starts_with("*.");
        let base_domain = if is_wildcard {
            pattern[2..].to_lowercase()
        } else {
            pattern.to_lowercase()
        };
        Self { pattern: pattern.to_string(), is_wildcard, base_domain }
    }

    /// Check if a host matches this pattern.
    pub fn matches(&self, host: &str) -> bool {
        let host_lower = host.to_lowercase();
        if self.is_wildcard {
            host_lower == self.base_domain || host_lower.ends_with(&format!(".{}", self.base_domain))
        } else {
            host_lower == self.base_domain
        }
    }
}

/// Network allowlist with domain patterns and blocked domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAllowlist {
    /// Allowed domain patterns.
    allowed: Vec<DomainPattern>,
    /// Explicitly blocked domains (takes precedence).
    blocked: HashSet<String>,
    /// Whether to allow localhost connections.
    allow_localhost: bool,
    /// Whether to allow private IP ranges.
    allow_private_ips: bool,
}

impl Default for NetworkAllowlist {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            blocked: HashSet::new(),
            allow_localhost: false,
            allow_private_ips: false,
        }
    }
}

impl NetworkAllowlist {
    pub fn new() -> Self { Self::default() }

    pub fn allow(mut self, pattern: &str) -> Self {
        self.allowed.push(DomainPattern::new(pattern));
        self
    }

    pub fn block(mut self, domain: &str) -> Self {
        self.blocked.insert(domain.to_lowercase());
        self
    }

    pub fn allow_localhost(mut self, allow: bool) -> Self { self.allow_localhost = allow; self }
    pub fn allow_private_ips(mut self, allow: bool) -> Self { self.allow_private_ips = allow; self }

    /// Check if a URL is allowed.
    pub fn is_allowed(&self, url: &str) -> Result<bool, AllowlistError> {
        let parsed = url::Url::parse(url).map_err(|e| AllowlistError::InvalidUrl(e.to_string()))?;
        let host = parsed.host_str().unwrap_or("");

        // Check blocked first
        if self.blocked.contains(&host.to_lowercase()) {
            return Ok(false);
        }

        // Check localhost
        if is_localhost(host) {
            return Ok(self.allow_localhost);
        }

        // Check private IPs
        if is_private_ip(host) {
            return Ok(self.allow_private_ips);
        }

        // Check allowlist
        if self.allowed.is_empty() {
            return Ok(true); // empty allowlist = allow all (except blocked)
        }
        Ok(self.allowed.iter().any(|p| p.matches(host)))
    }
}

fn is_localhost(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

fn is_private_ip(host: &str) -> bool {
    host.starts_with("10.") || host.starts_with("172.16.") || host.starts_with("192.168.") || host.starts_with("fd")
}

#[derive(Debug, thiserror::Error)]
pub enum AllowlistError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let p = DomainPattern::new("api.example.com");
        assert!(p.matches("api.example.com"));
        assert!(!p.matches("other.example.com"));
    }

    #[test]
    fn wildcard_match() {
        let p = DomainPattern::new("*.example.com");
        assert!(p.matches("api.example.com"));
        assert!(p.matches("sub.example.com"));
        assert!(p.matches("example.com")); // base domain matches
        assert!(!p.matches("other.org"));
    }

    #[test]
    fn allowlist_check() {
        let list = NetworkAllowlist::new()
            .allow("api.openai.com")
            .allow("*.huggingface.co")
            .block("evil.huggingface.co")
            .allow_localhost(true);

        assert!(list.is_allowed("https://api.openai.com/v1/chat").unwrap());
        assert!(list.is_allowed("https://models.huggingface.co/bert").unwrap());
        assert!(!list.is_allowed("https://evil.huggingface.co/x").unwrap());
        assert!(list.is_allowed("http://localhost:11434/api").unwrap());
        assert!(!list.is_allowed("https://random.com/evil").unwrap());
    }

    #[test]
    fn empty_allowlist_allows_all() {
        let list = NetworkAllowlist::new();
        assert!(list.is_allowed("https://any-domain.com").unwrap());
    }

    #[test]
    fn private_ip_blocked() {
        let list = NetworkAllowlist::new();
        assert!(!list.is_allowed("http://192.168.1.1:8080").unwrap());
    }
}
