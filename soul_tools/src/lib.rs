use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub path: String,
    pub description: String,
    pub category: ToolCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolCategory {
    System,
    Network,
    File,
    Process,
    Data,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionLevel {
    Read,
    Write,
    Destructive,
}

impl PermissionLevel {
    pub fn from_command(cmd: &str) -> Self {
        let lower = cmd.to_lowercase();
        // Destructive commands
        if lower.contains("rm -rf")
            || lower.contains("rm -r ")
            || lower.contains("mkfs")
            || lower.contains("dd if=")
            || lower.contains("shutdown")
            || lower.contains("reboot")
            || lower.contains("systemctl stop")
            || lower.contains("systemctl disable")
            || lower.contains("chmod 777")
            || lower.contains("chown -R")
        {
            return PermissionLevel::Destructive;
        }
        // Write commands
        if lower.starts_with("rm ")
            || lower.starts_with("mv ")
            || lower.starts_with("chmod")
            || lower.starts_with("chown")
            || lower.starts_with("mkdir")
            || lower.starts_with("touch")
            || lower.starts_with("cp ")
            || lower.contains(" > ")
            || lower.contains(" >> ")
            || lower.contains("tee ")
            || lower.contains("write")
            || lower.contains("patch")
            || lower.contains("sed -i")
        {
            return PermissionLevel::Write;
        }
        PermissionLevel::Read
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&Tool> {
        self.tools.values().collect()
    }

    pub fn search(&self, query: &str) -> Vec<&Tool> {
        self.tools
            .values()
            .filter(|t| {
                t.name.contains(query)
                    || t.description.to_lowercase().contains(&query.to_lowercase())
            })
            .collect()
    }

    pub fn by_category(&self, cat: &ToolCategory) -> Vec<&Tool> {
        self.tools.values().filter(|t| &t.category == cat).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Shell Execution ───────────────────────────────────────────────────

pub fn execute_shell(command: &str) -> Result<String, String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| format!("Failed to execute: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

pub fn execute_tool(tool: &Tool, args: &str) -> Result<String, String> {
    let mut cmd = std::process::Command::new(&tool.path);
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", tool.path, e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// ── Async Shell Execution ─────────────────────────────────────────────

pub struct AsyncShellExecutor {
    timeout: Duration,
}

impl AsyncShellExecutor {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub async fn execute(&self, command: &str) -> Result<ShellOutput, String> {
        let permission = PermissionLevel::from_command(command);

        let result = tokio::time::timeout(
            self.timeout,
            Command::new("sh").arg("-c").arg(command).output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => Ok(ShellOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                permission,
            }),
            Ok(Err(e)) => Err(format!("Failed to execute: {}", e)),
            Err(_) => Err(format!(
                "Command timed out after {}s: {}",
                self.timeout.as_secs(),
                command
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub permission: PermissionLevel,
}

impl ShellOutput {
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn summary(&self, max_len: usize) -> String {
        let output = if self.stdout.is_empty() {
            &self.stderr
        } else {
            &self.stdout
        };
        truncate_str(output, max_len)
    }
}

// ── File Operations ───────────────────────────────────────────────────

pub fn read_file(
    path: &str,
    start_line: Option<usize>,
    num_lines: Option<usize>,
) -> Result<String, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {}", path, e))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = start_line.unwrap_or(1).saturating_sub(1);
    let end = start + num_lines.unwrap_or(200);

    let selected: Vec<String> = lines[start..end.min(lines.len())]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}: {}", start + i + 1, line))
        .collect();

    let mut result = selected.join("\n");
    if end < lines.len() {
        result.push_str(&format!(
            "\n... ({} more lines, total {})",
            lines.len() - end,
            lines.len()
        ));
    }
    Ok(result)
}

pub fn write_file(path: &str, content: &str, append: bool) -> Result<(), String> {
    use std::io::Write;
    let mut file = if append {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Cannot open {}: {}", path, e))?
    } else {
        std::fs::File::create(path).map_err(|e| format!("Cannot create {}: {}", path, e))?
    };
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Cannot write {}: {}", path, e))
}

pub fn patch_file(path: &str, old_text: &str, new_text: &str) -> Result<String, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {}", path, e))?;

    let count = content.matches(old_text).count();
    if count == 0 {
        return Err(format!(
            "Text not found in {}. Try reading the file first.",
            path
        ));
    }
    if count > 1 {
        return Err(format!(
            "Text found {} times in {}. Must be unique. Use a larger chunk.",
            count, path
        ));
    }

    let new_content = content.replacen(old_text, new_text, 1);
    std::fs::write(path, new_content).map_err(|e| format!("Cannot write {}: {}", path, e))?;

    Ok(format!("Patched {} (1 occurrence replaced)", path))
}

pub fn list_directory(path: &str) -> Result<Vec<String>, String> {
    let entries =
        std::fs::read_dir(path).map_err(|e| format!("Cannot read directory {}: {}", path, e))?;

    let mut items: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let file_type = e
                .file_type()
                .map(|ft| {
                    if ft.is_dir() {
                        "/"
                    } else if ft.is_symlink() {
                        "@"
                    } else {
                        ""
                    }
                })
                .unwrap_or("");
            format!("{}{}", e.file_name().to_string_lossy(), file_type)
        })
        .collect();

    items.sort();
    Ok(items)
}

pub fn search_files(pattern: &str, path: &str) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("find")
        .arg(path)
        .arg("-name")
        .arg(pattern)
        .arg("-type")
        .arg("f")
        .output()
        .map_err(|e| format!("find failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

pub fn grep_content(
    pattern: &str,
    path: &str,
    file_pattern: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut args = vec!["-rn", pattern, path];
    if let Some(fp) = file_pattern {
        args.push("--include");
        args.push(fp);
    }

    let output = std::process::Command::new("grep")
        .args(&args)
        .output()
        .map_err(|e| format!("grep failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

// ── Tool Dispatch ─────────────────────────────────────────────────────

pub fn dispatch_tool(tool_name: &str, args: &serde_json::Value) -> Result<String, String> {
    match tool_name {
        "execute_shell" => {
            let cmd = args
                .get("command")
                .and_then(|c| c.as_str())
                .ok_or("Missing 'command' argument")?;
            execute_shell(cmd)
        }
        "read_file" => {
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or("Missing 'path' argument")?;
            let start = args
                .get("start_line")
                .and_then(|l| l.as_u64())
                .map(|n| n as usize);
            let num = args
                .get("num_lines")
                .and_then(|l| l.as_u64())
                .map(|n| n as usize);
            read_file(path, start, num)
        }
        "write_file" => {
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or("Missing 'path' argument")?;
            let content = args
                .get("content")
                .and_then(|c| c.as_str())
                .ok_or("Missing 'content' argument")?;
            let append = args
                .get("mode")
                .and_then(|m| m.as_str())
                .map(|m| m == "append")
                .unwrap_or(false);
            write_file(path, content, append)?;
            Ok(format!("Written to {}", path))
        }
        "patch_file" => {
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or("Missing 'path' argument")?;
            let old = args
                .get("old_text")
                .and_then(|t| t.as_str())
                .ok_or("Missing 'old_text' argument")?;
            let new = args
                .get("new_text")
                .and_then(|t| t.as_str())
                .ok_or("Missing 'new_text' argument")?;
            patch_file(path, old, new)
        }
        "list_directory" => {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            let items = list_directory(path)?;
            Ok(items.join("\n"))
        }
        "search_files" => {
            let pattern = args
                .get("pattern")
                .and_then(|p| p.as_str())
                .ok_or("Missing 'pattern' argument")?;
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            let files = search_files(pattern, path)?;
            Ok(files.join("\n"))
        }
        "grep_content" => {
            let pattern = args
                .get("pattern")
                .and_then(|p| p.as_str())
                .ok_or("Missing 'pattern' argument")?;
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or(".");
            let fp = args.get("file_pattern").and_then(|p| p.as_str());
            let results = grep_content(pattern, path, fp)?;
            Ok(results.join("\n"))
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

/// Async version of dispatch_tool that runs blocking operations on a separate thread.
/// Use this from async contexts to avoid blocking the tokio runtime.
pub async fn async_dispatch_tool(
    tool_name: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    // For shell execution, use spawn_blocking to avoid blocking the runtime
    if tool_name == "execute_shell" {
        let name = tool_name.to_string();
        tokio::task::spawn_blocking(move || dispatch_tool(&name, &args))
            .await
            .map_err(|e| format!("Task join error: {}", e))?
    } else {
        // For other tools (file I/O), they're fast enough to run inline
        // but we still wrap in spawn_blocking for consistency
        let name = tool_name.to_string();
        tokio::task::spawn_blocking(move || dispatch_tool(&name, &args))
            .await
            .map_err(|e| format!("Task join error: {}", e))?
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let half = max_len / 2;
        format!(
            "{}\n...[truncated {} chars]...\n{}",
            &s[..half],
            s.len() - max_len,
            &s[s.len() - half..]
        )
    }
}

// ── System Tool Discovery ─────────────────────────────────────────────

const SYSTEM_TOOLS: &[(&str, &str, &str, ToolCategory)] = &[
    ("ls", "ls", "List directory contents", ToolCategory::File),
    ("cat", "cat", "Display file contents", ToolCategory::File),
    ("grep", "grep", "Search text patterns", ToolCategory::File),
    ("find", "find", "Find files", ToolCategory::File),
    ("ps", "ps", "List processes", ToolCategory::Process),
    ("top", "top", "Process monitor", ToolCategory::Process),
    ("df", "df", "Disk usage", ToolCategory::System),
    ("du", "du", "Directory sizes", ToolCategory::System),
    ("free", "free", "Memory usage", ToolCategory::System),
    ("uname", "uname", "System info", ToolCategory::System),
    ("curl", "curl", "HTTP client", ToolCategory::Network),
    ("wget", "wget", "Download files", ToolCategory::Network),
    ("ping", "ping", "Network test", ToolCategory::Network),
    ("ssh", "ssh", "Remote shell", ToolCategory::Network),
    ("scp", "scp", "Copy files remote", ToolCategory::Network),
    ("git", "git", "Version control", ToolCategory::System),
    (
        "python3",
        "python3",
        "Python interpreter",
        ToolCategory::System,
    ),
    ("node", "node", "Node.js runtime", ToolCategory::System),
    ("jq", "jq", "JSON processor", ToolCategory::Data),
    ("sed", "sed", "Stream editor", ToolCategory::File),
    ("awk", "awk", "Text processing", ToolCategory::Data),
    ("sort", "sort", "Sort lines", ToolCategory::Data),
    ("wc", "wc", "Word count", ToolCategory::Data),
    ("head", "head", "First lines", ToolCategory::File),
    ("tail", "tail", "Last lines", ToolCategory::File),
    ("chmod", "chmod", "Change permissions", ToolCategory::File),
    ("chown", "chown", "Change ownership", ToolCategory::File),
    ("mkdir", "mkdir", "Create directory", ToolCategory::File),
    ("rm", "rm", "Remove files", ToolCategory::File),
    ("cp", "cp", "Copy files", ToolCategory::File),
    ("mv", "mv", "Move files", ToolCategory::File),
    ("tar", "tar", "Archive files", ToolCategory::File),
    (
        "docker",
        "docker",
        "Container runtime",
        ToolCategory::System,
    ),
    (
        "systemctl",
        "systemctl",
        "Service manager",
        ToolCategory::System,
    ),
    (
        "journalctl",
        "journalctl",
        "System logs",
        ToolCategory::System,
    ),
    ("nmcli", "nmcli", "Network manager", ToolCategory::Network),
    ("ip", "ip", "Network config", ToolCategory::Network),
    ("ss", "ss", "Socket stats", ToolCategory::Network),
    ("lsof", "lsof", "Open files", ToolCategory::System),
    (
        "strace",
        "strace",
        "System call trace",
        ToolCategory::Process,
    ),
    ("htop", "htop", "Interactive top", ToolCategory::Process),
    (
        "nvidia-smi",
        "nvidia-smi",
        "GPU monitor",
        ToolCategory::System,
    ),
    ("ollama", "ollama", "LLM runner", ToolCategory::System),
    (
        "cargo",
        "cargo",
        "Rust package manager",
        ToolCategory::System,
    ),
];

pub fn discover_system_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    for (name, path, desc, cat) in SYSTEM_TOOLS {
        if which::which(path).is_ok() {
            tools.push(Tool {
                name: name.to_string(),
                path: path.to_string(),
                description: desc.to_string(),
                category: cat.clone(),
            });
        }
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry() {
        let mut reg = ToolRegistry::new();
        reg.register(Tool {
            name: "test".to_string(),
            path: "/bin/echo".to_string(),
            description: "echo".to_string(),
            category: ToolCategory::System,
        });
        assert_eq!(reg.list().len(), 1);
        assert!(reg.get("test").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_tool_search() {
        let mut reg = ToolRegistry::new();
        reg.register(Tool {
            name: "docker".to_string(),
            path: "/usr/bin/docker".to_string(),
            description: "Container runtime".to_string(),
            category: ToolCategory::System,
        });
        reg.register(Tool {
            name: "ls".to_string(),
            path: "/bin/ls".to_string(),
            description: "List files".to_string(),
            category: ToolCategory::File,
        });
        let results = reg.search("docker");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "docker");
    }

    #[test]
    fn test_tool_by_category() {
        let mut reg = ToolRegistry::new();
        reg.register(Tool {
            name: "ls".to_string(),
            path: "/bin/ls".to_string(),
            description: "List files".to_string(),
            category: ToolCategory::File,
        });
        reg.register(Tool {
            name: "ps".to_string(),
            path: "/bin/ps".to_string(),
            description: "List processes".to_string(),
            category: ToolCategory::Process,
        });
        let file_tools = reg.by_category(&ToolCategory::File);
        assert_eq!(file_tools.len(), 1);
    }

    #[test]
    fn test_execute_shell() {
        let result = execute_shell("echo hello");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "hello");
    }

    #[test]
    fn test_execute_shell_error() {
        let result = execute_shell("nonexistent_command_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_tool() {
        let tool = Tool {
            name: "echo".to_string(),
            path: "/bin/echo".to_string(),
            description: "echo".to_string(),
            category: ToolCategory::System,
        };
        let result = execute_tool(&tool, "hello world");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "hello world");
    }

    #[test]
    fn test_execute_tool_no_args() {
        let tool = Tool {
            name: "echo".to_string(),
            path: "/bin/echo".to_string(),
            description: "echo".to_string(),
            category: ToolCategory::System,
        };
        let result = execute_tool(&tool, "");
        assert!(result.is_ok());
    }

    #[test]
    fn test_discover_system_tools() {
        let tools = discover_system_tools();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "ls"));
    }

    #[test]
    fn test_tool_category_eq() {
        assert_eq!(ToolCategory::System, ToolCategory::System);
        assert_ne!(ToolCategory::System, ToolCategory::File);
    }
}
