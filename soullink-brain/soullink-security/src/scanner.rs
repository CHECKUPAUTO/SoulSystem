//! File scanner — walks directories and scans for secrets.

use crate::detectors;
use crate::patterns::Severity;
use crate::verify::SecretVerifier;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::detectors::Finding;
use crate::verify::VerificationResult;
use tokio::sync::Mutex;
use tracing::warn;
use walkdir::WalkDir;

/// Result of a full scan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub lines_scanned: usize,
    pub findings: Vec<Finding>,
    pub verified: Vec<VerificationResult>,
    pub duration_ms: u64,
}

/// The main security scanner.
pub struct SecurityScanner {
    verifier: Arc<Mutex<SecretVerifier>>,
}

impl SecurityScanner {
    pub fn new() -> Self {
        Self {
            verifier: Arc::new(Mutex::new(SecretVerifier::new())),
        }
    }

    /// Scan a directory for secrets.
    pub async fn scan_directory(&self, path: &Path) -> ScanResult {
        let start = std::time::Instant::now();
        let mut files_scanned = 0;
        let mut lines_scanned = 0;
        let mut all_findings = Vec::new();

        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.into_path();

            // Skip binary, images, etc.
            if detectors::should_skip_file(&path) {
                continue;
            }

            // Skip common non-source directories
            if let Some(path_str) = path.to_str() {
                if path_str.contains("/.git/") ||
                   path_str.contains("/target/") ||
                   path_str.contains("/node_modules/") ||
                   path_str.contains("/.venv/") ||
                   path_str.contains("/__pycache__/") ||
                   path_str.contains(".cargo/registry")
                {
                    continue;
                }
            }

            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    files_scanned += 1;
                    for (line_num, line) in content.lines().enumerate() {
                        lines_scanned += 1;
                        let file_str = path.to_string_lossy().to_string();
                        let findings = detectors::scan_line(line, &file_str, line_num + 1);
                        all_findings.extend(findings);
                    }
                }
                Err(e) => {
                    warn!("Cannot read {}: {e}", path.display());
                }
            }
        }

        // Deduplicate findings
        all_findings.sort_by(|a, b| {
            b.severity.cmp(&a.severity)
                .then_with(|| a.file.cmp(&b.file))
                .then_with(|| a.line.cmp(&b.line))
        });
        all_findings.dedup_by(|a, b| a.secret == b.secret && a.file == b.file);

        // Verify findings
        let mut verifier = self.verifier.lock().await;
        let mut verified = Vec::new();
        for finding in &all_findings {
            if let Some(result) = verifier.verify(finding).await {
                verified.push(result);
            }
        }

        let duration = start.elapsed();

        ScanResult {
            files_scanned,
            lines_scanned,
            findings: all_findings,
            verified,
            duration_ms: duration.as_millis() as u64,
        }
    }

    /// Quick scan a single file.
    pub fn scan_file(&self, path: &Path) -> Vec<Finding> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let file_str = path.to_string_lossy().to_string();
        let mut findings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            findings.extend(detectors::scan_line(line, &file_str, line_num + 1));
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detectors;
    use crate::patterns::Severity;
    use std::io::Write;

    #[test]
    fn test_scan_clean_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("clean.rs");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "fn main() {{ println!(\"hello\"); }}").unwrap();

        let scanner = SecurityScanner::new();
        let findings = scanner.scan_file(&file_path);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scan_file_with_secret() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "api_key = \"ghp_1234567890abcdefghijklmnopqrstuvwxyz\"").unwrap();

        let scanner = SecurityScanner::new();
        let findings = scanner.scan_file(&file_path);
        assert!(!findings.is_empty());
        assert!(findings[0].rule_name.contains("GitHub"));
    }

    #[test]
    fn test_skip_binary() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("image.png");
        std::fs::write(&file_path, b"\x89PNG\r\n").unwrap();

        assert!(detectors::should_skip_file(&file_path));
    }
}