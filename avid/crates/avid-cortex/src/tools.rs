use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Signature for tool implementation functions.
pub type ToolFn = Box<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<Value, anyhow::Error>> + Send>> + Send + Sync>;

/// A tool that can be called by the LLM during reasoning.
pub struct Tool {
    /// Unique identifier for the tool.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub parameters: Value,
    /// Async handler function for tool execution.
    pub handler: ToolFn,
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish()
    }
}

/// Registry for managing and executing tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
}

impl ToolRegistry {
    /// Create a new empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new tool in the registry.
    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Execute a tool by name with the given arguments.
    ///
    /// # Errors
    /// Returns an error if the tool is not found or execution fails.
    pub async fn call(&self, name: &str, args: Value, timeout: Duration) -> Result<Value, anyhow::Error> {
        let tool = self.tools.get(name).ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", name))?;

        // Wrap execution in a timeout
        tokio::time::timeout(timeout, (tool.handler)(args))
            .await
            .map_err(|_| anyhow::anyhow!("Tool execution timed out after {:?}", timeout))?
    }

    /// Generate a list of tool definitions in JSON format for LLM context.
    pub fn get_definitions(&self) -> Vec<Value> {
        self.tools.values().map(|t| serde_json::json!({
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters
        })).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_tool_registry() {
        let mut tr = ToolRegistry::new();
        tr.register(Tool {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: json!({}),
            handler: Box::new(|_| Box::pin(async { Ok(json!({"ok": true})) })),
        });

        let res = tr.call("test", json!({}), Duration::from_secs(1)).await.unwrap();
        assert_eq!(res["ok"], true);
    }

    #[test]
    fn test_tool_definitions() {
        let mut tr = ToolRegistry::new();
        tr.register(Tool {
            name: "test".to_string(),
            description: "desc".to_string(),
            parameters: json!({}),
            handler: Box::new(|_| Box::pin(async { Ok(json!({})) })),
        });
        assert_eq!(tr.get_definitions().len(), 1);
    }
}
