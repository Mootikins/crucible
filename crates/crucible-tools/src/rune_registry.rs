//! Tool registry for managing Rune tools
//!
//! This module provides a registry for discovering, loading, and managing
//! Rune tools with support for hot-reloading, caching, and service integration.

use crate::errors::{ContextualError, ErrorContext, RuneError};
use crate::tool::RuneTool;
use crate::types::{LoadingStatus, ToolLoadingResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{info, warn, error};

/// Tool registry for managing Rune tools
pub struct RuneToolRegistry {
    /// Registered tools by name
    tools: Arc<RwLock<HashMap<String, Arc<RuneTool>>>>,
    /// Tools by file path for hot-reloading
    tools_by_path: Arc<RwLock<HashMap<std::path::PathBuf, Vec<String>>>>,
    /// Tool categories
    categories: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Tool tags
    tags: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Loading history
    loading_history: Arc<Mutex<Vec<ToolLoadingResult>>>,
    /// Rune context for compilation
    context: Arc<rune::Context>,
    /// Registry configuration
    config: RegistryConfig,
}

/// Registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Maximum number of cached tools
    pub max_cached_tools: usize,
    /// Whether to enable hot-reload
    pub enable_hot_reload: bool,
    /// Whether to validate tools on load
    pub validate_on_load: bool,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
    /// Whether to track loading history
    pub track_loading_history: bool,
    /// Maximum loading history size
    pub max_loading_history: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            max_cached_tools: 1000,
            enable_hot_reload: true,
            validate_on_load: true,
            cache_ttl_secs: 3600, // 1 hour
            track_loading_history: true,
            max_loading_history: 1000,
        }
    }
}

