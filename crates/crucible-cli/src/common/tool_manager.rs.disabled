//! Simple Tool Registry for Crucible CLI
//!
//! This module provides a simple, production-proven tool registry pattern
//! based on research of successful agentic frameworks (LangChain, OpenAI Swarm, etc.).
//! Uses direct function registration and execution with no global state or complex caching.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info};

/// Simple tool registry following production patterns from successful agentic frameworks
///
/// This is a straightforward function registry with no global state, caching, or
/// lifecycle management - just like the patterns used in LangChain, OpenAI Swarm, etc.
pub struct ToolRegistry {
    /// Registered tools by name
    tools: HashMap<String, ToolDefinition>,
    /// Kiln path for tool execution context
    kiln_path: Option<PathBuf>,
    /// Whether crucible-tools library is initialized
    initialized: bool,
}

/// Definition of a tool in the registry
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool category/group
    pub category: String,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        info!("Creating new ToolRegistry");
        Self {
            tools: HashMap::new(),
            kiln_path: None,
            initialized: false,
        }
    }

    /// Create a tool registry with a specific kiln path
    pub fn with_kiln_path(kiln_path: PathBuf) -> Self {
        info!("Creating ToolRegistry with kiln path: {:?}", kiln_path);
        Self {
            tools: HashMap::new(),
            kiln_path: Some(kiln_path),
            initialized: false,
        }
    }

    /// Initialize the crucible-tools library and discover available tools
    pub async fn ensure_initialized(&mut self) -> Result<()> {
        if self.initialized {
            debug!("Tools already initialized, skipping initialization");
            return Ok(());
        }

        info!("=== Initializing Tool Registry ===");
        debug!("Starting crucible-tools library initialization...");

        // Set up kiln path if provided
        if let Some(ref kiln_path) = self.kiln_path {
            use crucible_tools::types::{set_tool_context, ToolConfigContext};
            set_tool_context(ToolConfigContext::with_kiln_path(kiln_path.clone()));
            debug!("✓ Set kiln path: {:?}", kiln_path);
        }

        // Initialize crucible-tools library
        crucible_tools::init();
        debug!("✓ crucible-tools::init() completed");

        // Load all tools and populate registry
        debug!("Loading all tools via crucible_tools::load_all_tools()...");
        match crucible_tools::load_all_tools().await {
            Ok(_) => {
                debug!("✓ Successfully loaded all tools");

                // Discover and register available tools
                let tool_names = crucible_tools::list_registered_tools().await;
                info!(
                    "✓ Discovered {} tools from crucible-tools",
                    tool_names.len()
                );

                for tool_name in &tool_names {
                    self.tools.insert(
                        tool_name.clone(),
                        ToolDefinition {
                            name: tool_name.clone(),
                            description: format!("Tool: {}", tool_name),
                            category: self.categorize_tool(tool_name),
                        },
                    );
                }

                if !tool_names.is_empty() {
                    debug!("Registered tools: {:?}", tool_names);
                }
            }
            Err(e) => {
                error!("✗ Failed to load crucible-tools: {}", e);
                return Err(anyhow::anyhow!("Failed to load crucible-tools: {}", e));
            }
        }

        self.initialized = true;
        info!("=== Tool Registry Initialization Complete ===");
        Ok(())
    }

    /// Execute a tool by name with parameters
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<crucible_tools::ToolResult> {
        // Check if tool is registered
        if !self.tools.contains_key(tool_name) {
            return Err(anyhow::anyhow!(
                "Tool '{}' not found in registry",
                tool_name
            ));
        }

        debug!(
            "Executing tool: {} with params: {:?}",
            tool_name, parameters
        );

        // Execute the tool directly through crucible-tools
        let result =
            crucible_tools::execute_tool(tool_name.to_string(), parameters, user_id, session_id)
                .await
                .map_err(|e| {
                    error!("Tool execution failed for '{}': {}", tool_name, e);
                    anyhow::anyhow!("Tool execution failed: {}", e)
                })?;

        Ok(result)
    }

    /// Get list of available tool names
    pub fn list_tools(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get tools grouped by category
    pub fn list_tools_by_group(&self) -> HashMap<String, Vec<String>> {
        let mut grouped = HashMap::new();
        for tool_def in self.tools.values() {
            grouped
                .entry(tool_def.category.clone())
                .or_insert_with(Vec::new)
                .push(tool_def.name.clone());
        }
        grouped
    }

    /// Get tool definition by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Register a custom tool
    pub fn register_tool(&mut self, tool_def: ToolDefinition) {
        info!("Registering custom tool: {}", tool_def.name);
        self.tools.insert(tool_def.name.clone(), tool_def);
    }

    /// Get registry statistics
    pub fn get_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();
        stats.insert("total_tools".to_string(), self.tools.len().to_string());
        stats.insert("initialized".to_string(), self.initialized.to_string());
        stats.insert(
            "has_kiln_path".to_string(),
            self.kiln_path.is_some().to_string(),
        );

        if let Some(ref path) = self.kiln_path {
            stats.insert("kiln_path".to_string(), path.display().to_string());
        }

        let mut category_counts = HashMap::new();
        for tool_def in self.tools.values() {
            *category_counts.entry(&tool_def.category).or_insert(0) += 1;
        }

        for (category, count) in category_counts {
            stats.insert(format!("category_{}", category), count.to_string());
        }

        stats
    }

    /// Categorize a tool based on its name
    fn categorize_tool(&self, tool_name: &str) -> String {
        if tool_name.starts_with("system_") {
            "system".to_string()
        } else if tool_name.starts_with("search_")
            || tool_name.starts_with("semantic_")
            || tool_name.starts_with("rebuild_")
            || tool_name.starts_with("get_index")
            || tool_name.starts_with("optimize_")
            || tool_name.starts_with("advanced_")
        {
            "search".to_string()
        } else if tool_name.starts_with("get_")
            || tool_name.starts_with("create_")
            || tool_name.starts_with("update_")
            || tool_name.starts_with("delete_")
            || tool_name.starts_with("list_")
        {
            "kiln".to_string()
        } else if tool_name.contains("note")
            || tool_name.contains("content")
            || tool_name.contains("filename")
            || tool_name.contains("sync")
            || tool_name.contains("properties")
        {
            "database".to_string()
        } else {
            "other".to_string()
        }
    }
}

