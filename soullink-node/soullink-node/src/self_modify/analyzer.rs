use anyhow::Result;
use std::path::Path;
use std::fs;

/// Report produced by static code analysis.
#[derive(Debug, Clone, Default)]
pub struct CodeReport {
    pub opportunities: Vec<String>,
    /// Detailed findings with file paths and line suggestions.
    pub findings: Vec<CodeFinding>,
}

#[derive(Debug, Clone)]
pub struct CodeFinding {
    pub file: String,
    pub line: usize,
    pub category: FindingCategory,
    pub description: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FindingCategory {
    TodoMarker,
    DeadCode,
    UnsafePattern,
    LargeFunction,
    MissingDocs,
    CloneOptimization,
}

impl CodeReport {
    /// Return true if the report contains improvement opportunities.
    pub fn has_improvement_opportunities(&self) -> bool {
        !self.opportunities.is_empty() || !self.findings.is_empty()
    }
}

/// Analyzer trait.
pub trait Analyzer {
    fn analyze(&self, workspace: &Path) -> Result<CodeReport>;
}

/// Syn-based analyzer that scans Rust source files for improvement opportunities.
pub struct SynAnalyzer {
    /// Minimum line count for a function to be flagged as "large".
    pub large_fn_threshold: usize,
    /// File extensions to analyze.
    pub extensions: Vec<String>,
}

impl SynAnalyzer {
    /// Create a new syn-based analyzer.
    pub fn new() -> Self {
        Self {
            large_fn_threshold: 100,
            extensions: vec!["rs".into()],
        }
    }

    /// Create with custom large function threshold.
    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.large_fn_threshold = threshold;
        self
    }
}

impl Analyzer for SynAnalyzer {
    fn analyze(&self, workspace: &Path) -> Result<CodeReport> {
        let mut report = CodeReport::default();

        // Scan all Rust files in the workspace src directories
        self.scan_directory(workspace, &mut report)?;

        // Also check sub-crates
        if let Ok(entries) = fs::read_dir(workspace) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let src_dir = path.join("src");
                    if src_dir.exists() {
                        self.scan_directory(&src_dir, &mut report)?;
                    }
                }
            }
        }

        Ok(report)
    }
}

impl SynAnalyzer {
    fn scan_directory(&self, dir: &Path, report: &mut CodeReport) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                self.scan_directory(&path, report)?;
                continue;
            }

            if let Some(ext) = path.extension() {
                if !self.extensions.iter().any(|e| e.as_str() == ext) {
                    continue;
                }
            } else {
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = path.to_string_lossy().to_string();

            // Scan line by line for patterns
            for (i, line) in content.lines().enumerate() {
                let line_num = i + 1;
                let trimmed = line.trim();

                // Detect todo!() / unimplemented!() markers
                if trimmed.contains("todo!()") || trimmed.contains("unimplemented!()") {
                    report.findings.push(CodeFinding {
                        file: rel_path.clone(),
                        line: line_num,
                        category: FindingCategory::TodoMarker,
                        description: "Unresolved todo!() or unimplemented!() found".into(),
                        suggestion: "Replace with proper error handling or implementation".into(),
                    });
                    report.opportunities.push(format!(
                        "Replace todo/unimplemented at {}:{}",
                        rel_path, line_num
                    ));
                }

                // Detect TODO/FIXME comments
                if trimmed.contains("// TODO") || trimmed.contains("// FIXME") || trimmed.contains("// HACK") {
                    report.findings.push(CodeFinding {
                        file: rel_path.clone(),
                        line: line_num,
                        category: FindingCategory::TodoMarker,
                        description: "TODO/FIXME/HACK comment found".into(),
                        suggestion: "Resolve or convert to tracked issue".into(),
                    });
                }

                // Detect #[allow(dead_code)] on types/functions
                if trimmed.contains("#[allow(dead_code)]") {
                    report.findings.push(CodeFinding {
                        file: rel_path.clone(),
                        line: line_num,
                        category: FindingCategory::DeadCode,
                        description: "#[allow(dead_code)] suppression found".into(),
                        suggestion: "Wire up the dead code or remove it".into(),
                    });
                    report.opportunities.push(format!(
                        "Dead code at {}:{} — wire or remove",
                        rel_path, line_num
                    ));
                }

                // Detect unsafe blocks
                if trimmed.contains("unsafe") && !trimmed.starts_with("//") {
                    report.findings.push(CodeFinding {
                        file: rel_path.clone(),
                        line: line_num,
                        category: FindingCategory::UnsafePattern,
                        description: "Unsafe block detected".into(),
                        suggestion: "Verify safety invariants and add SAFETY comment".into(),
                    });
                }

                // Detect large functions (>100 lines)
                // Track function boundaries via indentation
                if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
                    let start_line = line_num;
                    let mut fn_len = 1;
                    let mut min_indent = usize::MAX;

                    // Estimate function length by tracking indentation
                    for subsequent in content.lines().skip(i + 1) {
                        let sub_trimmed = subsequent.trim();
                        if sub_trimmed.is_empty() {
                            fn_len += 1;
                            continue;
                        }
                        if sub_trimmed.starts_with("fn ") || sub_trimmed.starts_with("pub fn ") || sub_trimmed.starts_with("mod ") || sub_trimmed.starts_with("impl ") {
                            break;
                        }
                        let indent = subsequent.len() - sub_trimmed.len();
                        if indent == 0 && fn_len > 2 {
                            if sub_trimmed.starts_with('}') {
                                fn_len += 1;
                            }
                            break;
                        }
                        fn_len += 1;
                    }

                    if fn_len > self.large_fn_threshold {
                        report.findings.push(CodeFinding {
                            file: rel_path.clone(),
                            line: start_line,
                            category: FindingCategory::LargeFunction,
                            description: format!("Large function ({} lines)", fn_len),
                            suggestion: "Consider refactoring into smaller functions".into(),
                        });
                    }
                }

                // Detect .clone() calls that could be replaced with references
                if trimmed.contains(".clone()") && !trimmed.starts_with("//") {
                    report.findings.push(CodeFinding {
                        file: rel_path.clone(),
                        line: line_num,
                        category: FindingCategory::CloneOptimization,
                        description: "Potential unnecessary clone detected".into(),
                        suggestion: "Consider passing by reference instead".into(),
                    });
                }
            }
        }

        Ok(())
    }
}