impl RuneToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Result<Self, RuneError> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        Self::with_context(context, RegistryConfig::default())
    }

    /// Create a new tool registry with custom context
    pub fn with_context(context: Arc<rune::Context>, config: RegistryConfig) -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            tools_by_path: Arc::new(RwLock::new(HashMap::new())),
            categories: Arc::new(RwLock::new(HashMap::new())),
            tags: Arc::new(RwLock::new(HashMap::new())),
            loading_history: Arc::new(Mutex::new(Vec::new())),
            context,
            config,
        }
    }

    /// Register a tool
    pub async fn register_tool(&self, tool: RuneTool) -> Result<String, ContextualError> {
        let start_time = std::time::Instant::now();
        let tool_name = tool.name.clone();

        let context = ErrorContext::new()
            .with_operation("register_tool")
            .with_tool_name(&tool_name);

        // Check if tool already exists
        {
            let tools = self.tools.read().await;
            if tools.contains_key(&tool_name) {
                return Err(ContextualError::new(
                    RuneError::RegistryError {
                        message: format!("Tool '{}' already registered", tool_name),
                        operation: Some("register".to_string()),
                    },
                    context,
                ));
            }
        }

        // Validate tool if enabled
        if self.config.validate_on_load {
            if let Err(e) = self.validate_tool(&tool) {
                let loading_result = ToolLoadingResult {
                    status: LoadingStatus::Error,
                    tool: None,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                    warnings: Vec::new(),
                };

                self.record_loading_result(loading_result).await;

                return Err(ContextualError::new(
                    RuneError::LoadingError {
                        tool_name: tool_name.clone(),
                        source: e,
                    },
                    context,
                ));
            }
        }

        // Check cache size limit
        {
            let mut tools = self.tools.write().await;
            if tools.len() >= self.config.max_cached_tools {
                // Remove oldest tool (simple LRU)
                if let Some(oldest_key) = tools.keys().next() {
                    let oldest_name = oldest_key.clone();
                    tools.remove(&oldest_name);
                    warn!("Evicted tool '{}' from cache due to size limit", oldest_name);
                }
            }

            // Add tool to registry
            let tool_arc = Arc::new(tool);
            tools.insert(tool_name.clone(), tool_arc.clone());
        }

        // Update indexes
        self.update_indexes(&tool_name, &tool).await;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let loading_result = ToolLoadingResult {
            status: LoadingStatus::Success,
            tool: Some(tool.to_tool_definition()),
            duration_ms,
            error: None,
            warnings: Vec::new(),
        };

        self.record_loading_result(loading_result).await;

        info!("Successfully registered tool '{}'", tool_name);
        Ok(tool_name)
    }

    /// Unregister a tool
    pub async fn unregister_tool(&self, tool_name: &str) -> Result<bool, ContextualError> {
        let _context = ErrorContext::new()
            .with_operation("unregister_tool")
            .with_tool_name(tool_name);

        let mut tools = self.tools.write().await;
        if let Some(tool) = tools.remove(tool_name) {
            // Update indexes
            self.remove_from_indexes(tool_name, &tool).await;

            info!("Successfully unregistered tool '{}'", tool_name);
            Ok(true)
        } else {
            warn!("Tool '{}' not found for unregistration", tool_name);
            Ok(false)
        }
    }

    /// Get a tool by name
    pub async fn get_tool(&self, tool_name: &str) -> Result<Option<Arc<RuneTool>>, ContextualError> {
        let tools = self.tools.read().await;
        Ok(tools.get(tool_name).cloned())
    }

    /// Get the Rune context
    ///
    /// **DEPRECATED**: This method is deprecated and will be removed in a future version.
    /// Use ContextFactory::create_fresh_context() instead for creating fresh contexts per execution.
    #[deprecated(note = "Use ContextFactory::create_fresh_context() instead for better isolation")]
    pub fn get_context(&self) -> Arc<rune::Context> {
        warn!("get_context() is deprecated. Use ContextFactory::create_fresh_context() instead.");
        self.context.clone()
    }

    /// List all registered tools
    pub async fn list_tools(&self) -> Result<Vec<Arc<RuneTool>>, ContextualError> {
        let tools = self.tools.read().await;
        Ok(tools.values().cloned().collect())
    }

    /// List tools by category
    pub async fn list_tools_by_category(&self, category: &str) -> Result<Vec<Arc<RuneTool>>, ContextualError> {
        let categories = self.categories.read().await;
        if let Some(tool_names) = categories.get(category) {
            let tools = self.tools.read().await;
            let mut result = Vec::new();
            for tool_name in tool_names {
                if let Some(tool) = tools.get(tool_name) {
                    result.push(tool.clone());
                }
            }
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    /// Find tools by tag
    pub async fn find_tools_by_tag(&self, tag: &str) -> Result<Vec<Arc<RuneTool>>, ContextualError> {
        let tags = self.tags.read().await;
        if let Some(tool_names) = tags.get(tag) {
            let tools = self.tools.read().await;
            let mut result = Vec::new();
            for tool_name in tool_names {
                if let Some(tool) = tools.get(tool_name) {
                    result.push(tool.clone());
                }
            }
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    /// Search tools by name or description
    pub async fn search_tools(&self, query: &str) -> Result<Vec<Arc<RuneTool>>, ContextualError> {
        let tools = self.tools.read().await;
        let query_lower = query.to_lowercase();

        let mut results = Vec::new();
        for tool in tools.values() {
            if tool.name.to_lowercase().contains(&query_lower)
                || tool.description.to_lowercase().contains(&query_lower)
            {
                results.push(tool.clone());
            }
        }

        Ok(results)
    }

    /// Check if a tool needs reloading
    pub async fn needs_reload(&self, tool_name: &str) -> Result<bool, ContextualError> {
        let tools = self.tools.read().await;
        if let Some(tool) = tools.get(tool_name) {
            tool.needs_reload().await
        } else {
            Ok(false)
        }
    }

    /// Reload a tool from its source file
    pub async fn reload_tool(&self, tool_name: &str) -> Result<bool, ContextualError> {
        let context = ErrorContext::new()
            .with_operation("reload_tool")
            .with_tool_name(tool_name);

        let tools = self.tools.read().await;
        if let Some(tool) = tools.get(tool_name) {
            let tool_clone = Arc::clone(tool);
            drop(tools);

            match tool_clone.reload(&self.context).await {
                Ok(_) => {
                    info!("Successfully reloaded tool '{}'", tool_name);
                    Ok(true)
                }
                Err(e) => {
                    error!("Failed to reload tool '{}': {}", tool_name, e);
                    Err(ContextualError::new(
                        RuneError::HotReloadError {
                            message: format!("Failed to reload tool: {}", e),
                            file_path: tool_clone.file_path.clone(),
                        },
                        context,
                    ))
                }
            }
        } else {
            warn!("Tool '{}' not found for reloading", tool_name);
            Ok(false)
        }
    }

    /// Reload all tools that need reloading
    pub async fn reload_tools(&self) -> Result<usize, ContextualError> {
        let mut reloaded_count = 0;
        let tools = self.tools.read().await;
        let tool_names: Vec<String> = tools.keys().cloned().collect();
        drop(tools);

        for tool_name in tool_names {
            if self.needs_reload(&tool_name).await? {
                if self.reload_tool(&tool_name).await? {
                    reloaded_count += 1;
                }
            }
        }

        if reloaded_count > 0 {
            info!("Reloaded {} tools", reloaded_count);
        }

        Ok(reloaded_count)
    }

    /// Get registry statistics
    pub async fn get_stats(&self) -> RegistryStats {
        let tools = self.tools.read().await;
        let categories = self.categories.read().await;
        let tags = self.tags.read().await;

        RegistryStats {
            total_tools: tools.len(),
            enabled_tools: tools.values().filter(|t| t.enabled).count(),
            categories: categories.len(),
            total_tags: tags.len(),
            tools_with_files: tools.values().filter(|t| t.file_path.is_some()).count(),
            loading_history_size: {
                let history = self.loading_history.lock().await;
                history.len()
            },
        }
    }

    /// Get loading history
    pub async fn get_loading_history(&self) -> Vec<ToolLoadingResult> {
        let history = self.loading_history.lock().await;
        history.clone()
    }

    /// Clear loading history
    pub async fn clear_loading_history(&self) {
        let mut history = self.loading_history.lock().await;
        history.clear();
    }

    /// Validate a tool
    fn validate_tool(&self, tool: &RuneTool) -> Result<(), RuneError> {
        // Check tool name
        if tool.name.is_empty() {
            return Err(RuneError::ValidationError {
                message: "Tool name cannot be empty".to_string(),
                field: Some("name".to_string()),
                value: None,
            });
        }

        // Check description
        if tool.description.is_empty() {
            return Err(RuneError::ValidationError {
                message: "Tool description cannot be empty".to_string(),
                field: Some("description".to_string()),
                value: None,
            });
        }

        // Check source code
        if tool.source_code.is_empty() {
            return Err(RuneError::ValidationError {
                message: "Tool source code cannot be empty".to_string(),
                field: Some("source_code".to_string()),
                value: None,
            });
        }

        Ok(())
    }

    /// Update indexes for a tool
    async fn update_indexes(&self, tool_name: &str, tool: &RuneTool) {
        // Update category index
        if !tool.category.is_empty() {
            let mut categories = self.categories.write().await;
            categories
                .entry(tool.category.clone())
                .or_insert_with(Vec::new)
                .push(tool_name.to_string());
        }

        // Update tag indexes
        for tag in &tool.tags {
            let mut tags = self.tags.write().await;
            tags
                .entry(tag.clone())
                .or_insert_with(Vec::new)
                .push(tool_name.to_string());
        }

        // Update file path index
        if let Some(ref file_path) = tool.file_path {
            let mut tools_by_path = self.tools_by_path.write().await;
            tools_by_path
                .entry(file_path.clone())
                .or_insert_with(Vec::new)
                .push(tool_name.to_string());
        }
    }

    /// Remove tool from indexes
    async fn remove_from_indexes(&self, tool_name: &str, tool: &RuneTool) {
        // Remove from category index
        if !tool.category.is_empty() {
            let mut categories = self.categories.write().await;
            if let Some(tools) = categories.get_mut(&tool.category) {
                tools.retain(|name| name != tool_name);
                if tools.is_empty() {
                    categories.remove(&tool.category);
                }
            }
        }

        // Remove from tag indexes
        for tag in &tool.tags {
            let mut tags = self.tags.write().await;
            if let Some(tools) = tags.get_mut(tag) {
                tools.retain(|name| name != tool_name);
                if tools.is_empty() {
                    tags.remove(tag);
                }
            }
        }

        // Remove from file path index
        if let Some(ref file_path) = tool.file_path {
            let mut tools_by_path = self.tools_by_path.write().await;
            if let Some(tools) = tools_by_path.get_mut(file_path) {
                tools.retain(|name| name != tool_name);
                if tools.is_empty() {
                    tools_by_path.remove(file_path);
                }
            }
        }
    }

    /// Record loading result
    async fn record_loading_result(&self, result: ToolLoadingResult) {
        if !self.config.track_loading_history {
            return;
        }

        let mut history = self.loading_history.lock().await;
        history.push(result);

        // Trim history if it exceeds maximum size
        if history.len() > self.config.max_loading_history {
            history.drain(0..history.len() - self.config.max_loading_history);
        }
    }
}

impl Default for RuneToolRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create default registry")
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    /// Total number of registered tools
    pub total_tools: usize,
    /// Number of enabled tools
    pub enabled_tools: usize,
    /// Number of categories
    pub categories: usize,
    /// Total number of unique tags
    pub total_tags: usize,
    /// Number of tools loaded from files
    pub tools_with_files: usize,
    /// Loading history size
    pub loading_history_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DiscoveredTool;
    use crate::types::ToolMetadata;

    #[tokio::test]
    async fn test_registry_basic_operations() {
        let registry = RuneToolRegistry::new().unwrap();

        // Create a mock tool
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() {
                #{ type: "object", properties: #{ name: #{ type: "string" } } }
            }
            pub async fn call(args) {
                #{ success: true, message: `Hello ${args.name}` }
            }
        "#;

        let tool = RuneTool::from_source(tool_source, &registry.context, None).unwrap();

        // Register tool
        let tool_name = registry.register_tool(tool).await.unwrap();
        assert_eq!(tool_name, "test_tool");

        // Get tool
        let retrieved_tool = registry.get_tool(&tool_name).await.unwrap();
        assert!(retrieved_tool.is_some());
        assert_eq!(retrieved_tool.unwrap().name, "test_tool");

        // List tools
        let tools = registry.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);

        // Unregister tool
        let unregistered = registry.unregister_tool(&tool_name).await.unwrap();
        assert!(unregistered);

        // Verify tool is gone
        let retrieved_tool = registry.get_tool(&tool_name).await.unwrap();
        assert!(retrieved_tool.is_none());
    }

    #[tokio::test]
    async fn test_registry_search_and_filtering() {
        let registry = RuneToolRegistry::new().unwrap();

        // Register multiple tools
        let tools = vec![
            create_test_tool("search_tool", "A tool for searching", "search"),
            create_test_tool("file_tool", "A tool for files", "file"),
            create_test_tool("test_tool", "A test tool", "test"),
        ];

        for tool in tools {
            let _ = registry.register_tool(tool).await;
        }

        // Search by name
        let search_results = registry.search_tools("search").await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].name, "search_tool");

        // Search by description
        let search_results = registry.search_tools("test").await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].name, "test_tool");

        // Find by category
        let category_results = registry.list_tools_by_category("search").await.unwrap();
        assert_eq!(category_results.len(), 1);

        // Find by tag
        let tag_results = registry.find_tools_by_tag("test").await.unwrap();
        assert_eq!(tag_results.len(), 1);
    }

    #[tokio::test]
    async fn test_registry_stats() {
        let registry = RuneToolRegistry::new().unwrap();

        // Register tools
        let tool1 = create_test_tool("tool1", "Tool 1", "category1");
        let tool2 = create_test_tool("tool2", "Tool 2", "category2");

        registry.register_tool(tool1).await.unwrap();
        registry.register_tool(tool2).await.unwrap();

        let stats = registry.get_stats().await;
        assert_eq!(stats.total_tools, 2);
        assert_eq!(stats.enabled_tools, 2);
        assert_eq!(stats.categories, 2);
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let registry = RuneToolRegistry::new().unwrap();

        let tool = create_test_tool("duplicate_tool", "A duplicate tool", "test");
        let tool_name = registry.register_tool(tool.clone()).await.unwrap();

        // Try to register again
        let result = registry.register_tool(tool).await;
        assert!(result.is_err());
    }

    fn create_test_tool(name: &str, description: &str, category: &str) -> RuneTool {
        let tool_source = format!(r#"
            pub fn NAME() { "{}" }
            pub fn DESCRIPTION() { "{}" }
            pub fn INPUT_SCHEMA() {{
                #{{ type: "object", properties: {{}} }}
            }}
            pub async fn call(args) {{
                #{{ success: true }}
            }}
        "#, name, description);

        let mut tool = RuneTool::from_source(&tool_source, &rune::Context::with_default_modules().unwrap(), None).unwrap();
        tool.category = category.to_string();
        tool.tags = vec!["test".to_string()];
        tool
    }
}