//! Centralized Tool Manager for Crucible CLI
//!
//! This module provides a unified interface for tool discovery and execution,
//! eliminating duplicate initialization and registry management across CLI commands
//! and REPL components.

use anyhow::{Result, Context};
use serde_json::Value;
use std::sync::{Arc, Mutex, Once};
use std::collections::HashMap;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{info, warn, debug, error};

/// Centralized tool manager that handles all crucible-tools interactions
///
/// This singleton ensures tools are initialized only once and provides
/// both direct execution and REPL-compatible interfaces.
pub struct CrucibleToolManager {
    /// Whether tools have been initialized
    initialized: Arc<Mutex<bool>>,
    /// Cache for tool lists to avoid repeated discovery
    tool_list_cache: Arc<AsyncRwLock<Option<Vec<String>>>>,
    /// Cache for tool execution results
    execution_cache: Arc<AsyncRwLock<HashMap<String, CachedResult>>>,
    /// Configuration for caching behavior
    cache_config: ToolManagerConfig,
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
    result: crucible_tools::ToolResult,
    /// Timestamp when cached
    timestamp: std::time::Instant,
}

impl CachedResult {
    /// Check if cache entry is still valid
    fn is_valid(&self, ttl_secs: u64) -> bool {
        self.timestamp.elapsed().as_secs() < ttl_secs
    }
}

impl CrucibleToolManager {
    /// Get the global tool manager instance
    pub fn instance() -> &'static Self {
        static mut INSTANCE: Option<CrucibleToolManager> = None;
        static INIT: Once = Once::new();

        unsafe {
            INIT.call_once(|| {
                INSTANCE = Some(CrucibleToolManager::new());
            });
            INSTANCE.as_ref().unwrap()
        }
    }

    /// Create a new tool manager
    fn new() -> Self {
        info!("Creating centralized CrucibleToolManager");
        Self {
            initialized: Arc::new(Mutex::new(false)),
            tool_list_cache: Arc::new(AsyncRwLock::new(None)),
            execution_cache: Arc::new(AsyncRwLock::new(HashMap::new())),
            cache_config: ToolManagerConfig::default(),
        }
    }

    /// Create a tool manager with custom configuration
    pub fn with_config(config: ToolManagerConfig) -> Self {
        info!("Creating CrucibleToolManager with custom config");
        Self {
            initialized: Arc::new(Mutex::new(false)),
            tool_list_cache: Arc::new(AsyncRwLock::new(None)),
            execution_cache: Arc::new(AsyncRwLock::new(HashMap::new())),
            cache_config: config,
        }
    }

    /// Ensure tools are initialized (lazy initialization)
    pub async fn ensure_initialized(&self) -> Result<()> {
        let mut init_flag = self.initialized.lock().unwrap();
        if !*init_flag {
            info!("Initializing crucible-tools through centralized manager");

            // Initialize crucible-tools library
            crucible_tools::init();

            // Load all tools
            crucible_tools::load_all_tools()
                .await
                .context("Failed to load crucible-tools")?;

            *init_flag = true;
            info!("Successfully initialized crucible-tools through centralized manager");
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
    ) -> Result<crucible_tools::ToolResult> {
        // Ensure tools are initialized
        self.ensure_initialized().await?;

        // Check cache first if enabled
        if self.cache_config.enable_result_cache {
            let cache_key = self.create_cache_key(tool_name, &parameters, &user_id, &session_id);
            {
                let cache = self.execution_cache.read().await;
                if let Some(cached) = cache.get(&cache_key) {
                    if cached.is_valid(self.cache_config.cache_ttl_secs) {
                        debug!("Cache hit for tool: {}", tool_name);
                        return Ok(cached.result.clone());
                    }
                }
            }
        }

        // Execute the tool
        debug!("Executing tool: {} with params: {:?}", tool_name, parameters);
        let result = crucible_tools::execute_tool(
            tool_name.to_string(),
            parameters,
            user_id.clone(),
            session_id.clone(),
        ).await.map_err(|e| {
            error!("Tool execution failed for '{}': {}", tool_name, e);
            anyhow::anyhow!("Tool execution failed: {}", e)
        })?;

        // Cache result if enabled
        if self.cache_config.enable_result_cache {
            let cache_key = self.create_cache_key(tool_name, &result.data.clone().unwrap_or(Value::Null), &user_id, &session_id);
            let mut cache = self.execution_cache.write().await;

            // Enforce cache size limit
            if cache.len() >= self.cache_config.max_cache_size {
                // Remove oldest entries (simple FIFO)
                let mut keys: Vec<String> = cache.keys().cloned().collect();
                keys.sort(); // Simple sort for removal
                let excess = cache.len() - self.cache_config.max_cache_size + 1;
                for key in keys.into_iter().take(excess) {
                    cache.remove(&key);
                }
            }

            cache.insert(cache_key, CachedResult {
                result: result.clone(),
                timestamp: std::time::Instant::now(),
            });
        }

        Ok(result)
    }

    /// Get list of available tools with caching
    pub async fn list_tools(&self) -> Result<Vec<String>> {
        // Ensure tools are initialized
        self.ensure_initialized().await?;

        // Check cache first if enabled
        if self.cache_config.enable_list_cache {
            {
                let cache = self.tool_list_cache.read().await;
                if let Some(cached_tools) = &*cache {
                    debug!("Tool list cache hit: {} tools", cached_tools.len());
                    return Ok(cached_tools.clone());
                }
            }
        }

        // Get tools from crucible-tools
        let tools = crucible_tools::list_registered_tools().await;
        debug!("Discovered {} tools from crucible-tools", tools.len());

        // Cache result if enabled
        if self.cache_config.enable_list_cache {
            *self.tool_list_cache.write().await = Some(tools.clone());
        }

        Ok(tools)
    }

    /// Get tools grouped by category
    pub async fn list_tools_by_group(&self) -> Result<HashMap<String, Vec<String>>> {
        let tools = self.list_tools().await?;
        let mut grouped = HashMap::new();

        // Group tools by prefix or known categories
        for tool in tools {
            let category = if tool.starts_with("system_") {
                "system".to_string()
            } else if tool.starts_with("search_") || tool.starts_with("semantic_") || tool.starts_with("rebuild_") || tool.starts_with("get_index") || tool.starts_with("optimize_") || tool.starts_with("advanced_") {
                "search".to_string()
            } else if tool.starts_with("get_") || tool.starts_with("create_") || tool.starts_with("update_") || tool.starts_with("delete_") || tool.starts_with("list_") {
                "vault".to_string()
            } else if tool.contains("document") || tool.contains("content") || tool.contains("filename") || tool.contains("sync") || tool.contains("properties") {
                "database".to_string()
            } else {
                "other".to_string()
            };

            grouped.entry(category).or_insert_with(Vec::new).push(tool);
        }

        Ok(grouped)
    }

    /// Clear all caches
    pub async fn clear_caches(&self) {
        info!("Clearing tool manager caches");
        *self.tool_list_cache.write().await = None;
        self.execution_cache.write().await.clear();
    }

    /// Get tool manager statistics
    pub async fn get_stats(&self) -> HashMap<String, String> {
        let mut stats = HashMap::new();

        stats.insert("initialized".to_string(), {
            let init_flag = self.initialized.lock().unwrap();
            init_flag.to_string()
        });

        stats.insert("cached_tool_lists".to_string(), {
            let cache = self.tool_list_cache.read().await;
            cache.is_some().to_string()
        });

        stats.insert("cached_results".to_string(), {
            let cache = self.execution_cache.read().await;
            cache.len().to_string()
        });

        stats.insert("list_cache_enabled".to_string(), self.cache_config.enable_list_cache.to_string());
        stats.insert("result_cache_enabled".to_string(), self.cache_config.enable_result_cache.to_string());
        stats.insert("max_cache_size".to_string(), self.cache_config.max_cache_size.to_string());
        stats.insert("cache_ttl_secs".to_string(), self.cache_config.cache_ttl_secs.to_string());

        if let Ok(tool_count) = self.list_tools().await {
            stats.insert("total_tools".to_string(), tool_count.len().to_string());
        }

        stats
    }

    /// Create cache key for tool execution
    fn create_cache_key(&self, tool_name: &str, parameters: &Value, user_id: &Option<String>, session_id: &Option<String>) -> String {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        parameters.to_string().hash(&mut hasher);
        user_id.hash(&mut hasher);
        session_id.hash(&mut hasher);

        format!("{}_{}", tool_name, hasher.finish())
    }

    /// Force re-initialization of tools
    pub async fn force_reinitialize(&self) -> Result<()> {
        info!("Force re-initializing crucible-tools");

        // Clear caches
        self.clear_caches().await;

        // Reset initialization flag
        {
            let mut init_flag = self.initialized.lock().unwrap();
            *init_flag = false;
        }

        // Re-initialize
        self.ensure_initialized().await
    }
}

