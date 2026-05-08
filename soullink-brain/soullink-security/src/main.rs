//! SoulLink Security Scanner — standalone binary.
//!
//! Usage: soullink-scan [PATH]
//! Scans files/directories for secrets and vulnerabilities.

use soullink_security::{SecurityScanner, Severity};
use std::path::PathBuf;
use tracing::info;

#[tokio::main(worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    info!("🦞 SoulLink Security Scanner v13.5");
    info!("Scanning: {}", path.display());

    let scanner = SecurityScanner::new();
    let result = scanner.scan_directory(&path).await;

    println!("\n🦞 SoulLink Security Scan Results");
    println!("==================================");
    println!("Files scanned:  {}", result.files_scanned);
    println!("Lines scanned:  {}", result.lines_scanned);
    println!("Duration:       {}ms", result.duration_ms);
    println!();

    if result.findings.is_empty() {
        println!("✅ No secrets detected!");
    } else {
        println!("⚠️  Found {} potential secrets:", result.findings.len());
        println!();

        let critical: Vec<_> = result.findings.iter().filter(|f| f.severity == Severity::Critical).collect();
        let high: Vec<_> = result.findings.iter().filter(|f| f.severity == Severity::High).collect();
        let medium: Vec<_> = result.findings.iter().filter(|f| f.severity == Severity::Medium).collect();

        if !critical.is_empty() {
            println!("🔴 CRITICAL ({}):", critical.len());
            for f in &critical {
                println!("   {}:{} — {} [{}]", f.file, f.line, f.rule_name, f.masked);
            }
        }

        if !high.is_empty() {
            println!("🟠 HIGH ({}):", high.len());
            for f in &high {
                println!("   {}:{} — {} [{}]", f.file, f.line, f.rule_name, f.masked);
            }
        }

        if !medium.is_empty() {
            println!("🟡 MEDIUM ({}):", medium.len());
            for f in &medium {
                println!("   {}:{} — {} [{}]", f.file, f.line, f.rule_name, f.masked);
            }
        }
    }

    if !result.verified.is_empty() {
        println!("\n🔍 Verified Secrets:");
        for v in &result.verified {
            if v.verified {
                println!("   🚨 {} — {}", v.rule_name, v.status);
            } else {
                println!("   ✅ {} — {}", v.rule_name, v.status);
            }
        }
    }

    // Exit code: 1 if critical findings, 0 otherwise
    let has_critical = result.findings.iter().any(|f| f.severity == Severity::Critical);
    std::process::exit(if has_critical { 1 } else { 0 });
}