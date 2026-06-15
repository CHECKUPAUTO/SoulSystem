//! Detectors — combines pattern matching and entropy analysis.

use crate::entropy::{is_base64_secret, is_hex_secret};
use crate::patterns::{Severity, SECRET_PATTERNS};
use std::path::Path;

/// A finding from a scan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub rule_name: String,
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub secret: String,
    pub verifiable: bool,
    /// Masked version of the secret for safe display.
    pub masked: String,
}

impl Finding {
    /// Mask the secret for safe display (show first 4 and last 4 chars).
    pub fn mask_secret(secret: &str) -> String {
        if secret.len() <= 12 {
            return "*".repeat(secret.len());
        }
        format!("{}...{}", &secret[..4], &secret[secret.len() - 4..])
    }
}

/// Scan a single line for secrets.
pub fn scan_line(line: &str, file: &str, line_num: usize) -> Vec<Finding> {
    let mut findings = Vec::new();

    // 1. Pattern-based detection
    for pattern in SECRET_PATTERNS.iter() {
        for mat in pattern.pattern.find_iter(line) {
            findings.push(Finding {
                rule_name: pattern.name.to_string(),
                severity: pattern.severity,
                file: file.to_string(),
                line: line_num,
                column: mat.start(),
                secret: mat.as_str().to_string(),
                verifiable: pattern.verifiable,
                masked: Finding::mask_secret(mat.as_str()),
            });
        }
    }

    // 2. Entropy-based detection (catches unknown secrets)
    for word in line.split_whitespace() {
        let word = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';');
        if word.len() >= 20 && (is_base64_secret(word) || is_hex_secret(word)) {
            {
                // Skip if already caught by pattern matching
                let already_found = findings.iter().any(|f| f.secret == word);
                if !already_found {
                    findings.push(Finding {
                        rule_name: "High-Entropy String".to_string(),
                        severity: Severity::Medium,
                        file: file.to_string(),
                        line: line_num,
                        column: line.find(word).unwrap_or(0),
                        secret: word.to_string(),
                        verifiable: false,
                        masked: Finding::mask_secret(word),
                    });
                }
            }
        }
    }

    findings
}

/// Should we skip this file? (binary, images, etc.)
pub fn should_skip_file(path: &Path) -> bool {
    let skip_extensions = [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg", ".woff", ".woff2", ".ttf",
        ".eot", ".zip", ".tar", ".gz", ".bz2", ".xz", ".zst", ".mp3", ".mp4", ".wav", ".avi",
        ".mkv", ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".class", ".jar", ".war", ".so", ".o",
        ".a", ".bin", ".exe", ".dll", ".dylib",
    ];

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return skip_extensions.contains(&format!(".{ext}").as_str());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_key_in_line() {
        let findings = scan_line("aws_key = AKIAIOSFODNN7EXAMPLE", "config.rs", 1);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule_name, "AWS Access Key ID");
    }

    #[test]
    fn test_password_in_line() {
        let findings = scan_line("password = \"mysecretpassword123\"", "config.toml", 5);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_skip_binary() {
        assert!(should_skip_file(Path::new("image.png")));
        assert!(should_skip_file(Path::new("lib.so")));
        assert!(!should_skip_file(Path::new("main.rs")));
        assert!(!should_skip_file(Path::new("config.toml")));
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(Finding::mask_secret("short"), "*****");
        assert_eq!(
            Finding::mask_secret("AKIAIOSFODNN7EXAMPLE1234567890"),
            "AKIA...7890"
        );
    }
}
