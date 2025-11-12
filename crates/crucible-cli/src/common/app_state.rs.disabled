//! Application State Pattern for CLI without global singletons
//!
//! This demonstrates how to organize application state in a clean, testable way
//! without relying on global mutable state.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use crucible_tools::types_refactored::{ToolManager, ToolManagerConfig};

/// Centralized application state that replaces global singletons
#[derive(Debug, Clone)]
pub struct AppState {
    /// Tool manager for all tool operations
    pub tool_manager: Arc<ToolManager>,
    /// Application configuration
    pub config: Arc<AppConfig>,
    /// Runtime state and caches
    pub runtime: Arc<RuntimeState>,
}

/// Application configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Kiln directory path
    pub kiln_path: PathBuf,
    /// User preferences
    pub preferences: HashMap<String, String>,
    /// Logging configuration
    pub log_level: String,
    /// Cache configuration
    pub cache_config: CacheConfig,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enable tool result caching
    pub enable_tool_cache: bool,
    /// Enable file listing cache
    pub enable_file_cache: bool,
    /// Cache directory
    pub cache_dir: Option<PathBuf>,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
}

/// Runtime state that changes during execution
#[derive(Debug, Default)]
pub struct RuntimeState {
    /// Current user ID
    pub user_id: Option<String>,
    /// Current session ID
    pub session_id: Option<String>,
    /// Working directory for current operation
    pub working_dir: Option<PathBuf>,
    /// Environment variables for current session
    pub environment: HashMap<String, String>,
}

/// Builder pattern for creating AppState
#[derive(Debug)]
pub struct AppStateBuilder {
    config: AppConfig,
    tool_manager_config: ToolManagerConfig,
    runtime: RuntimeState,
}

impl AppStateBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: AppConfig::default(),
            tool_manager_config: ToolManagerConfig::default(),
            runtime: RuntimeState::default(),
        }
    }

    /// Set the kiln path
    pub fn with_kiln_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.config.kiln_path = path.into();
        self
    }

    /// Set the log level
    pub fn with_log_level<S: Into<String>>(mut self, level: S) -> Self {
        self.config.log_level = level.into();
        self
    }

    /// Set tool manager configuration
    pub fn with_tool_config(mut self, config: ToolManagerConfig) -> Self {
        self.tool_manager_config = config;
        self
    }

    /// Set cache configuration
    pub fn with_cache_config(mut self, config: CacheConfig) -> Self {
        self.config.cache_config = config;
        self
    }

    /// Set user preferences
    pub fn with_preferences(mut self, prefs: HashMap<String, String>) -> Self {
        self.config.preferences = prefs;
        self
    }

    /// Set initial user ID
    pub fn with_user_id<S: Into<String>>(mut self, user_id: S) -> Self {
        self.runtime.user_id = Some(user_id.into());
        self
    }

    /// Set initial session ID
    pub fn with_session_id<S: Into<String>>(mut self, session_id: S) -> Self {
        self.runtime.session_id = Some(session_id.into());
        self
    }

    /// Set working directory
    pub fn with_working_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.runtime.working_dir = Some(dir.into());
        self
    }

    /// Build the AppState
    pub async fn build(self) -> Result<Arc<AppState>> {
        let tool_manager = Arc::new(ToolManager::with_config(self.tool_manager_config));

        // Set up tool manager with configuration
        let registry = tool_manager.registry();
        let tool_config = crucible_tools::types::ToolConfigContext::with_kiln_path(
            self.config.kiln_path.clone()
        );
        registry.set_config_context(tool_config).await;

        // Initialize the tool manager
        tool_manager.ensure_initialized().await?;

        Ok(Arc::new(AppState {
            tool_manager,
            config: Arc::new(self.config),
            runtime: Arc::new(self.runtime),
        }))
    }
}

impl Default for AppStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            kiln_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            preferences: HashMap::new(),
            log_level: "info".to_string(),
            cache_config: CacheConfig::default(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enable_tool_cache: true,
            enable_file_cache: true,
            cache_dir: None,
            cache_ttl_secs: 300,
        }
    }
}

impl AppState {
    /// Create a builder for AppState
    pub fn builder() -> AppStateBuilder {
        AppStateBuilder::new()
    }

