use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

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
        self.tools
            .values()
            .filter(|t| &t.category == cat)
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn execute_shell(command: &str) -> Result<String, String> {
    let output = Command::new("sh")
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
    let mut cmd = Command::new(&tool.path);
    if !args.is_empty() {
        for arg in args.split_whitespace() {
            cmd.arg(arg);
        }
    }
    let output = cmd.output().map_err(|e| format!("Failed to execute {}: {}", tool.path, e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

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
    ("python3", "python3", "Python interpreter", ToolCategory::System),
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
    ("docker", "docker", "Container runtime", ToolCategory::System),
    ("systemctl", "systemctl", "Service manager", ToolCategory::System),
    ("journalctl", "journalctl", "System logs", ToolCategory::System),
    ("nmcli", "nmcli", "Network manager", ToolCategory::Network),
    ("ip", "ip", "Network config", ToolCategory::Network),
    ("ss", "ss", "Socket stats", ToolCategory::Network),
    ("lsof", "lsof", "Open files", ToolCategory::System),
    ("strace", "strace", "System call trace", ToolCategory::Process),
    ("htop", "htop", "Interactive top", ToolCategory::Process),
    ("nvidia-smi", "nvidia-smi", "GPU monitor", ToolCategory::System),
    ("ollama", "ollama", "LLM runner", ToolCategory::System),
    ("cargo", "cargo", "Rust package manager", ToolCategory::System),
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
