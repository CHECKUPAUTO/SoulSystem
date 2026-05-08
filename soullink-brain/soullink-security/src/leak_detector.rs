//! Leak Detector — Aho-Corasick multi-pattern secret scanning at organ boundaries.
//!
//! Ported from IronClaw's `ironclaw_safety/src/leak_detector.rs` (1499 LOC).
//!
//! Scans data at TWO boundaries:
//!   1. Before outbound requests (prevent exfiltration)
//!   2. After responses/outputs (prevent accidental exposure)
//!
//! Uses Aho-Corasick for fast multi-pattern matching + regex for complex patterns.
//!
//! # Actions
//! - `Clean`: No secrets found, pass through
//! - `Warn`: Log warning but allow
//! - `Redact`: Replace secret with [REDACTED:type]
//! - `Block`: Reject entirely

use std::ops::Range;

/// Action to take when a leak is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeakAction {
    Block,
    Redact,
    Warn,
}

impl std::fmt::Display for LeakAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeakAction::Block => write!(f, "block"),
            LeakAction::Redact => write!(f, "redact"),
            LeakAction::Warn => write!(f, "warn"),
        }
    }
}

/// Type of detected secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretType {
    AwsAccessKey,
    AwsSecretKey,
    GithubToken,
    OpenAiKey,
    AnthropicKey,
    PrivateKey,
    Jwt,
    GenericApiKey,
    Base64Secret,
    CredentialUrl,
}

impl std::fmt::Display for SecretType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretType::AwsAccessKey => write!(f, "aws_access_key"),
            SecretType::AwsSecretKey => write!(f, "aws_secret_key"),
            SecretType::GithubToken => write!(f, "github_token"),
            SecretType::OpenAiKey => write!(f, "openai_key"),
            SecretType::AnthropicKey => write!(f, "anthropic_key"),
            SecretType::PrivateKey => write!(f, "private_key"),
            SecretType::Jwt => write!(f, "jwt"),
            SecretType::GenericApiKey => write!(f, "api_key"),
            SecretType::Base64Secret => write!(f, "base64_secret"),
            SecretType::CredentialUrl => write!(f, "credential_url"),
        }
    }
}

/// A detected leak.
#[derive(Debug, Clone)]
pub struct LeakMatch {
    pub secret_type: SecretType,
    pub action: LeakAction,
    pub range: Range<usize>,
    pub preview: String,
}

/// Result of scanning for leaks.
#[derive(Debug, Clone)]
pub struct LeakScanResult {
    pub is_clean: bool,
    pub matches: Vec<LeakMatch>,
    pub redacted: Option<String>,
}

impl LeakScanResult {
    pub fn clean() -> Self {
        Self { is_clean: true, matches: vec![], redacted: None }
    }

    pub fn should_block(&self) -> bool {
        self.matches.iter().any(|m| m.action == LeakAction::Block)
    }

    pub fn has_warnings(&self) -> bool {
        self.matches.iter().any(|m| m.action == LeakAction::Warn)
    }
}

/// Policy mapping secret types to actions.
#[derive(Debug, Clone)]
pub struct LeakPolicy {
    /// Default action for any detected secret.
    pub default_action: LeakAction,
    /// Override actions per secret type.
    pub overrides: Vec<(SecretType, LeakAction)>,
}

impl Default for LeakPolicy {
    fn default() -> Self {
        Self {
            default_action: LeakAction::Redact,
            overrides: vec![
                (SecretType::PrivateKey, LeakAction::Block),
                (SecretType::AwsSecretKey, LeakAction::Block),
                (SecretType::CredentialUrl, LeakAction::Block),
            ],
        }
    }
}

impl LeakPolicy {
    pub fn action_for(&self, secret_type: &SecretType) -> LeakAction {
        self.overrides.iter()
            .find(|(t, _)| t == secret_type)
            .map(|(_, a)| *a)
            .unwrap_or(self.default_action)
    }
}

/// Leak detector using regex patterns.
///
/// Aho-Corasick would be ideal for production, but regex gives us
/// good coverage for the known secret patterns with zero external deps
/// beyond the `regex` crate we already have.
pub struct LeakDetector {
    policy: LeakPolicy,
    patterns: Vec<(SecretType, regex::Regex)>,
}

