//! Auto-diagnostic: scans journalctl, source files, and system state
//! to propose concrete patches for runtime errors and warnings.
#![allow(clippy::if_same_then_else)]

use std::process::Command;
use std::time::SystemTime;

/// Severity levels for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(&self) -> &str {
        match self {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN",
            Severity::Info => "INFO",
        }
    }
    pub fn order(&self) -> u8 {
        match self {
            Severity::Error => 3,
            Severity::Warning => 2,
            Severity::Info => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub source: String,
    pub severity: Severity,
    pub message: String,
    pub location: Option<String>,
    pub timestamp: SystemTime,
    pub suggested_fix: Option<String>,
}

pub struct SystemDiagnostic {
    service_patterns: Vec<String>,
    source_checks: Vec<fn(&str) -> Vec<Diagnostic>>,
}

impl Default for SystemDiagnostic {
    fn default() -> Self {
        Self {
            service_patterns: vec![
                "soullink-*".into(),
                "synergie*".into(),
                "openevolve*".into(),
                "ollama*".into(),
            ],
            source_checks: vec![check_for_unwraps, check_for_todo, check_for_unsafe_blocks],
        }
    }
}

impl SystemDiagnostic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run_full(&self, source_dir: &str) -> Vec<Diagnostic> {
        let mut all = Vec::new();
        all.extend(self.scan_journal(200));
        all.extend(self.scan_source(source_dir));
        all.extend(self.check_runtime());
        #[allow(clippy::unnecessary_sort_by)]
        all.sort_by(|a, b| b.severity.order().cmp(&a.severity.order()));
        all
    }

    pub fn scan_journal(&self, lines: usize) -> Vec<Diagnostic> {
        let mut results = Vec::new();
        for pattern in &self.service_patterns {
            let output = Command::new("journalctl")
                .args(["-u", pattern, "--no-pager", "-n", &lines.to_string()])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout);
                    for line in text.lines() {
                        let lower = line.to_lowercase();
                        let sev = if lower.contains("panic") || lower.contains("fatal") {
                            Severity::Error
                        } else if lower.contains("error") {
                            Severity::Error
                        } else if lower.contains("warn") || lower.contains("fail") {
                            Severity::Warning
                        } else {
                            continue;
                        };
                        results.push(Diagnostic {
                            source: format!("journalctl -u {}", pattern),
                            severity: sev,
                            message: line.chars().take(200).collect(),
                            location: None,
                            timestamp: SystemTime::now(),
                            suggested_fix: self.propose_patch(line, pattern),
                        });
                    }
                }
                _ => {
                    results.push(Diagnostic {
                        source: "journalctl".into(),
                        severity: Severity::Info,
                        message: format!("Cannot read journal for {}", pattern),
                        location: None,
                        timestamp: SystemTime::now(),
                        suggested_fix: Some("Check systemd journal permissions".into()),
                    });
                }
            }
        }
        results
    }

    pub fn scan_source(&self, dir: &str) -> Vec<Diagnostic> {
        let mut results = Vec::new();
        let path = std::path::Path::new(dir);
        if !path.exists() {
            results.push(Diagnostic {
                source: "source".into(),
                severity: Severity::Warning,
                message: format!("Source directory not found: {}", dir),
                location: Some(dir.into()),
                timestamp: SystemTime::now(),
                suggested_fix: Some("Check source directory path".into()),
            });
            return results;
        }
        let walker = walkdir::WalkDir::new(dir)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| matches!(ext.to_str(), Some("rs" | "py" | "js" | "ts")))
                    .unwrap_or(false)
            });
        for entry in walker {
            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for check in &self.source_checks {
                for mut d in check(&content) {
                    d.source = entry.path().to_string_lossy().to_string();
                    d.timestamp = SystemTime::now();
                    results.push(d);
                }
            }
        }
        results
    }

    pub fn check_runtime(&self) -> Vec<Diagnostic> {
        let mut results = Vec::new();
        if let Ok(output) = Command::new("df").args(["-h", "/"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = text.lines().nth(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    if let Ok(pct) = parts[4].trim_end_matches('%').parse::<u8>() {
                        let sev = if pct > 90 {
                            Severity::Error
                        } else if pct > 75 {
                            Severity::Warning
                        } else {
                            Severity::Info
                        };
                        results.push(Diagnostic {
                            source: "df".into(),
                            severity: sev,
                            message: format!("Disk usage: {}%", pct),
                            location: Some("/".into()),
                            timestamp: SystemTime::now(),
                            suggested_fix: if pct > 80 {
                                Some("sudo journalctl --vacuum-time=3d".into())
                            } else {
                                None
                            },
                        });
                    }
                }
            }
        }
        if let Ok(output) = Command::new("free").args(["-m"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = text.lines().nth(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let total: f64 = parts[1].parse().unwrap_or(1.0);
                    let used: f64 = parts[2].parse().unwrap_or(0.0);
                    let ratio = used / total;
                    let sev = if ratio > 0.90 {
                        Severity::Error
                    } else if ratio > 0.75 {
                        Severity::Warning
                    } else {
                        Severity::Info
                    };
                    results.push(Diagnostic {
                        source: "free".into(),
                        severity: sev,
                        message: format!("Memory: {:.1}%", ratio * 100.0),
                        location: None,
                        timestamp: SystemTime::now(),
                        suggested_fix: if ratio > 0.85 {
                            Some("Restart services or add swap".into())
                        } else {
                            None
                        },
                    });
                }
            }
        }
        results
    }

    pub fn propose_patch(&self, error: &str, context: &str) -> Option<String> {
        let lower = error.to_lowercase();
        if lower.contains("oom") || lower.contains("out of memory") {
            Some("Increase memory limit or reduce context size".into())
        } else if lower.contains("connection refused") {
            Some("systemctl restart synergie".into())
        } else if lower.contains("segfault") || lower.contains("sigsegv") {
            Some("Review unsafe blocks in Rust code".into())
        } else if lower.contains("timeout") {
            Some("Increase timeout in config".into())
        } else if lower.contains("disk full") || lower.contains("no space") {
            Some("sudo journalctl --vacuum-size=500M".into())
        } else {
            Some(format!(
                "Review {} logs: journalctl -u {} -n 50",
                context, context
            ))
        }
    }
}

