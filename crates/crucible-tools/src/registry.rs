//! Tool registry for managing static tools
//!
//! This module provides a registry for static tool definitions and management
//! that integrates with the broader crucible-services architecture.

use crate::types::{ToolCategory, ToolDependency};
use crucible_services::types::tool::{ToolDefinition, ToolExecutionContext, ToolExecutionResult, ContextRef};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Static tool registry for managing built-in tools
pub struct ToolRegistry {
    /// Registered tools by name
    pub tools: HashMap<String, ToolDefinition>,
    /// Tool categories
    pub categories: HashMap<ToolCategory, Vec<String>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register_tool(&mut self, tool: ToolDefinition) {
        let name = tool.name.clone();

        // Add to tools map
        self.tools.insert(name.clone(), tool.clone());

        // Add to category mapping
        if let Some(category_str) = &tool.category {
            if let Ok(category) = category_str.parse() {
                self.categories
                    .entry(category)
                    .or_insert_with(Vec::new)
                    .push(name);
            }
        }

        debug!("Registered tool: {}", name);
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// List all tools
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// List tools by category
    pub fn list_tools_by_category(&self, category: &ToolCategory) -> Vec<&ToolDefinition> {
        if let Some(tool_names) = self.categories.get(category) {
            tool_names
                .iter()
                .filter_map(|name| self.tools.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get tool dependencies
    pub fn get_tool_dependencies(&self, tool_name: &str) -> Vec<ToolDependency> {
        self.tools
            .get(tool_name)
            .map(|tool| {
                // Extract dependencies from tool metadata or parameters
                // This is a placeholder implementation
                Vec::new()
            })
            .unwrap_or_default()
    }

    /// Validate tool dependencies
    pub fn validate_dependencies(&self, tool_name: &str) -> Result<(), String> {
        let dependencies = self.get_tool_dependencies(tool_name);

        for dep in dependencies {
            if !dep.optional {
                // Check if dependency tool is registered
                if !self.tools.contains_key(&dep.name) {
                    return Err(format!(
                        "Missing required dependency: {} (version: {:?})",
                        dep.name,
                        dep.version
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get registry statistics
    pub fn get_stats(&self) -> RegistryStats {
        RegistryStats {
            total_tools: self.tools.len(),
            categories: self.categories.len(),
            tools_with_dependencies: self.tools
                .values()
                .filter(|t| !self.get_tool_dependencies(&t.name).is_empty())
                .count(),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    /// Total number of registered tools
    pub total_tools: usize,
    /// Number of categories
    pub categories: usize,
    /// Number of tools with dependencies
    pub tools_with_dependencies: usize,
}

/// Initialize the tool registry with built-in tools
pub fn initialize_registry() -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();

    // Register built-in system tools
    register_system_tools(&mut registry);
    register_vault_tools(&mut registry);
    register_database_tools(&mut registry);
    register_search_tools(&mut registry);

    let registry = Arc::new(registry);

    info!(
        "Initialized tool registry with {} tools across {} categories",
        registry.tools.len(),
        registry.categories.len()
    );

    registry
}

/// Register system tools
fn register_system_tools(registry: &mut ToolRegistry) {
    let system_tools = vec![
        ToolDefinition {
            name: "system_info".to_string(),
            description: "Get system information".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["system".to_string(), "info".to_string()],
            enabled: true,
            parameters: vec![],
        },
        ToolDefinition {
            name: "file_list".to_string(),
            description: "List files in a directory".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to list recursively",
                        "default": false
                    }
                },
                "required": ["path"]
            }),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["system".to_string(), "file".to_string()],
            enabled: true,
            parameters: vec![],
        },
    ];

    for tool in system_tools {
        registry.register_tool(tool);
    }
}

/// Register vault tools
fn register_vault_tools(registry: &mut ToolRegistry) {
    let vault_tools = vec![
        ToolDefinition {
            name: "vault_search".to_string(),
            description: "Search vault content".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to search within (optional)"
                    }
                },
                "required": ["query"]
            }),
            category: Some("Vault".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["vault".to_string(), "search".to_string()],
            enabled: true,
            parameters: vec![],
        },
    ];

    for tool in vault_tools {
        registry.register_tool(tool);
    }
}

/// Register database tools
fn register_database_tools(registry: &mut ToolRegistry) {
    let database_tools = vec![
        ToolDefinition {
            name: "database_query".to_string(),
            description: "Execute database query".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "SQL query to execute"
                    },
                    "parameters": {
                        "type": "array",
                        "description": "Query parameters",
                        "items": {}
                    }
                },
                "required": ["query"]
            }),
            category: Some("Database".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["database".to_string(), "query".to_string()],
            enabled: true,
            parameters: vec![],
        },
    ];

    for tool in database_tools {
        registry.register_tool(tool);
    }
}

/// Register search tools
fn register_search_tools(registry: &mut ToolRegistry) {
    let search_tools = vec![
        ToolDefinition {
            name: "semantic_search".to_string(),
            description: "Perform semantic search".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
            category: Some("Search".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["search".to_string(), "semantic".to_string()],
            enabled: true,
            parameters: vec![],
        },
    ];

    for tool in search_tools {
        registry.register_tool(tool);
    }
}

/// Create a tool manager from the registry
pub fn create_tool_manager_from_registry() -> crate::system_tools::ToolManager {
    let mut manager = crate::system_tools::ToolManager::new();

    // Note: This is a simplified implementation
    // In a full implementation, you would create actual tool implementations
    // for each registered tool definition

    manager
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tools.len(), 0);
        assert_eq!(registry.categories.len(), 0);
    }

    #[test]
    fn test_tool_registration() {
        let mut registry = ToolRegistry::new();

        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            input_schema: json!({}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool);

        assert_eq!(registry.tools.len(), 1);
        assert!(registry.get_tool("test_tool").is_some());
        assert_eq!(registry.list_tools_by_category(&ToolCategory::System).len(), 1);
    }

    #[test]
    fn test_registry_stats() {
        let mut registry = ToolRegistry::new();

        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            input_schema: json!({}),
            category: Some("System".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            tags: vec![],
            enabled: true,
            parameters: vec![],
        };

        registry.register_tool(tool);

        let stats = registry.get_stats();
        assert_eq!(stats.total_tools, 1);
        assert_eq!(stats.categories, 1);
        assert_eq!(stats.tools_with_dependencies, 0);
    }
}