use std::collections::HashMap;
use std::fs;
use std::path::Path;

use regex::Regex;
use tracing::{info, warn};

/// Élément de surface d'attaque
#[derive(Debug, Clone)]
pub struct SurfaceItem {
    pub file_path: String,
    pub line_number: u32,
    pub description: String,
    pub severity: String,
    pub context: String,
}

/// Inventaire d'un dossier
#[derive(Debug, Clone)]
pub struct Inventory {
    pub file_count: usize,
    pub total_lines: usize,
    pub function_count: usize,
    pub unsafe_blocks: usize,
    pub file_types: HashMap<String, usize>,
}

/// Graphe de flux de données (stub structurel)
#[derive(Debug, Clone)]
pub struct DataFlow {
    pub entry_point: String,
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

/// Analyse la surface d'attaque d'un dossier
pub fn map_surface(dir_path: &str) -> anyhow::Result<Vec<SurfaceItem>> {
    let path = Path::new(dir_path);
    if !path.exists() {
        anyhow::bail!("Path does not exist: {dir_path}");
    }

    let mut items = Vec::new();
    let unsafe_re = Regex::new(r"unsafe\s*\{")?;
    let unwrap_re = Regex::new(r"\.unwrap\(\)")?;
    let http_re = Regex::new(r#"https?://[^\s"']+"#)?;
    let eval_re = Regex::new(r"\beval\s*\(")?;
    let shell_exec = Regex::new(r#""/bin/(?:sh|bash)\s*-c"#)?;

    visit_dir(path, &mut items, &unsafe_re, &unwrap_re, &http_re, &eval_re, &shell_exec)?;

    info!("Surface mapping complete: {} items found", items.len());
    Ok(items)
}

/// Parcours récursif d'un dossier à la recherche de patterns dangereux
fn visit_dir(
    path: &Path,
    items: &mut Vec<SurfaceItem>,
    unsafe_re: &Regex,
    unwrap_re: &Regex,
    http_re: &Regex,
    eval_re: &Regex,
    shell_re: &Regex,
) -> anyhow::Result<()> {
    if !path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry_path.is_dir() {
            // Skip hidden directories and node_modules/target
            let name = entry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                visit_dir(&entry_path, items, unsafe_re, unwrap_re, http_re, eval_re, shell_re)?;
            }
        } else if let Some(ext) = entry_path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            match ext.as_str() {
                "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" => {
                    scan_file(&entry_path, items, unsafe_re, unwrap_re, http_re, eval_re, shell_re)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Scanne un fichier pour des patterns dangereux
fn scan_file(
    path: &Path,
    items: &mut Vec<SurfaceItem>,
    unsafe_re: &Regex,
    unwrap_re: &Regex,
    http_re: &Regex,
    eval_re: &Regex,
    shell_re: &Regex,
) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?;
    let file_path = path.to_string_lossy().to_string();

    for (line_num, line) in content.lines().enumerate() {
        let line_num = (line_num + 1) as u32;

        // Check for unsafe blocks (Rust)
        if unsafe_re.is_match(line)
            && unsafe_re.is_match(line) {
                items.push(SurfaceItem {
                    file_path: file_path.clone(),
                    line_number: line_num,
                    description: "Unsafe block detected".to_string(),
                    severity: "high".to_string(),
                    context: line.trim().to_string(),
                });
                warn!("[UNSAFE] {}:{} — {}", file_path, line_num, line.trim());
            }

        // Check for unwrap() calls
        if unwrap_re.is_match(line) {
            items.push(SurfaceItem {
                file_path: file_path.clone(),
                line_number: line_num,
                description: "Unwrap call detected (potential panic)".to_string(),
                severity: "medium".to_string(),
                context: line.trim().to_string(),
            });
        }

        // Check for hardcoded HTTP URLs
        if http_re.is_match(line) {
            items.push(SurfaceItem {
                file_path: file_path.clone(),
                line_number: line_num,
                description: "Hardcoded URL detected".to_string(),
                severity: "low".to_string(),
                context: line.trim().to_string(),
            });
        }

        // Check for eval() calls (JavaScript/Python)
        if eval_re.is_match(line) && path.extension().is_some_and(|e| e == "js" || e == "py") {
            items.push(SurfaceItem {
                file_path: file_path.clone(),
                line_number: line_num,
                description: "eval() call detected (code injection risk)".to_string(),
                severity: "high".to_string(),
                context: line.trim().to_string(),
            });
        }

        // Check for shell execution
        if shell_re.is_match(line) {
            items.push(SurfaceItem {
                file_path: file_path.clone(),
                line_number: line_num,
                description: "Shell execution detected".to_string(),
                severity: "critical".to_string(),
                context: line.trim().to_string(),
            });
        }
    }
    Ok(())
}

/// Trace de flux de données (stub structurellement valide)
pub fn trace_dataflow(entry_point: &str) -> DataFlow {
    DataFlow {
        entry_point: entry_point.to_string(),
        nodes: vec!["input".to_string(), "process".to_string(), "output".to_string()],
        edges: vec![
            ("input".to_string(), "process".to_string()),
            ("process".to_string(), "output".to_string()),
        ],
    }
}

/// Inventaire complet d'un dossier source
pub fn build_inventory(dir_path: &str) -> anyhow::Result<Inventory> {
    let path = Path::new(dir_path);
    if !path.exists() {
        anyhow::bail!("Path does not exist: {dir_path}");
    }

    let mut file_count = 0usize;
    let mut total_lines = 0usize;
    let mut function_count = 0usize;
    let mut unsafe_blocks = 0usize;
    let mut file_types = HashMap::new();

    let fn_re = Regex::new(r"\b(?:fn|def|function)\s+\w+")?;
    let unsafe_re = Regex::new(r"unsafe\s*\{")?;

    visit_dir_inventory(
        path,
        &mut file_count,
        &mut total_lines,
        &mut function_count,
        &mut unsafe_blocks,
        &mut file_types,
        &fn_re,
        &unsafe_re,
    )?;

    Ok(Inventory {
        file_count,
        total_lines,
        function_count,
        unsafe_blocks,
        file_types,
    })
}

#[allow(clippy::too_many_arguments)]
fn visit_dir_inventory(
    path: &Path,
    file_count: &mut usize,
    total_lines: &mut usize,
    function_count: &mut usize,
    unsafe_blocks: &mut usize,
    file_types: &mut HashMap<String, usize>,
    fn_re: &Regex,
    unsafe_re: &Regex,
) -> anyhow::Result<()> {
    if !path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if entry_path.is_dir() {
            let name = entry_path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                visit_dir_inventory(
                    &entry_path,
                    file_count,
                    total_lines,
                    function_count,
                    unsafe_blocks,
                    file_types,
                    fn_re,
                    unsafe_re,
                )?;
            }
        } else if let Some(ext) = entry_path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            match ext.as_str() {
                "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" => {
                    let content = fs::read_to_string(&entry_path)?;
                    *file_count += 1;
                    *total_lines += content.lines().count();
                    *function_count += fn_re.find_iter(&content).count();
                    *unsafe_blocks += unsafe_re.find_iter(&content).count();
                    *file_types.entry(ext).or_insert(0) += 1;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_surface_with_rs_file() {
        let dir = std::env::temp_dir().join("avid_test_understand");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("test.rs"),
            "fn main() {\n    unsafe { *ptr = 1; }\n    let x = val.unwrap();\n    let url = \"http://evil.com\";\n}\n",
        )
        .unwrap();

        let items = map_surface(&dir.to_string_lossy()).unwrap();
        assert!(items.len() >= 3, "Expected at least 3 items, got {}", items.len());
        assert!(items.iter().any(|i| i.description.contains("Unsafe")));
        assert!(items.iter().any(|i| i.description.contains("Unwrap")));
        assert!(items.iter().any(|i| i.description.contains("URL")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_build_inventory() {
        let dir = std::env::temp_dir().join("avid_test_inventory");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn foo() {}\nfn bar() {}\nunsafe {}\n").unwrap();
        std::fs::write(dir.join("b.py"), "def baz():\n    pass\n").unwrap();

        let inv = build_inventory(&dir.to_string_lossy()).unwrap();
        assert_eq!(inv.file_count, 2);
        assert_eq!(inv.function_count, 3);
        assert_eq!(inv.unsafe_blocks, 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}