    /// Execute a tool with current context
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
    ) -> Result<crucible_tools::ToolResult> {
        self.tool_manager
            .execute_tool(
                tool_name,
                parameters,
                self.runtime.user_id.clone(),
                self.runtime.session_id.clone(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Tool execution failed: {}", e))
    }

    /// Get list of available tools
    pub async fn list_tools(&self) -> Result<Vec<String>> {
        self.tool_manager
            .list_tools()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tools: {}", e))
    }

    /// Set current user context
    pub async fn set_user_context(&self, user_id: Option<String>, session_id: Option<String>) {
        // Note: In a real implementation, you might want Arc<RwLock<RuntimeState>>
        // for thread-safe mutation. This is a simplified example.
        // For demonstration, we'll show the pattern:
        // let mut runtime = self.runtime.write().await;
        // runtime.user_id = user_id;
        // runtime.session_id = session_id;
    }

    /// Get current kiln path
    pub fn kiln_path(&self) -> &PathBuf {
        &self.config.kiln_path
    }

    /// Get cache configuration
    pub fn cache_config(&self) -> &CacheConfig {
        &self.config.cache_config
    }

    /// Clear all caches
    pub async fn clear_caches(&self) {
        self.tool_manager.clear_caches().await;
    }
}

/// Trait for operations that need access to application state
#[async_trait::async_trait]
pub trait AppStateExt {
    /// Execute a tool with automatic error handling
    async fn execute_tool_safe(
        &self,
        tool_name: &str,
        parameters: Value,
    ) -> Result<crucible_tools::ToolResult>;

    /// Execute a tool and return JSON output
    async fn execute_tool_json(
        &self,
        tool_name: &str,
        parameters: Value,
    ) -> Result<serde_json::Value> {
        let result = self.execute_tool_safe(tool_name, parameters).await?;
        if result.success {
            Ok(result.data.unwrap_or(serde_json::Value::Null))
        } else {
            Err(anyhow::anyhow!("Tool failed: {}", result.error.unwrap_or_default()))
        }
    }
}

#[async_trait::async_trait]
impl AppStateExt for AppState {
    async fn execute_tool_safe(
        &self,
        tool_name: &str,
        parameters: Value,
    ) -> Result<crucible_tools::ToolResult> {
        match self.execute_tool(tool_name, parameters).await {
            Ok(result) => Ok(result),
            Err(e) => {
                tracing::error!("Tool '{}' execution failed: {}", tool_name, e);
                Err(e)
            }
        }
    }
}

/// Convenience functions for creating app state from different sources
impl AppState {
    /// Create app state from CLI config
    pub async fn from_cli_config(cli_config: crate::config::CliConfig) -> Result<Arc<Self>> {
        AppState::builder()
            .with_kiln_path(cli_config.kiln.path)
            .with_log_level(cli_config.log_level)
            .with_user_id("cli_user".to_string())
            .with_session_id(format!("cli_session_{}", uuid::Uuid::new_v4()))
            .build()
            .await
    }

    /// Create app state for testing
    pub async fn for_test() -> Result<Arc<Self>> {
        let temp_dir = std::env::temp_dir();
        AppState::builder()
            .with_kiln_path(temp_dir.join("test_kiln"))
            .with_log_level("debug".to_string())
            .with_tool_config(ToolManagerConfig {
                enable_list_cache: false,
                enable_result_cache: false,
                max_cache_size: 10,
                cache_ttl_secs: 1,
            })
            .with_user_id("test_user".to_string())
            .with_session_id("test_session".to_string())
            .build()
            .await
    }

    /// Create app state with minimal setup for embedded usage
    pub async fn minimal<P: Into<PathBuf>>(kiln_path: P) -> Result<Arc<Self>> {
        AppState::builder()
            .with_kiln_path(kiln_path)
            .with_tool_config(ToolManagerConfig {
                enable_list_cache: false,
                enable_result_cache: false,
                max_cache_size: 0,
                cache_ttl_secs: 0,
            })
            .build()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_app_state_builder() {
        let temp_dir = std::env::temp_dir();
        let app_state = AppState::builder()
            .with_kiln_path(temp_dir.clone())
            .with_log_level("debug".to_string())
            .with_user_id("test_user".to_string())
            .build()
            .await
            .unwrap();

        assert_eq!(app_state.kiln_path(), &temp_dir);
        assert_eq!(app_state.config.log_level, "debug");
        assert_eq!(app_state.runtime.user_id, Some("test_user".to_string()));
    }

    #[tokio::test]
    async fn test_app_state_for_test() {
        let app_state = AppState::for_test().await.unwrap();

        // Should have tools loaded
        let tools = app_state.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        // Should be able to execute a tool
        let result = app_state
            .execute_tool_safe("system_info", json!({}))
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_multiple_independent_states() {
        let state1 = AppState::for_test().await.unwrap();
        let state2 = AppState::for_test().await.unwrap();

        // Should be independent
        let state1_ptr = state1.tool_manager.as_ref() as *const _;
        let state2_ptr = state2.tool_manager.as_ref() as *const _;
        assert_ne!(state1_ptr, state2_ptr);

        // Both should work independently
        let tools1 = state1.list_tools().await.unwrap();
        let tools2 = state2.list_tools().await.unwrap();
        assert_eq!(tools1.len(), tools2.len());
        assert!(!tools1.is_empty());
    }

    #[tokio::test]
    async fn test_app_state_ext() {
        let app_state = AppState::for_test().await.unwrap();

        // Test safe execution
        let result = app_state
            .execute_tool_safe("system_info", json!({}))
            .await
            .unwrap();
        assert!(result.success);

        // Test JSON execution
        let json_result = app_state
            .execute_tool_json("system_info", json!({}))
            .await
            .unwrap();
        assert!(json_result.is_object());
    }
}