//! Tool Group Trait for Unified Tool System
//!
//! This module defines the ToolGroup trait that provides a unified interface
//! for different types of tools in the Crucible system:
//! - System tools (crucible-tools)
//! - Rune tools (scripted tools)
//! - MCP server tools (external servers)
//!
//! The trait enables the REPL to work with multiple tool sources while
//! maintaining a consistent interface for discovery and execution.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Once};
use std::time::{Duration, Instant};
use tokio::sync::RwLock as AsyncRwLock;
use super::types::ToolResult;

/// Result type for tool group operations
pub type ToolGroupResult<T> = Result<T, ToolGroupError>;

/// Errors that can occur during tool group operations
#[derive(Debug, thiserror::Error)]
pub enum ToolGroupError {
    #[error("Tool group initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Tool discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Parameter conversion failed: {0}")]
    ParameterConversionFailed(String),

    #[error("Result conversion failed: {0}")]
    ResultConversionFailed(String),

    #[error("Tool group error: {0}")]
    Other(String),
}

/// Schema information for a tool
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSchema {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for parameters
    pub input_schema: Value,
    /// Expected output type description
    pub output_schema: Option<Value>,
}

/// Cache entry for tool discovery results
#[derive(Debug, Clone)]
pub struct ToolCacheEntry {
    /// Cached tool list
    pub tools: Vec<String>,
    /// Cache timestamp
    pub timestamp: Instant,
    /// TTL for this cache entry
    pub ttl: Duration,
}

impl ToolCacheEntry {
    pub fn new(tools: Vec<String>, ttl: Duration) -> Self {
        Self {
            tools,
            timestamp: Instant::now(),
            ttl,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.timestamp.elapsed() < self.ttl
    }
}

/// Schema cache entry
#[derive(Debug, Clone)]
pub struct SchemaCacheEntry {
    /// Cached schema
    pub schema: Option<ToolSchema>,
    /// Cache timestamp
    pub timestamp: Instant,
    /// TTL for this cache entry
    pub ttl: Duration,
}

impl SchemaCacheEntry {
    pub fn new(schema: Option<ToolSchema>, ttl: Duration) -> Self {
        Self {
            schema,
            timestamp: Instant::now(),
            ttl,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.timestamp.elapsed() < self.ttl
    }
}

/// Performance metrics for tool groups
#[derive(Debug, Default, Clone)]
pub struct ToolGroupMetrics {
    /// Number of tool discoveries performed
    pub discoveries: u64,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
    /// Total time spent on tool discovery (in milliseconds)
    pub total_discovery_time_ms: u64,
    /// Total time spent on tool execution (in milliseconds)
    pub total_execution_time_ms: u64,
    /// Initialization time (in milliseconds)
    pub initialization_time_ms: Option<u64>,
    /// Current memory usage estimate (in bytes)
    pub memory_usage_bytes: usize,
}

impl ToolGroupMetrics {
    pub fn add_discovery_time(&mut self, duration: Duration) {
        self.discoveries += 1;
        self.total_discovery_time_ms += duration.as_millis() as u64;
    }

    pub fn add_execution_time(&mut self, duration: Duration) {
        self.total_execution_time_ms += duration.as_millis() as u64;
    }

    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
    }

    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
    }

    pub fn cache_hit_rate(&self) -> f64 {
        if self.cache_hits + self.cache_misses == 0 {
            0.0
        } else {
            self.cache_hits as f64 / (self.cache_hits + self.cache_misses) as f64
        }
    }

    pub fn average_discovery_time_ms(&self) -> f64 {
        if self.discoveries == 0 {
            0.0
        } else {
            self.total_discovery_time_ms as f64 / self.discoveries as f64
        }
    }
}

/// Trait for tool groups that can be discovered and executed through the REPL
///
/// This trait provides a unified interface for different types of tools:
/// - System tools (Rust functions from crucible-tools)
/// - Rune tools (scripted .rn files)
/// - MCP server tools (external tool servers)
#[async_trait]
pub trait ToolGroup: std::fmt::Debug + Send + Sync {
    /// Get the name of this tool group
    fn group_name(&self) -> &str;

    /// Get a description of this tool group
    fn group_description(&self) -> &str;

    /// Discover all available tools in this group (with caching)
    async fn discover_tools(&mut self) -> ToolGroupResult<Vec<String>>;

    /// List all currently available tools in this group (may trigger lazy loading)
    async fn list_tools(&mut self) -> ToolGroupResult<Vec<String>>;

