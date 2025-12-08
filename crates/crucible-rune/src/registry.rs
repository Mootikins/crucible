//! Simplified Rune tool registry (no hot-reload)

use crate::discovery::ToolDiscovery;
use crate::executor::RuneExecutor;
use crate::types::{RuneDiscoveryConfig, RuneExecutionResult, RuneTool};
use crate::RuneError;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Simplified registry for Rune tools
///
/// This is a streamlined version without hot-reload, caching TTL, or categories.
/// Tools are discovered once at startup and can be manually refreshed.
pub struct RuneToolRegistry {
    /// Registered tools by name
    tools: Arc<RwLock<HashMap<String, RuneTool>>>,
    /// Executor for running tools
    executor: RuneExecutor,
    /// Discovery configuration
    config: RuneDiscoveryConfig,
}

impl RuneToolRegistry {
    /// Create a new registry with the given configuration
    pub fn new(config: RuneDiscoveryConfig) -> Result<Self, RuneError> {
        let executor = RuneExecutor::new()?;
        Ok(Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            executor,
            config,
        })
    }

    /// Discover and register all tools from configured directories
    pub async fn discover(&self) -> Result<usize, RuneError> {
        let discovery = ToolDiscovery::new(self.config.clone());
        let discovered = discovery.discover_all()?;

        let mut tools = self.tools.write().await;
        tools.clear();

        let count = discovered.len();
        for tool in discovered {
            let name = format!("rune_{}", tool.name);
            debug!("Registering Rune tool: {}", name);
            tools.insert(name, tool);
        }

        info!("Discovered and registered {} Rune tools", count);
        Ok(count)
    }

    /// Create a registry and discover tools
    pub async fn discover_from(config: RuneDiscoveryConfig) -> Result<Self, RuneError> {
        let registry = Self::new(config)?;
        registry.discover().await?;
        Ok(registry)
    }

    /// Get a tool by name
    pub async fn get_tool(&self, name: &str) -> Option<RuneTool> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// List all registered tools
    pub async fn list_tools(&self) -> Vec<RuneTool> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }

    /// Get tool count
    pub async fn tool_count(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// Get tool names
    pub async fn tool_names(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        tools.keys().cloned().collect()
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        tool_name: &str,
        args: Value,
    ) -> Result<RuneExecutionResult, RuneError> {
        let tool = self.get_tool(tool_name).await.ok_or_else(|| {
            RuneError::NotFound(format!("Tool '{}' not found", tool_name))
        })?;

        self.executor.execute(&tool, args).await
    }

    /// Check if a tool exists
    pub async fn has_tool(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }

    /// Manually register a tool
    pub async fn register(&self, tool: RuneTool) -> Result<(), RuneError> {
        let name = format!("rune_{}", tool.name);
        let mut tools = self.tools.write().await;
        tools.insert(name, tool);
        Ok(())
    }

    /// Unregister a tool
    pub async fn unregister(&self, name: &str) -> bool {
        let mut tools = self.tools.write().await;
        tools.remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_registry_creation() {
        let config = RuneDiscoveryConfig::default();
        let registry = RuneToolRegistry::new(config);
        assert!(registry.is_ok());
    }

    #[tokio::test]
    async fn test_registry_discover_empty() {
        let temp = TempDir::new().unwrap();
        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let registry = RuneToolRegistry::new(config).unwrap();
        let count = registry.discover().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_registry_discover_tool() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("hello.rn"), "//! Say hello\npub fn main() {}").unwrap();

        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let registry = RuneToolRegistry::discover_from(config).await.unwrap();

        assert_eq!(registry.tool_count().await, 1);
        assert!(registry.has_tool("rune_hello").await);
    }

    #[tokio::test]
    async fn test_registry_list_tools() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("a.rn"), "pub fn main() {}").unwrap();
        std::fs::write(temp.path().join("b.rn"), "pub fn main() {}").unwrap();

        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let registry = RuneToolRegistry::discover_from(config).await.unwrap();

        let tools = registry.list_tools().await;
        assert_eq!(tools.len(), 2);
    }
}