/// Type alias for backward compatibility
pub type CrucibleToolManager = ToolRegistry;

/// Convenience functions for creating registries
impl ToolRegistry {
    /// Create a new registry (alias for consistency)
    pub fn create() -> Self {
        Self::new()
    }

    /// Create a new registry with kiln path
    pub fn create_with_kiln_path(kiln_path: PathBuf) -> Self {
        Self::with_kiln_path(kiln_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert!(!registry.initialized);
        assert!(registry.tools.is_empty());
        assert!(registry.kiln_path.is_none());
    }

    #[tokio::test]
    async fn test_registry_with_kiln_path() {
        let path = PathBuf::from("/test/path");
        let registry = ToolRegistry::with_kiln_path(path.clone());
        assert_eq!(registry.kiln_path, Some(path));
    }

    #[tokio::test]
    async fn test_initialization() {
        let mut registry = ToolRegistry::new();

        // Should initialize successfully
        assert!(registry.ensure_initialized().await.is_ok());
        assert!(registry.initialized);

        // Should not initialize again
        assert!(registry.ensure_initialized().await.is_ok());
    }

    #[tokio::test]
    async fn test_tool_listing() {
        let mut registry = ToolRegistry::new();
        registry.ensure_initialized().await.unwrap();

        let tools = registry.list_tools();
        assert!(!tools.is_empty());

        let grouped = registry.list_tools_by_group();
        assert!(!grouped.is_empty());
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let mut registry = ToolRegistry::new();
        registry.ensure_initialized().await.unwrap();

        let result = registry
            .execute_tool(
                "system_info",
                json!({}),
                Some("test_user".to_string()),
                Some("test_session".to_string()),
            )
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_custom_tool_registration() {
        let mut registry = ToolRegistry::new();

        let tool_def = ToolDefinition {
            name: "custom_tool".to_string(),
            description: "A custom test tool".to_string(),
            category: "test".to_string(),
        };

        registry.register_tool(tool_def);

        let retrieved = registry.get_tool("custom_tool");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "custom_tool");
    }

    #[tokio::test]
    async fn test_stats() {
        let mut registry = ToolRegistry::new();
        registry.ensure_initialized().await.unwrap();

        let stats = registry.get_stats();
        assert!(!stats.is_empty());
        assert!(stats.contains_key("total_tools"));
        assert!(stats.contains_key("initialized"));
    }

    #[tokio::test]
    async fn test_tool_categorization() {
        let registry = ToolRegistry::new();

        assert_eq!(registry.categorize_tool("system_info"), "system");
        assert_eq!(registry.categorize_tool("search_files"), "search");
        assert_eq!(registry.categorize_tool("get_document"), "kiln");
        assert_eq!(registry.categorize_tool("sync_content"), "database");
        assert_eq!(registry.categorize_tool("random_tool"), "other");
    }
}