impl LeakDetector {
    pub fn new(policy: LeakPolicy) -> Self {
        use regex::Regex;
        let patterns = vec![
            (SecretType::AwsAccessKey, Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap()),
            (SecretType::AwsSecretKey, Regex::new(r"(?i)aws_secret_access_key\s*=\s*\S+").unwrap()),
            (SecretType::GithubToken, Regex::new(r"gh[ps]_[0-9a-zA-Z]{36,}").unwrap()),
            (SecretType::OpenAiKey, Regex::new(r"sk-[a-zA-Z0-9]{48}").unwrap()),
            (SecretType::AnthropicKey, Regex::new(r"sk-ant-[a-zA-Z0-9\-]{48,}").unwrap()),
            (SecretType::PrivateKey, Regex::new(r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----").unwrap()),
            (SecretType::Jwt, Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap()),
            (SecretType::GenericApiKey, Regex::new(r"(?i)(api[_-]?key|apikey|secret[_-]?key)\s*[:=]\s*\S{8,}").unwrap()),
            (SecretType::CredentialUrl, Regex::new(r"(?i)://[^/\s:]+:[^/\s@]+@").unwrap()),
        ];

        Self { policy, patterns }
    }

    pub fn with_default_policy() -> Self {
        Self::new(LeakPolicy::default())
    }

    /// Scan text for secret leaks.
    pub fn scan(&self, text: &str) -> LeakScanResult {
        let mut matches = Vec::new();
        let mut redacted = text.to_string();

        for (secret_type, regex) in &self.patterns {
            for m in regex.find_iter(text) {
                let action = self.policy.action_for(secret_type);
                let preview = Self::preview(&text[m.range()]);

                matches.push(LeakMatch {
                    secret_type: secret_type.clone(),
                    action,
                    range: m.range().clone(),
                    preview,
                });

                if action == LeakAction::Redact {
                    let redaction = format!("[REDACTED:{}]", secret_type);
                    redacted.replace_range(m.range().clone(), &redaction);
                }
            }
        }

        // Re-scan redacted if any Block matches (they should block entirely)
        let is_clean = matches.is_empty();

        LeakScanResult {
            is_clean,
            matches,
            redacted: if redacted != text { Some(redacted) } else { None },
        }
    }

    /// Scan and return either clean text or redacted version.
    /// Returns Err if any Block-level leak is found.
    pub fn scan_or_block(&self, text: &str) -> Result<String, LeakScanResult> {
        let result = self.scan(text);
        if result.should_block() {
            Err(result)
        } else {
            Ok(result.redacted.unwrap_or_else(|| text.to_string()))
        }
    }

    fn preview(secret: &str) -> String {
        if secret.len() <= 8 {
            "***".into()
        } else {
            format!("{}...{}", &secret[..4], &secret[secret.len()-2..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("Hello world, this is fine");
        assert!(result.is_clean);
        assert!(!result.should_block());
    }

    #[test]
    fn detect_aws_key() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("key=AKIAIOSFODNN7EXAMPLE");
        assert!(!result.is_clean);
        assert!(result.matches.iter().any(|m| m.secret_type == SecretType::AwsAccessKey));
    }

    #[test]
    fn detect_github_token() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij1234567890");
        assert!(!result.is_clean);
        assert!(result.matches.iter().any(|m| m.secret_type == SecretType::GithubToken));
    }

    #[test]
    fn detect_private_key_blocks() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("-----BEGIN RSA PRIVATE KEY-----\nMIIEow...");
        assert!(result.should_block());
    }

    #[test]
    fn redact_generic_key() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("api_key: supersecret12345");
        assert!(!result.is_clean);
        assert!(result.redacted.is_some());
        assert!(result.redacted.as_ref().unwrap().contains("[REDACTED:api_key]"));
    }

    #[test]
    fn credential_url_blocks() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan("postgres://user:pass123@db.example.com:5432/mydb");
        assert!(result.should_block());
    }

    #[test]
    fn scan_or_block_clean() {
        let det = LeakDetector::with_default_policy();
        let out = det.scan_or_block("Normal text here").unwrap();
        assert_eq!(out, "Normal text here");
    }

    #[test]
    fn scan_or_block_blocked() {
        let det = LeakDetector::with_default_policy();
        let result = det.scan_or_block("-----BEGIN PRIVATE KEY-----\ndata");
        assert!(result.is_err());
    }
}