    /// Get schema information for a specific tool (with caching)
    async fn get_tool_schema(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>>;

    /// Execute a tool from this group (with metrics)
    async fn execute_tool(
        &self,
        tool_name: &str,
        args: &[String],
    ) -> ToolGroupResult<ToolResult>;

    /// Check if this group has been initialized
    fn is_initialized(&self) -> bool;

    /// Initialize the tool group (if needed) - should be lazy
    async fn initialize(&mut self) -> ToolGroupResult<()>;

    /// Force refresh of all cached data
    async fn refresh_cache(&mut self) -> ToolGroupResult<()>;

    /// Get performance metrics for this tool group
    fn get_metrics(&self) -> ToolGroupMetrics;

    /// Get cache configuration
    fn get_cache_config(&self) -> &ToolGroupCacheConfig;

    /// Get metadata about this tool group
    fn get_metadata(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Internal method: Perform actual tool discovery (override in implementations)
    async fn perform_discovery(&self) -> ToolGroupResult<Vec<String>> {
        Ok(Vec::new())
    }

    /// Internal method: Perform actual schema retrieval (override in implementations)
    async fn perform_schema_retrieval(&self, _tool_name: &str) -> ToolGroupResult<Option<ToolSchema>> {
        Ok(None)
    }
}

/// Configuration for tool group caching
#[derive(Debug, Clone)]
pub struct ToolGroupCacheConfig {
    /// TTL for tool discovery cache
    pub discovery_ttl: Duration,
    /// TTL for tool schema cache
    pub schema_ttl: Duration,
    /// Maximum number of cached schemas
    pub max_schema_cache_size: usize,
    /// Whether caching is enabled
    pub caching_enabled: bool,
}

impl Default for ToolGroupCacheConfig {
    fn default() -> Self {
        Self {
            discovery_ttl: Duration::from_secs(300), // 5 minutes
            schema_ttl: Duration::from_secs(600),     // 10 minutes
            max_schema_cache_size: 1000,
            caching_enabled: true,
        }
    }
}

impl ToolGroupCacheConfig {
    pub fn no_caching() -> Self {
        Self {
            discovery_ttl: Duration::ZERO,
            schema_ttl: Duration::ZERO,
            max_schema_cache_size: 0,
            caching_enabled: false,
        }
    }

    pub fn fast_cache() -> Self {
        Self {
            discovery_ttl: Duration::from_secs(60),   // 1 minute
            schema_ttl: Duration::from_secs(120),     // 2 minutes
            max_schema_cache_size: 500,
            caching_enabled: true,
        }
    }
}

/// Helper trait for tool parameter conversion
pub trait ParameterConverter {
    /// Convert string arguments to the expected parameter format
    fn convert_args_to_params(&self, tool_name: &str, args: &[String]) -> ToolGroupResult<Value>;

    /// Validate that the provided parameters match the tool's schema
    fn validate_params(&self, tool_name: &str, params: &Value) -> ToolGroupResult<()>;
}

/// Helper trait for tool result conversion
pub trait ResultConverter {
    /// Convert tool execution result to the standard ToolResult format
    fn convert_to_tool_result(&self, tool_name: &str, raw_result: Value) -> ToolGroupResult<ToolResult>;
}

/// Registry for managing multiple tool groups with lazy loading and caching
#[derive(Debug)]
pub struct ToolGroupRegistry {
    groups: AsyncRwLock<HashMap<String, Box<dyn ToolGroup>>>,
    tool_to_group: AsyncRwLock<HashMap<String, String>>, // Maps tool name to group name
    registry_metrics: Arc<RwLock<RegistryMetrics>>,
}

/// Performance metrics for the entire registry
#[derive(Debug, Default, Clone)]
pub struct RegistryMetrics {
    /// Total groups registered
    pub total_groups: u64,
    /// Total tools registered
    pub total_tools: u64,
    /// Registry initialization time
    pub initialization_time_ms: Option<u64>,
    /// Number of lazy initializations performed
    pub lazy_initializations: u64,
    /// Total cache statistics across all groups
    pub aggregate_cache_hits: u64,
    pub aggregate_cache_misses: u64,
}

impl ToolGroupRegistry {
    /// Create a new tool group registry
    pub fn new() -> Self {
        Self {
            groups: AsyncRwLock::new(HashMap::new()),
            tool_to_group: AsyncRwLock::new(HashMap::new()),
            registry_metrics: Arc::new(RwLock::new(RegistryMetrics::default())),
        }
    }

    /// Register a tool group (lazy - doesn't initialize until needed)
    pub async fn register_group(&self, group: Box<dyn ToolGroup>) -> ToolGroupResult<()> {
        let start_time = Instant::now();
        let group_name = group.group_name().to_string();

        // Register the group without initializing
        {
            let mut groups = self.groups.write().await;
            groups.insert(group_name.clone(), group);
        }

        // Update metrics
        {
            let mut metrics = self.registry_metrics.write().unwrap();
            metrics.total_groups += 1;
        }

        tracing::info!("Registered tool group '{}' (lazy initialization)", group_name);

        let duration = start_time.elapsed();
        tracing::debug!("Group registration took {}ms", duration.as_millis());

        Ok(())
    }

    /// Initialize a group on-demand
    async fn ensure_group_initialized(&self, group_name: &str) -> ToolGroupResult<()> {
        let groups = self.groups.read().await;
        if let Some(group) = groups.get(group_name) {
            if !group.is_initialized() {
                drop(groups); // Release read lock

                // Get write lock and initialize
                let mut groups = self.groups.write().await;
                if let Some(group) = groups.get_mut(group_name) {
                    if !group.is_initialized() {
                        let start_time = Instant::now();
                        group.initialize().await?;

                        // Discover tools and update mappings
                        let tools = group.discover_tools().await?;
                        {
                            let mut tool_to_group = self.tool_to_group.write().await;
                            for tool_name in &tools {
                                tool_to_group.insert(tool_name.clone(), group_name.to_string());
                            }
                        }

                        // Update metrics
                        {
                            let mut metrics = self.registry_metrics.write().unwrap();
                            metrics.lazy_initializations += 1;
                            metrics.total_tools += tools.len() as u64;
                        }

                        let duration = start_time.elapsed();
                        tracing::info!("Lazy initialization of '{}' completed in {}ms ({} tools)",
                                     group_name, duration.as_millis(), tools.len());
                    }
                }
            }
        }
        Ok(())
    }

    /// List all tools from all groups (lazy initialization)
    pub async fn list_all_tools(&self) -> Vec<String> {
        // First, ensure all registered groups are initialized to discover their tools
        let group_names: Vec<String> = {
            let groups = self.groups.read().await;
            groups.keys().cloned().collect()
        };

        // Initialize each group if not already initialized
        for group_name in &group_names {
            let _ = self.ensure_group_initialized(group_name).await;
        }

        // Now list all discovered tools
        let tool_to_group = self.tool_to_group.read().await;
        tool_to_group.keys().cloned().collect()
    }

    /// List tools grouped by source (lazy initialization)
    pub async fn list_tools_by_group(&self) -> HashMap<String, Vec<String>> {
        // First, ensure all registered groups are initialized to discover their tools
        let group_names: Vec<String> = {
            let groups = self.groups.read().await;
            groups.keys().cloned().collect()
        };

        // Initialize each group if not already initialized
        for group_name in &group_names {
            let _ = self.ensure_group_initialized(group_name).await;
        }

        // Now group the discovered tools
        let tool_to_group = self.tool_to_group.read().await;
        let mut grouped = HashMap::new();

        for (tool_name, group_name) in &*tool_to_group {
            grouped
                .entry(group_name.clone())
                .or_insert_with(Vec::new)
                .push(tool_name.clone());
        }

        // Sort tools within each group
        for tools in grouped.values_mut() {
            tools.sort();
        }

        grouped
    }

    /// Get the group that owns a specific tool
    pub async fn get_tool_group(&self, tool_name: &str) -> Option<String> {
        let tool_to_group = self.tool_to_group.read().await;
        tool_to_group.get(tool_name).cloned()
    }

    /// Get all registered groups
    pub async fn list_groups(&self) -> Vec<String> {
        let groups = self.groups.read().await;
        groups.keys().cloned().collect()
    }

    /// Execute a tool by finding the appropriate group (lazy initialization)
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        args: &[String],
    ) -> ToolGroupResult<ToolResult> {
        // Ensure tool mapping is loaded (may trigger lazy initialization)
        let group_name = self.get_tool_group(tool_name).await
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Tool '{}' not found in any group", tool_name
            )))?;

        // Ensure the group is initialized
        self.ensure_group_initialized(&group_name).await?;

        // Execute the tool
        let groups = self.groups.read().await;
        let group = groups.get(&group_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Group '{}' not found for tool '{}'", group_name, tool_name
            )))?;

        group.execute_tool(tool_name, args).await
    }

    /// Get schema for a tool (lazy initialization)
    pub async fn get_tool_schema(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>> {
        // Ensure tool mapping is loaded
        let group_name = self.get_tool_group(tool_name).await
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Tool '{}' not found in any group", tool_name
            )))?;

        // Ensure the group is initialized
        self.ensure_group_initialized(&group_name).await?;

        // Get the schema
        let groups = self.groups.read().await;
        let group = groups.get(&group_name)
            .ok_or_else(|| ToolGroupError::ToolNotFound(format!(
                "Group '{}' not found for tool '{}'", group_name, tool_name
            )))?;

        group.get_tool_schema(tool_name).await
    }

    /// Force refresh all tool groups
    pub async fn refresh_all(&self) -> ToolGroupResult<()> {
        let groups = self.groups.read().await;
        let group_names: Vec<String> = groups.keys().cloned().collect();
        drop(groups);

        for group_name in group_names {
            let mut groups = self.groups.write().await;
            if let Some(group) = groups.get_mut(&group_name) {
                group.refresh_cache().await?;
            }
        }

        tracing::info!("All tool groups refreshed");
        Ok(())
    }

    /// Get registry performance metrics
    pub fn get_metrics(&self) -> RegistryMetrics {
        self.registry_metrics.read().unwrap().clone()
    }

    /// Get detailed metrics including all group metrics
    pub async fn get_detailed_metrics(&self) -> HashMap<String, ToolGroupMetrics> {
        let groups = self.groups.read().await;
        let mut detailed = HashMap::new();

        for (name, group) in groups.iter() {
            detailed.insert(name.clone(), group.get_metrics());
        }

        detailed
    }
}

impl Default for ToolGroupRegistry {
    fn default() -> Self {
        Self::new()
    }
}