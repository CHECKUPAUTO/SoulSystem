//! Known secret patterns — regex detectors for common credential types.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Severity of a finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// A known secret pattern.
#[derive(Debug)]
pub struct SecretPattern {
    pub name: &'static str,
    pub pattern: Regex,
    pub severity: Severity,
    pub verifiable: bool,
}

/// All known secret patterns.
pub static SECRET_PATTERNS: LazyLock<Vec<SecretPattern>> = LazyLock::new(|| {
    vec![
        // ── AWS ──────────────────────────────────────────────────
        SecretPattern {
            name: "AWS Access Key ID",
            pattern: Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        SecretPattern {
            name: "AWS Secret Access Key",
            pattern: Regex::new(r"(?i)(?:aws_secret_access_key|aws_secret)\s*[:=]\s*[A-Za-z0-9/+=]{40}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        // ── GitHub ───────────────────────────────────────────────
        SecretPattern {
            name: "GitHub Personal Access Token",
            pattern: Regex::new(r"gh[ps]_[A-Za-z0-9_]{36,}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        SecretPattern {
            name: "GitHub OAuth Access Token",
            pattern: Regex::new(r"gho_[A-Za-z0-9_]{36,}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        // ── Slack ────────────────────────────────────────────────
        SecretPattern {
            name: "Slack Bot Token",
            pattern: Regex::new(r"xoxb-[0-9]{10,13}-[0-9]{10,13}-[0-9a-z]{24}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        SecretPattern {
            name: "Slack Webhook URL",
            pattern: Regex::new(r"https://hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+").unwrap(),
            severity: Severity::High,
            verifiable: true,
        },
        // ── Stripe ──────────────────────────────────────────────
        SecretPattern {
            name: "Stripe Secret Key",
            pattern: Regex::new(r"sk_live_[0-9a-zA-Z]{24,}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        // ── Google ──────────────────────────────────────────────
        SecretPattern {
            name: "Google API Key",
            pattern: Regex::new(r"AIza[0-9A-Za-z_\-]{35}").unwrap(),
            severity: Severity::High,
            verifiable: true,
        },
        // ── Telegram ────────────────────────────────────────────
        SecretPattern {
            name: "Telegram Bot Token",
            pattern: Regex::new(r"[0-9]{8,10}:[A-Za-z0-9_\-]{35}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        // ── GitLab ──────────────────────────────────────────────
        SecretPattern {
            name: "GitLab Personal Access Token",
            pattern: Regex::new(r"glpat-[A-Za-z0-9_\-]{20,}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
        // ── JWT ─────────────────────────────────────────────────
        SecretPattern {
            name: "JSON Web Token",
            pattern: Regex::new(r"eyJ[A-Za-z0-9_\-]*\.eyJ[A-Za-z0-9_\-]*\.[A-Za-z0-9_\-]*").unwrap(),
            severity: Severity::High,
            verifiable: false,
        },
        // ── Private Keys ────────────────────────────────────────
        SecretPattern {
            name: "Private Key (RSA/EC/DSA)",
            pattern: Regex::new(r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----").unwrap(),
            severity: Severity::Critical,
            verifiable: false,
        },
        // ── Generic Secrets (simplified patterns) ───────────────
        SecretPattern {
            name: "Password in Config",
            pattern: Regex::new(r"(?i)(?:password|passwd|pwd)\s*[:=]\s*\S{8,}").unwrap(),
            severity: Severity::High,
            verifiable: false,
        },
        SecretPattern {
            name: "API Key Assignment",
            pattern: Regex::new(r"(?i)(?:api[_-]?key|apikey)\s*[:=]\s*[A-Za-z0-9_\-]{20,}").unwrap(),
            severity: Severity::High,
            verifiable: false,
        },
        SecretPattern {
            name: "Token Assignment",
            pattern: Regex::new(r"(?i)(?:token|auth[_-]?token|access[_-]?token)\s*[:=]\s*[A-Za-z0-9_\-]{20,}").unwrap(),
            severity: Severity::High,
            verifiable: false,
        },
        SecretPattern {
            name: "Secret Assignment",
            pattern: Regex::new(r"(?i)(?:secret[_-]?key|client[_-]?secret|app[_-]?secret)\s*[:=]\s*[A-Za-z0-9_\-]{20,}").unwrap(),
            severity: Severity::High,
            verifiable: false,
        },
        // ── Binance ─────────────────────────────────────────────
        SecretPattern {
            name: "Binance API Key",
            pattern: Regex::new(r"(?i)binance[_-]?(?:api[_-]?key|secret)(?:\s*[:=]\s*)[A-Za-z0-9]{40,}").unwrap(),
            severity: Severity::Critical,
            verifiable: true,
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_key_detection() {
        let pattern = &SECRET_PATTERNS[0];
        assert!(pattern.pattern.is_match("AKIAIOSFODNN7EXAMPLE"));
        assert!(!pattern.pattern.is_match("NOT_A_KEY_AT_ALL"));
    }

    #[test]
    fn test_github_token_detection() {
        let found = SECRET_PATTERNS.iter().any(|p| {
            p.pattern
                .is_match("ghs_1234567890abcdefghijklmnopqrstuvwxyz")
        });
        assert!(found);
    }

    #[test]
    fn test_telegram_token_detection() {
        let found = SECRET_PATTERNS.iter().any(|p| {
            p.pattern
                .is_match("1234567890:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        });
        assert!(found);
    }

    #[test]
    fn test_private_key_detection() {
        let found = SECRET_PATTERNS
            .iter()
            .any(|p| p.pattern.is_match("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(found);
    }

    #[test]
    fn test_password_in_config() {
        let found = SECRET_PATTERNS
            .iter()
            .any(|p| p.pattern.is_match("password = mysecretpassword123"));
        assert!(found);
    }
}
