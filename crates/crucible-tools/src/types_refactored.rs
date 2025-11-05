//! Refactored tool types without global state - dependency injection approach
//!
//! This demonstrates how to eliminate global state by passing shared instances
//! explicitly as parameters.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// Re-export existing types for compatibility
pub use crate::types::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolExecutionRequest,
    ToolResult, ToolFunction, ToolFunctionRegistry, ToolConfigContext
};

/// Thread-safe tool registry without global state
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    /// Inner registry with Arc<RwLock<>> for async-safe sharing
    registry: Arc<RwLock<ToolFunctionRegistry>>,
    /// Configuration context for all tools
    config_context: Arc<RwLock<Option<ToolConfigContext>>>,
}

impl ToolRegistry {
    /// Create a new, empty tool registry
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            config_context: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a registry with existing tools
    pub fn with_tools(tools: ToolFunctionRegistry) -> Self {
        Self {
            registry: Arc::new(RwLock::new(tools)),
            config_context: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a registry with both tools and configuration
    pub fn with_tools_and_config(
        tools: ToolFunctionRegistry,
        config: ToolConfigContext,
    ) -> Self {
        Self {
            registry: Arc::new(RwLock::new(tools)),
            config_context: Arc::new(RwLock::new(Some(config))),
        }
    }

    /// Register a tool function
    pub async fn register_tool(&self, name: String, function: ToolFunction) -> Result<(), ToolError> {
        let mut registry = self.registry.write().await;
        registry.insert(name, function);
        Ok(())
    }

    /// Execute a tool by name
    pub async fn execute_tool(
        &self,
        tool_name: String,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<ToolResult, ToolError> {
        let start_time = std::time::Instant::now();

        // Find the tool function
        let tool_fn = {
            let registry = self.registry.read().await;
            registry
                .get(&tool_name)
                .ok_or_else(|| ToolError::ToolNotFound(tool_name.clone()))?
                .clone()
        };

        // Execute the tool
        let result = tool_fn(tool_name.clone(), parameters, user_id, session_id).await?;

        // Add timing if not already present
        let final_result = if result.duration_ms == 0 {
            ToolResult::success_with_duration(
                result.tool_name,
                result.data.unwrap_or(serde_json::Value::Null),
                start_time.elapsed().as_millis() as u64,
            )
        } else {
            result
        };

        Ok(final_result)
    }

    /// Get list of all registered tool names
    pub async fn list_tools(&self) -> Vec<String> {
        let registry = self.registry.read().await;
        registry.keys().cloned().collect()
    }

    /// Get tool count
    pub async fn tool_count(&self) -> usize {
        let registry = self.registry.read().await;
        registry.len()
    }

    /// Check if a tool is registered
    pub async fn has_tool(&self, name: &str) -> bool {
        let registry = self.registry.read().await;
        registry.contains_key(name)
    }

    /// Set the global configuration context for this registry
    pub async fn set_config_context(&self, context: ToolConfigContext) {
        let mut ctx = self.config_context.write().await;
        *ctx = Some(context);
    }

    /// Get the configuration context
    pub async fn get_config_context(&self) -> Option<ToolConfigContext> {
        let ctx = self.config_context.read().await;
        ctx.clone()
    }

    /// Get the kiln path from the configuration context
    pub async fn get_kiln_path(&self) -> Result<PathBuf, ToolError> {
        let context = self.get_config_context().await
            .ok_or_else(|| ToolError::Other("Tool configuration context not set".to_string()))?;

        context
            .kiln_path
            .ok_or_else(|| ToolError::Other("No kiln path configured".to_string()))
    }

    /// Load all tools into this registry instance
    pub async fn load_all_tools(&self) -> Result<(), ToolError> {
        tracing::info!("Loading all tools into registry instance");

        // Register system tools
        self.register_system_tools().await?;

        // Register kiln tools
        self.register_kiln_tools().await?;

        // Register database tools
        self.register_database_tools().await?;

        // Register search tools
        self.register_search_tools().await?;

        let tool_count = self.tool_count().await;
        tracing::info!("Successfully loaded {} tools", tool_count);

        Ok(())
    }

    /// Register all system tools
    async fn register_system_tools(&self) -> Result<(), ToolError> {
        use crate::system_tools;

        let tools = vec![
            ("system_info", system_tools::get_system_info()),
            ("execute_command", system_tools::execute_command()),
            ("list_files", system_tools::list_files()),
            ("read_file", system_tools::read_file()),
            ("get_environment", system_tools::get_environment()),
        ];

        for (name, function) in tools {
            self.register_tool(name.to_string(), function).await?;
            tracing::debug!("Registered system tool: {}", name);
        }

        Ok(())
    }

    /// Register all kiln tools
    async fn register_kiln_tools(&self) -> Result<(), ToolError> {
        use crate::kiln_tools;

        let tools = vec![
            ("search_by_properties", kiln_tools::search_by_properties()),
            ("search_by_tags", kiln_tools::search_by_tags()),
            ("search_by_folder", kiln_tools::search_by_folder()),
            ("create_note", kiln_tools::create_note()),
            ("update_note", kiln_tools::update_note()),
            ("delete_note", kiln_tools::delete_note()),
            ("get_kiln_stats", kiln_tools::get_kiln_stats()),
            ("list_tags", kiln_tools::list_tags()),
        ];

        for (name, function) in tools {
            self.register_tool(name.to_string(), function).await?;
            tracing::debug!("Registered kiln tool: {}", name);
        }

        Ok(())
    }

    /// Register all database tools
    async fn register_database_tools(&self) -> Result<(), ToolError> {
        use crate::database_tools;

        let tools = vec![
            ("semantic_search", database_tools::semantic_search()),
            ("search_by_content", database_tools::search_by_content()),
            ("search_by_filename", database_tools::search_by_filename()),
            (
                "update_note_properties",
                database_tools::update_note_properties(),
            ),
            ("index_document", database_tools::index_document()),
            ("get_document_stats", database_tools::get_document_stats()),
            ("sync_metadata", database_tools::sync_metadata()),
        ];

        for (name, function) in tools {
            self.register_tool(name.to_string(), function).await?;
            tracing::debug!("Registered database tool: {}", name);
        }

        Ok(())
    }

    /// Register all search tools
    async fn register_search_tools(&self) -> Result<(), ToolError> {
        use crate::search_tools;

        let tools = vec![
            ("search_documents", search_tools::search_documents()),
            ("rebuild_index", search_tools::rebuild_index()),
            ("get_index_stats", search_tools::get_index_stats()),
            ("optimize_index", search_tools::optimize_index()),
            ("advanced_search", search_tools::advanced_search()),
        ];

        for (name, function) in tools {
            self.register_tool(name.to_string(), function).await?;
            tracing::debug!("Registered search tool: {}", name);
        }

        Ok(())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool manager without global state - using explicit Arc<ToolRegistry>
#[derive(Debug, Clone)]
pub struct ToolManager {
    /// Shared tool registry
    registry: Arc<ToolRegistry>,
    /// Configuration for caching behavior
    config: ToolManagerConfig,
    /// Cache for tool lists to avoid repeated discovery
    tool_list_cache: Arc<tokio::sync::RwLock<Option<Vec<String>>>>,
    /// Cache for tool execution results
    execution_cache: Arc<tokio::sync::RwLock<HashMap<String, CachedResult>>>,
    /// Whether tools have been initialized
    initialized: Arc<std::sync::atomic::AtomicBool>,
}

/// Configuration for the tool manager
#[derive(Debug, Clone)]
pub struct ToolManagerConfig {
    /// Enable tool list caching
    pub enable_list_cache: bool,
    /// Enable result caching
    pub enable_result_cache: bool,
    /// Maximum cache size
    pub max_cache_size: usize,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
}

impl Default for ToolManagerConfig {
    fn default() -> Self {
        Self {
            enable_list_cache: true,
            enable_result_cache: true,
            max_cache_size: 100,
            cache_ttl_secs: 300, // 5 minutes
        }
    }
}

/// Cached execution result
#[derive(Debug, Clone)]
struct CachedResult {
    /// Result value
    result: ToolResult,
    /// Timestamp when cached
    timestamp: std::time::Instant,
}

impl CachedResult {
    /// Check if cache entry is still valid
    fn is_valid(&self, ttl_secs: u64) -> bool {
        self.timestamp.elapsed().as_secs() < ttl_secs
    }
}

impl ToolManager {
    /// Create a new tool manager with default configuration
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ToolRegistry::new()),
            config: ToolManagerConfig::default(),
            tool_list_cache: Arc::new(tokio::sync::RwLock::new(None)),
            execution_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            initialized: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a tool manager with custom configuration
    pub fn with_config(config: ToolManagerConfig) -> Self {
        Self {
            registry: Arc::new(ToolRegistry::new()),
            config,
            tool_list_cache: Arc::new(tokio::sync::RwLock::new(None)),
            execution_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            initialized: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a tool manager with existing registry
    pub fn with_registry(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            config: ToolManagerConfig::default(),
            tool_list_cache: Arc::new(tokio::sync::RwLock::new(None)),
            execution_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            initialized: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Ensure tools are initialized (lazy initialization)
    pub async fn ensure_initialized(&self) -> Result<(), ToolError> {
        if !self.initialized.load(std::sync::atomic::Ordering::Relaxed) {
            tracing::info!("=== Initializing Tool Manager ===");

            // Initialize crucible-tools library
            crate::init();

            // Load all tools
            self.registry.load_all_tools().await?;

            self.initialized.store(true, std::sync::atomic::Ordering::Relaxed);
            tracing::info!("=== Tool Manager Initialization Complete ===");
        }
        Ok(())
    }

    /// Execute a tool with caching
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<ToolResult, ToolError> {
        // Ensure tools are initialized
        self.ensure_initialized().await?;

        // Check cache first if enabled
        if self.config.enable_result_cache {
            let cache_key = self.create_cache_key(tool_name, &parameters, &user_id, &session_id);
            {
                let cache = self.execution_cache.read().await;
                if let Some(cached) = cache.get(&cache_key) {
                    if cached.is_valid(self.config.cache_ttl_secs) {
                        tracing::debug!("Cache hit for tool: {}", tool_name);
                        return Ok(cached.result.clone());
                    }
                }
            }
        }

        // Execute the tool
        tracing::debug!("Executing tool: {} with params: {:?}", tool_name, parameters);
        let result = self
            .registry
            .execute_tool(
                tool_name.to_string(),
                parameters,
                user_id.clone(),
                session_id.clone(),
            )
            .await?;

        // Cache result if enabled
        if self.config.enable_result_cache {
            let cache_key = self.create_cache_key(
                tool_name,
                &result.data.clone().unwrap_or(Value::Null),
                &user_id,
                &session_id,
            );
            let mut cache = self.execution_cache.write().await;

            // Enforce cache size limit
            if cache.len() >= self.config.max_cache_size {
                // Remove oldest entries (simple FIFO)
                let mut keys: Vec<String> = cache.keys().cloned().collect();
                keys.sort();
                let excess = cache.len() - self.config.max_cache_size + 1;
                for key in keys.into_iter().take(excess) {
                    cache.remove(&key);
                }
            }

            cache.insert(
                cache_key,
                CachedResult {
                    result: result.clone(),
                    timestamp: std::time::Instant::now(),
                },
            );
        }

        Ok(result)
    }

    /// Get list of available tools with caching
    pub async fn list_tools(&self) -> Result<Vec<String>, ToolError> {
        // Ensure tools are initialized
        self.ensure_initialized().await?;

        // Check cache first if enabled
        if self.config.enable_list_cache {
            {
                let cache = self.tool_list_cache.read().await;
                if let Some(cached_tools) = &*cache {
                    tracing::debug!("Tool list cache hit: {} tools", cached_tools.len());
                    return Ok(cached_tools.clone());
                }
            }
        }

        // Get tools from registry
        let tools = self.registry.list_tools().await;
        tracing::debug!("Discovered {} tools", tools.len());

        // Cache result if enabled
        if self.config.enable_list_cache {
            *self.tool_list_cache.write().await = Some(tools.clone());
        }

        Ok(tools)
    }

    /// Get the underlying registry for direct access
    pub fn registry(&self) -> Arc<ToolRegistry> {
        self.registry.clone()
    }

    /// Clear all caches
    pub async fn clear_caches(&self) {
        tracing::info!("Clearing tool manager caches");
        *self.tool_list_cache.write().await = None;
        self.execution_cache.write().await.clear();
    }

    /// Create cache key for tool execution
    fn create_cache_key(
        &self,
        tool_name: &str,
        parameters: &Value,
        user_id: &Option<String>,
        session_id: &Option<String>,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        parameters.to_string().hash(&mut hasher);
        user_id.hash(&mut hasher);
        session_id.hash(&mut hasher);

        format!("{}_{}", tool_name, hasher.finish())
    }
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_registry_no_globals() {
        let registry = ToolRegistry::new();

        // Registry should be empty initially
        assert_eq!(registry.tool_count().await, 0);
        assert!(!registry.has_tool("system_info").await);

        // Can register tools
        registry.register_tool("test_tool".to_string(), |_name, _params, _user, _session| {
            Box::pin(async {
                Ok(ToolResult::success("test_tool".to_string(), json!({"test": true})))
            })
        }).await.unwrap();

        assert_eq!(registry.tool_count().await, 1);
        assert!(registry.has_tool("test_tool").await);
    }

    #[tokio::test]
    async fn test_manager_no_globals() {
        let manager = ToolManager::new();

        // Should initialize successfully
        assert!(manager.ensure_initialized().await.is_ok());

        // Should not initialize again
        assert!(manager.ensure_initialized().await.is_ok());

        // Should have tools loaded
        let tools = manager.list_tools().await.unwrap();
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_managers_independent() {
        let manager1 = ToolManager::new();
        let manager2 = ToolManager::new();

        // Initialize both managers independently
        manager1.ensure_initialized().await.unwrap();
        manager2.ensure_initialized().await.unwrap();

        // Both should have tools
        let tools1 = manager1.list_tools().await.unwrap();
        let tools2 = manager2.list_tools().await.unwrap();

        assert_eq!(tools1.len(), tools2.len());
        assert!(!tools1.is_empty());

        // Should be independent instances
        let tools1_ptr = manager1.registry.as_ref() as *const _;
        let tools2_ptr = manager2.registry.as_ref() as *const _;
        assert_ne!(tools1_ptr, tools2_ptr);
    }
}