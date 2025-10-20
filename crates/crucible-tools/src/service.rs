//! Service layer integration
//!
//! This module provides the service layer integration that connects the tools
//! to the broader Crucible service architecture, including database services,
//! search services, and external integrations.

use crate::system_tools::ToolManager;
use crate::types::{ToolCategory, ToolDefinition, ToolExecutionContext, ToolExecutionResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Tool service interface
#[async_trait]
pub trait ToolService: Send + Sync {
    /// Execute a tool by name
    async fn execute_tool(
        &self,
        tool_name: &str,
        params: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult>;

    /// List all available tools
    async fn list_tools(&self) -> Result<Vec<ToolDefinition>>;

    /// Get a specific tool definition
    async fn get_tool(&self, name: &str) -> Result<Option<ToolDefinition>>;

    /// Search tools by query
    async fn search_tools(&self, query: &str) -> Result<Vec<ToolDefinition>>;

    /// Get tools by category
    async fn get_tools_by_category(&self, category: &ToolCategory) -> Result<Vec<ToolDefinition>>;

    /// Check if a tool exists
    async fn tool_exists(&self, name: &str) -> Result<bool>;

    /// Get tool categories
    async fn get_categories(&self) -> Result<Vec<ToolCategory>>;
}

/// System tool service implementation
pub struct SystemToolService {
    tool_manager: Arc<ToolManager>,
}

impl SystemToolService {
    pub fn new() -> Self {
        let mut tool_manager = ToolManager::new();

        // Register all tool categories
        crate::vault_tools::register_vault_tools(&mut tool_manager);
        crate::database_tools::register_database_tools(&mut tool_manager);
        crate::search_tools::register_search_tools(&mut tool_manager);

        Self {
            tool_manager: Arc::new(tool_manager),
        }
    }

    pub fn with_manager(tool_manager: Arc<ToolManager>) -> Self {
        Self { tool_manager }
    }

    /// Create a new service with custom tool registration
    pub fn with_custom_tools<F>(register_fn: F) -> Self
    where
        F: FnOnce(&mut ToolManager),
    {
        let mut tool_manager = ToolManager::new();
        register_fn(&mut tool_manager);

        Self {
            tool_manager: Arc::new(tool_manager),
        }
    }
}

impl Default for SystemToolService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolService for SystemToolService {
    async fn execute_tool(
        &self,
        tool_name: &str,
        params: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        info!("Executing tool: {} with context: {:?}", tool_name, context);

        let result = self.tool_manager.execute_tool(tool_name, params, context).await?;

        if result.success {
            debug!("Tool {} executed successfully", tool_name);
        } else {
            warn!("Tool {} execution failed: {:?}", tool_name, result.error);
        }

        Ok(result)
    }

    async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        Ok(self.tool_manager.list_tools().into_iter().cloned().collect())
    }

    async fn get_tool(&self, name: &str) -> Result<Option<ToolDefinition>> {
        Ok(self.tool_manager.get_tool_definition(name).cloned())
    }

    async fn search_tools(&self, query: &str) -> Result<Vec<ToolDefinition>> {
        Ok(self.tool_manager.search_tools(query).into_iter().cloned().collect())
    }

    async fn get_tools_by_category(&self, category: &ToolCategory) -> Result<Vec<ToolDefinition>> {
        Ok(self
            .tool_manager
            .list_tools_by_category(category)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn tool_exists(&self, name: &str) -> Result<bool> {
        Ok(self.tool_manager.get_tool_definition(name).is_some())
    }

    async fn get_categories(&self) -> Result<Vec<ToolCategory>> {
        let tools = self.tool_manager.list_tools();
        let mut categories = std::collections::HashSet::new();

        for tool in tools {
            categories.insert(tool.category.clone());
        }

        Ok(categories.into_iter().collect())
    }
}

/// Tool execution context builder
pub struct ExecutionContextBuilder {
    workspace_path: Option<String>,
    vault_path: Option<String>,
    user_id: Option<String>,
    session_id: Option<String>,
}

impl ExecutionContextBuilder {
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            vault_path: None,
            user_id: None,
            session_id: None,
        }
    }

    pub fn workspace_path<S: Into<String>>(mut self, path: S) -> Self {
        self.workspace_path = Some(path.into());
        self
    }

    pub fn vault_path<S: Into<String>>(mut self, path: S) -> Self {
        self.vault_path = Some(path.into());
        self
    }

    pub fn user_id<S: Into<String>>(mut self, id: S) -> Self {
        self.user_id = Some(id.into());
        self
    }

    pub fn session_id<S: Into<String>>(mut self, id: S) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn build(self) -> ToolExecutionContext {
        ToolExecutionContext {
            workspace_path: self.workspace_path,
            vault_path: self.vault_path,
            user_id: self.user_id,
            session_id: self.session_id,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl Default for ExecutionContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool service configuration
#[derive(Debug, Clone)]
pub struct ToolServiceConfig {
    /// Enable/disable specific tool categories
    pub enabled_categories: HashMap<ToolCategory, bool>,
    /// Tool execution timeout in milliseconds
    pub execution_timeout_ms: Option<u64>,
    /// Maximum number of concurrent tool executions
    pub max_concurrent_executions: Option<usize>,
    /// Enable tool usage metrics
    pub enable_metrics: bool,
    /// Custom tool registration hooks
    pub custom_registration_hooks: Vec<String>,
}

impl Default for ToolServiceConfig {
    fn default() -> Self {
        let mut enabled_categories = HashMap::new();
        enabled_categories.insert(ToolCategory::FileSystem, true);
        enabled_categories.insert(ToolCategory::Database, true);
        enabled_categories.insert(ToolCategory::Search, true);
        enabled_categories.insert(ToolCategory::Vault, true);
        enabled_categories.insert(ToolCategory::Semantic, true);
        enabled_categories.insert(ToolCategory::System, true);
        enabled_categories.insert(ToolCategory::Integration, true);

        Self {
            enabled_categories,
            execution_timeout_ms: Some(30000), // 30 seconds
            max_concurrent_executions: Some(10),
            enable_metrics: true,
            custom_registration_hooks: Vec::new(),
        }
    }
}

/// Enhanced tool service with configuration and metrics
pub struct ConfigurableToolService {
    inner: Arc<SystemToolService>,
    config: ToolServiceConfig,
    // Metrics could be added here
}

impl ConfigurableToolService {
    pub fn new(config: ToolServiceConfig) -> Self {
        Self {
            inner: Arc::new(SystemToolService::new()),
            config,
        }
    }

    pub fn with_service(service: Arc<SystemToolService>, config: ToolServiceConfig) -> Self {
        Self {
            inner: service,
            config,
        }
    }

    /// Check if a tool category is enabled
    pub fn is_category_enabled(&self, category: &ToolCategory) -> bool {
        self.config
            .enabled_categories
            .get(category)
            .copied()
            .unwrap_or(true)
    }

    /// Get service configuration
    pub fn config(&self) -> &ToolServiceConfig {
        &self.config
    }
}

#[async_trait]
impl ToolService for ConfigurableToolService {
    async fn execute_tool(
        &self,
        tool_name: &str,
        params: Value,
        context: ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        // Check if tool exists and is enabled
        let tool_def = self.inner.tool_manager.get_tool_definition(tool_name);
        if let Some(def) = tool_def {
            if !self.is_category_enabled(&def.category) {
                return Ok(ToolExecutionResult {
                    success: false,
                    data: None,
                    error: Some(format!(
                        "Tool category '{}' is disabled",
                        def.category
                    )),
                    execution_time_ms: None,
                });
            }
        }

        // Apply timeout if configured
        if let Some(timeout_ms) = self.config.execution_timeout_ms {
            // In a real implementation, you would use tokio::time::timeout here
            debug!("Tool execution timeout set to {}ms", timeout_ms);
        }

        // Execute the tool
        self.inner.execute_tool(tool_name, params, context).await
    }

    async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        let all_tools = self.inner.list_tools().await?;
        let enabled_tools: Vec<ToolDefinition> = all_tools
            .into_iter()
            .filter(|tool| self.is_category_enabled(&tool.category))
            .collect();

        Ok(enabled_tools)
    }

    async fn get_tool(&self, name: &str) -> Result<Option<ToolDefinition>> {
        if let Some(tool) = self.inner.get_tool(name).await? {
            if self.is_category_enabled(&tool.category) {
                Ok(Some(tool))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn search_tools(&self, query: &str) -> Result<Vec<ToolDefinition>> {
        let all_results = self.inner.search_tools(query).await?;
        let enabled_results: Vec<ToolDefinition> = all_results
            .into_iter()
            .filter(|tool| self.is_category_enabled(&tool.category))
            .collect();

        Ok(enabled_results)
    }

    async fn get_tools_by_category(&self, category: &ToolCategory) -> Result<Vec<ToolDefinition>> {
        if !self.is_category_enabled(category) {
            return Ok(Vec::new());
        }

        self.inner.get_tools_by_category(category).await
    }

    async fn tool_exists(&self, name: &str) -> Result<bool> {
        if let Some(tool) = self.inner.get_tool(name).await? {
            Ok(self.is_category_enabled(&tool.category))
        } else {
            Ok(false)
        }
    }

    async fn get_categories(&self) -> Result<Vec<ToolCategory>> {
        let all_categories = self.inner.get_categories().await?;
        let enabled_categories: Vec<ToolCategory> = all_categories
            .into_iter()
            .filter(|cat| self.is_category_enabled(cat))
            .collect();

        Ok(enabled_categories)
    }
}

/// Tool service factory for creating configured services
pub struct ToolServiceFactory;

impl ToolServiceFactory {
    /// Create a default tool service
    pub fn create_default() -> Arc<dyn ToolService> {
        Arc::new(SystemToolService::new())
    }

    /// Create a tool service with custom configuration
    pub fn create_with_config(config: ToolServiceConfig) -> Arc<dyn ToolService> {
        Arc::new(ConfigurableToolService::new(config))
    }

    /// Create a minimal tool service (only essential tools)
    pub fn create_minimal() -> Arc<dyn ToolService> {
        let mut config = ToolServiceConfig::default();
        // Only enable essential categories
        config.enabled_categories.clear();
        config.enabled_categories.insert(ToolCategory::Search, true);
        config.enabled_categories.insert(ToolCategory::Database, true);

        Arc::new(ConfigurableToolService::new(config))
    }

    /// Create a development tool service (all tools enabled, longer timeouts)
    pub fn create_development() -> Arc<dyn ToolService> {
        let mut config = ToolServiceConfig::default();
        config.execution_timeout_ms = Some(120000); // 2 minutes
        config.enable_metrics = false;

        Arc::new(ConfigurableToolService::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_tool_service() {
        let service = SystemToolService::new();

        // Test listing tools
        let tools = service.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        // Test getting tools by category
        let search_tools = service.get_tools_by_category(&ToolCategory::Search).await.unwrap();
        assert!(!search_tools.is_empty());

        // Test searching tools
        let found_tools = service.search_tools("search").await.unwrap();
        assert!(!found_tools.is_empty());

        // Test tool existence
        assert!(service.tool_exists("semantic_search").await.unwrap());
        assert!(!service.tool_exists("nonexistent_tool").await.unwrap());
    }

    #[tokio::test]
    async fn test_execution_context_builder() {
        let context = ExecutionContextBuilder::new()
            .workspace_path("/workspace")
            .vault_path("/vault")
            .user_id("user123")
            .session_id("session456")
            .build();

        assert_eq!(context.workspace_path, Some("/workspace".to_string()));
        assert_eq!(context.vault_path, Some("/vault".to_string()));
        assert_eq!(context.user_id, Some("user123".to_string()));
        assert_eq!(context.session_id, Some("session456".to_string()));
    }

    #[tokio::test]
    async fn test_configurable_tool_service() {
        let mut config = ToolServiceConfig::default();
        config.enabled_categories.insert(ToolCategory::Search, true);
        config.enabled_categories.insert(ToolCategory::Vault, false);

        let service = ConfigurableToolService::new(config);

        // Should return only search tools
        let tools = service.list_tools().await.unwrap();
        assert!(!tools.is_empty());
        assert!(tools.iter().all(|t| t.category == ToolCategory::Search));

        // Vault tools should not exist
        let vault_tools = service.get_tools_by_category(&ToolCategory::Vault).await.unwrap();
        assert!(vault_tools.is_empty());
    }

    #[test]
    fn test_tool_service_factory() {
        let default_service = ToolServiceFactory::create_default();
        let minimal_service = ToolServiceFactory::create_minimal();
        let dev_service = ToolServiceFactory::create_development();

        // All should be different implementations
        // (This is a basic sanity check)
        assert!(!Arc::ptr_eq(&default_service, &minimal_service));
        assert!(!Arc::ptr_eq(&minimal_service, &dev_service));
    }
}