fn check_for_unwraps(content: &str) -> Vec<Diagnostic> {
    let mut r = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.contains(".unwrap(")
            && !line.trim_start().starts_with("//")
            && !line.trim_start().starts_with("#[")
        {
            r.push(Diagnostic {
                source: String::new(),
                severity: Severity::Warning,
                message: format!(
                    "Unwrap at line {}: {}",
                    i + 1,
                    line.trim().chars().take(60).collect::<String>()
                ),
                location: Some(format!("line {}", i + 1)),
                timestamp: SystemTime::now(),
                suggested_fix: Some("Replace with ? operator".into()),
            });
        }
    }
    r
}

fn check_for_todo(content: &str) -> Vec<Diagnostic> {
    let mut r = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.trim().to_uppercase().contains("TODO") && !line.trim().starts_with("//") {
            r.push(Diagnostic {
                source: String::new(),
                severity: Severity::Info,
                message: format!(
                    "TODO at line {}: {}",
                    i + 1,
                    line.trim().chars().take(60).collect::<String>()
                ),
                location: Some(format!("line {}", i + 1)),
                timestamp: SystemTime::now(),
                suggested_fix: Some("Implement or convert to GitHub issue".into()),
            });
        }
    }
    r
}

fn check_for_unsafe_blocks(content: &str) -> Vec<Diagnostic> {
    let mut r = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.trim() == "unsafe {" || line.trim() == "unsafe{" {
            r.push(Diagnostic {
                source: String::new(),
                severity: Severity::Warning,
                message: format!("Unsafe block at line {}", i + 1),
                location: Some(format!("line {}", i + 1)),
                timestamp: SystemTime::now(),
                suggested_fix: Some("Audit unsafe block — prefer safe abstractions".into()),
            });
        }
    }
    r
}

pub fn print_report(diagnostics: &[Diagnostic]) {
    let ec = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .count();
    let wc = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Warning))
        .count();
    println!("═══ Diagnostic Report ═══");
    println!(
        "Errors: {}  Warnings: {}  Info: {}",
        ec,
        wc,
        diagnostics.len() - ec - wc
    );
    for d in diagnostics {
        let icon = match d.severity {
            Severity::Error => "✘",
            Severity::Warning => "⚠",
            Severity::Info => "ℹ",
        };
        println!("{} {}: {}", icon, d.severity.label(), d.message);
        if let Some(fix) = &d.suggested_fix {
            println!("   Fix: {}", fix);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detect_unwrap() {
        let r = check_for_unwraps("let x = foo().unwrap();\n");
        assert_eq!(r.len(), 1);
    }
    #[test]
    fn test_severity_order() {
        assert!(Severity::Error.order() > Severity::Warning.order());
    }
}