/// Convenience functions for global access
impl CrucibleToolManager {
    /// Execute a tool using the global manager
    pub async fn execute_tool_global(
        tool_name: &str,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<crucible_tools::ToolResult> {
        Self::instance().execute_tool(tool_name, parameters, user_id, session_id).await
    }

    /// List tools using the global manager
    pub async fn list_tools_global() -> Result<Vec<String>> {
        Self::instance().list_tools().await
    }

    /// List tools by group using the global manager
    pub async fn list_tools_by_group_global() -> Result<HashMap<String, Vec<String>>> {
        Self::instance().list_tools_by_group().await
    }

    /// Ensure initialization using the global manager
    pub async fn ensure_initialized_global() -> Result<()> {
        Self::instance().ensure_initialized().await
    }

    /// Clear caches using the global manager
    pub async fn clear_caches_global() {
        Self::instance().clear_caches().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_singleton_behavior() {
        let manager1 = CrucibleToolManager::instance();
        let manager2 = CrucibleToolManager::instance();

        // Both should be the same instance
        assert!(std::ptr::eq(manager1, manager2));
    }

    #[tokio::test]
    async fn test_initialization() {
        let manager = CrucibleToolManager::with_config(ToolManagerConfig::default());

        // Should initialize successfully
        assert!(manager.ensure_initialized().await.is_ok());

        // Should not initialize again
        assert!(manager.ensure_initialized().await.is_ok());
    }

    #[tokio::test]
    async fn test_tool_listing() {
        let manager = CrucibleToolManager::instance();

        let tools = manager.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        let grouped = manager.list_tools_by_group().await.unwrap();
        assert!(!grouped.is_empty());
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let manager = CrucibleToolManager::instance();

        let result = manager.execute_tool(
            "system_info",
            json!({}),
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_caching() {
        let config = ToolManagerConfig {
            enable_list_cache: true,
            enable_result_cache: true,
            max_cache_size: 10,
            cache_ttl_secs: 1,
        };
        let manager = CrucibleToolManager::with_config(config);

        // First call should populate cache
        let tools1 = manager.list_tools().await.unwrap();

        // Second call should use cache
        let tools2 = manager.list_tools().await.unwrap();

        assert_eq!(tools1, tools2);

        // Check stats
        let stats = manager.get_stats().await;
        assert_eq!(stats.get("cached_tool_lists"), Some(&"true".to_string()));
    }
}