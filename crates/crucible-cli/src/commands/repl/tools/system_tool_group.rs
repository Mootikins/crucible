//! System Tool Group Implementation
//!
//! This module implements the SystemToolGroup that wraps crucible-tools
//! and provides them through the ToolGroup trait interface.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::RwLock as AsyncRwLock;
use super::tool_group::{
    ToolGroup, ToolGroupResult, ToolGroupError, ToolSchema, ParameterConverter, ResultConverter,
    ToolGroupMetrics, ToolGroupCacheConfig, ToolCacheEntry, SchemaCacheEntry
};
use super::types::ToolResult;

/// System Tool Group that wraps crucible-tools functionality
///
/// This tool group provides access to all crucible-tools (system tools) through
/// the unified ToolGroup interface. It handles parameter conversion from
/// string arguments to JSON and result conversion back to ToolResult format.
///
/// Features:
/// - Lazy initialization (only initializes when first needed)
/// - Caching for tool discovery and schemas
/// - Performance metrics tracking
/// - Memory-efficient tool loading
#[derive(Debug)]
pub struct SystemToolGroup {
    /// Whether crucible-tools has been initialized
    initialized: bool,
    /// Cache configuration
    cache_config: ToolGroupCacheConfig,
    /// Cached tool discovery result
    tools_cache: AsyncRwLock<Option<ToolCacheEntry>>,
    /// Cached tool schemas
    schema_cache: AsyncRwLock<HashMap<String, SchemaCacheEntry>>,
    /// Performance metrics
    metrics: Arc<RwLock<ToolGroupMetrics>>,
    /// Available tools (populated after initialization)
    available_tools: AsyncRwLock<Vec<String>>,
    /// Tool schemas (static, can be shared)
    static_schemas: HashMap<String, ToolSchema>,
}

impl SystemToolGroup {
    /// Create a new SystemToolGroup with default caching
    pub fn new() -> Self {
        Self::with_cache_config(ToolGroupCacheConfig::default())
    }

    /// Create a new SystemToolGroup with custom cache configuration
    pub fn with_cache_config(cache_config: ToolGroupCacheConfig) -> Self {
        Self {
            initialized: false,
            cache_config,
            tools_cache: AsyncRwLock::new(None),
            schema_cache: AsyncRwLock::new(HashMap::new()),
            metrics: Arc::new(RwLock::new(ToolGroupMetrics::default())),
            available_tools: AsyncRwLock::new(Vec::new()),
            static_schemas: Self::create_tool_schemas(),
        }
    }

    /// Create a new SystemToolGroup with no caching (for testing)
    pub fn without_cache() -> Self {
        Self::with_cache_config(ToolGroupCacheConfig::no_caching())
    }

    /// Create tool schemas for known crucible-tools
    fn create_tool_schemas() -> HashMap<String, ToolSchema> {
        let mut schemas = HashMap::new();

        // System tools
        schemas.insert("system_info".to_string(), ToolSchema {
            name: "system_info".to_string(),
            description: "Get system information (OS, memory, disk usage)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "os": {"type": "string"},
                    "memory": {"type": "object"},
                    "disk": {"type": "object"}
                }
            })),
        });

        schemas.insert("list_files".to_string(), ToolSchema {
            name: "list_files".to_string(),
            description: "List files in a directory".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path to list"}
                },
                "required": ["path"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "string"}
            })),
        });

        schemas.insert("execute_command".to_string(), ToolSchema {
            name: "execute_command".to_string(),
            description: "Execute a system command".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Command to execute"},
                    "args": {"type": "array", "items": {"type": "string"}, "description": "Command arguments"}
                },
                "required": ["command"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "stdout": {"type": "string"},
                    "stderr": {"type": "string"},
                    "exit_code": {"type": "integer"}
                }
            })),
        });

        schemas.insert("read_file".to_string(), ToolSchema {
            name: "read_file".to_string(),
            description: "Read contents of a file".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "string"
            })),
        });

        schemas.insert("get_environment".to_string(), ToolSchema {
            name: "get_environment".to_string(),
            description: "Get environment variables".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "filter": {"type": "string", "description": "Optional filter pattern"}
                },
                "required": []
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "additionalProperties": {"type": "string"}
            })),
        });

        // Vault tools
        schemas.insert("search_by_properties".to_string(), ToolSchema {
            name: "search_by_properties".to_string(),
            description: "Search vault notes by frontmatter properties".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "properties": {"type": "object", "description": "Property key-value pairs to match"},
                    "limit": {"type": "integer", "description": "Maximum number of results"}
                },
                "required": ["properties"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        });

        schemas.insert("search_by_tags".to_string(), ToolSchema {
            name: "search_by_tags".to_string(),
            description: "Search vault notes by tags".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags to search for"},
                    "match_all": {"type": "boolean", "description": "Whether to match all tags or any"}
                },
                "required": ["tags"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        });

        schemas.insert("get_vault_stats".to_string(), ToolSchema {
            name: "get_vault_stats".to_string(),
            description: "Get vault statistics".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "total_notes": {"type": "integer"},
                    "total_size": {"type": "integer"},
                    "tags": {"type": "array", "items": {"type": "string"}}
                }
            })),
        });

        schemas.insert("create_note".to_string(), ToolSchema {
            name: "create_note".to_string(),
            description: "Create a new vault note".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path for the new note"},
                    "title": {"type": "string", "description": "Note title"},
                    "content": {"type": "string", "description": "Note content"},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": "Note tags"}
                },
                "required": ["path", "title", "content"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "created": {"type": "boolean"}
                }
            })),
        });

        // Database tools
        schemas.insert("semantic_search".to_string(), ToolSchema {
            name: "semantic_search".to_string(),
            description: "Perform semantic search on vault content".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "limit": {"type": "integer", "description": "Maximum results"}
                },
                "required": ["query"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        });

        schemas.insert("search_by_content".to_string(), ToolSchema {
            name: "search_by_content".to_string(),
            description: "Search vault content by text".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "case_sensitive": {"type": "boolean", "description": "Case sensitive search"},
                    "limit": {"type": "integer", "description": "Maximum results"}
                },
                "required": ["query"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        });

        schemas.insert("search_documents".to_string(), ToolSchema {
            name: "search_documents".to_string(),
            description: "Search documents using various criteria".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "top_k": {"type": "integer", "description": "Number of results to return"},
                    "filters": {"type": "object", "description": "Search filters"}
                },
                "required": ["query"]
            }),
            output_schema: Some(serde_json::json!({
                "type": "array",
                "items": {"type": "object"}
            })),
        });

        // Search tools
        schemas.insert("rebuild_index".to_string(), ToolSchema {
            name: "rebuild_index".to_string(),
            description: "Rebuild the search index".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "status": {"type": "string"},
                    "documents_indexed": {"type": "integer"}
                }
            })),
        });

        schemas.insert("get_index_stats".to_string(), ToolSchema {
            name: "get_index_stats".to_string(),
            description: "Get search index statistics".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "total_documents": {"type": "integer"},
                    "index_size": {"type": "integer"}
                }
            })),
        });

        schemas
    }
}

impl Default for SystemToolGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolGroup for SystemToolGroup {
    fn group_name(&self) -> &str {
        "system"
    }

    fn group_description(&self) -> &str {
        "System tools (crucible-tools) for vault management, search, and system operations"
    }

    async fn discover_tools(&mut self) -> ToolGroupResult<Vec<String>> {
        let start_time = Instant::now();

        // Initialize if needed
        if !self.is_initialized() {
            self.initialize().await?;
        }

        // Check cache first
        if self.cache_config.caching_enabled {
            let cache = self.tools_cache.write().await;
            if let Some(cached) = &*cache {
                if cached.is_valid() {
                    // Cache hit
                    self.metrics.write().unwrap().record_cache_hit();
                    tracing::debug!("Tool discovery cache hit for {} tools", cached.tools.len());
                    return Ok(cached.tools.clone());
                }
            }
            // Cache miss
            self.metrics.write().unwrap().record_cache_miss();
        }

        // Perform actual discovery
        let tools = self.perform_discovery().await?;

        // Update cache
        if self.cache_config.caching_enabled {
            let cache_entry = ToolCacheEntry::new(tools.clone(), self.cache_config.discovery_ttl);
            *self.tools_cache.write().await = Some(cache_entry);
        }

        // Update metrics
        let duration = start_time.elapsed();
        self.metrics.write().unwrap().add_discovery_time(duration);

        tracing::info!("Discovered {} system tools in {}ms", tools.len(), duration.as_millis());
        Ok(tools)
    }

    async fn list_tools(&mut self) -> ToolGroupResult<Vec<String>> {
        // If not initialized, do lazy initialization
        if !self.is_initialized() {
            self.initialize().await?;
        }

        // Check if we have cached tools
        if self.cache_config.caching_enabled {
            let cache = self.tools_cache.read().await;
            if let Some(cached) = &*cache {
                if cached.is_valid() {
                    return Ok(cached.tools.clone());
                }
            }
        }

        // Fall back to discovery
        self.discover_tools().await
    }

    async fn get_tool_schema(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>> {
        // Check cache first
        if self.cache_config.caching_enabled {
            let cache = self.schema_cache.write().await;
            if let Some(cached) = cache.get(tool_name) {
                if cached.is_valid() {
                    self.metrics.write().unwrap().record_cache_hit();
                    return Ok(cached.schema.clone());
                }
            }
            // Cache miss
            self.metrics.write().unwrap().record_cache_miss();
        }

        // Perform actual schema retrieval
        let schema = self.perform_schema_retrieval(tool_name).await?;

        // Update cache
        if self.cache_config.caching_enabled {
            let cache_entry = SchemaCacheEntry::new(schema.clone(), self.cache_config.schema_ttl);
            let mut cache = self.schema_cache.write().await;
            cache.insert(tool_name.to_string(), cache_entry);

            // Enforce cache size limit
            if cache.len() > self.cache_config.max_schema_cache_size {
                // Remove oldest entries (simple FIFO strategy)
                let mut keys: Vec<String> = cache.keys().cloned().collect();
                keys.sort(); // This is a crude way to remove some entries - in practice, you'd want timestamp-based eviction
                let excess = cache.len() - self.cache_config.max_schema_cache_size;
                for key in keys.into_iter().take(excess) {
                    cache.remove(&key);
                }
            }
        }

        Ok(schema)
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        args: &[String],
    ) -> ToolGroupResult<ToolResult> {
        let start_time = Instant::now();

        // Initialize if needed (this should be ensured by the registry)
        if !self.is_initialized() {
            return Err(ToolGroupError::InitializationFailed(
                "SystemToolGroup not initialized".to_string()
            ));
        }

        // Convert string arguments to JSON parameters
        let params = self.convert_args_to_params(tool_name, args)?;

        // Execute the tool through crucible-tools
        let result = crucible_tools::execute_tool(
            tool_name.to_string(),
            params,
            Some("repl_user".to_string()),
            Some("repl_session".to_string()),
        ).await.map_err(|e| ToolGroupError::ExecutionFailed(format!("crucible-tools error: {}", e)))?;

        // Update metrics
        let duration = start_time.elapsed();
        self.metrics.write().unwrap().add_execution_time(duration);

        // Convert result back to ToolResult format
        self.convert_crucible_result_to_tool_result(tool_name, result)
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    async fn initialize(&mut self) -> ToolGroupResult<()> {
        if self.initialized {
            return Ok(());
        }

        let start_time = Instant::now();
        tracing::info!("Initializing SystemToolGroup with crucible-tools");

        // Initialize crucible-tools library
        crucible_tools::init();

        // Load all tools
        crucible_tools::load_all_tools().await
            .map_err(|e| ToolGroupError::InitializationFailed(
                format!("Failed to load crucible-tools: {}", e)
            ))?;

        // Discover available tools
        let tools = crucible_tools::list_registered_tools().await;
        *self.available_tools.write().await = tools.clone();

        self.initialized = true;

        // Update metrics
        let duration = start_time.elapsed();
        self.metrics.write().unwrap().initialization_time_ms = Some(duration.as_millis() as u64);

        tracing::info!("SystemToolGroup initialized with {} tools in {}ms", tools.len(), duration.as_millis());
        Ok(())
    }

    async fn refresh_cache(&mut self) -> ToolGroupResult<()> {
        tracing::info!("Refreshing SystemToolGroup cache");

        // Clear tool discovery cache
        if self.cache_config.caching_enabled {
            *self.tools_cache.write().await = None;
        }

        // Clear schema cache
        if self.cache_config.caching_enabled {
            self.schema_cache.write().await.clear();
        }

        // Re-discover tools
        if self.is_initialized() {
            self.discover_tools().await?;
        }

        tracing::info!("SystemToolGroup cache refreshed");
        Ok(())
    }

    fn get_metrics(&self) -> ToolGroupMetrics {
        self.metrics.read().unwrap().clone()
    }

    fn get_cache_config(&self) -> &ToolGroupCacheConfig {
        &self.cache_config
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        let available_count = match self.available_tools.try_read() {
            Ok(guard) => guard.len(),
            Err(_) => 0,
        };
        metadata.insert("tool_count".to_string(), available_count.to_string());
        metadata.insert("initialized".to_string(), self.initialized.to_string());
        metadata.insert("backend".to_string(), "crucible-tools".to_string());
        metadata.insert("version".to_string(), crucible_tools::VERSION.to_string());
        metadata.insert("caching_enabled".to_string(), self.cache_config.caching_enabled.to_string());
        metadata
    }

    /// Internal method: Perform actual tool discovery
    async fn perform_discovery(&self) -> ToolGroupResult<Vec<String>> {
        // Get all available tools from crucible-tools
        let tools = crucible_tools::list_registered_tools().await;

        // Update the cached tools list
        *self.available_tools.write().await = tools.clone();

        tracing::debug!("Performed actual discovery of {} system tools", tools.len());
        Ok(tools)
    }

    /// Internal method: Perform actual schema retrieval
    async fn perform_schema_retrieval(&self, tool_name: &str) -> ToolGroupResult<Option<ToolSchema>> {
        // Check our static schemas first
        if let Some(schema) = self.static_schemas.get(tool_name) {
            return Ok(Some(schema.clone()));
        }

        // For unknown tools, return None
        tracing::debug!("No schema available for tool: {}", tool_name);
        Ok(None)
    }
}

impl ParameterConverter for SystemToolGroup {
    fn convert_args_to_params(&self, tool_name: &str, args: &[String]) -> ToolGroupResult<Value> {
        match tool_name {
            // Tools that take no arguments
            "system_info" | "get_vault_stats" | "get_index_stats" | "get_environment" => {
                if !args.is_empty() {
                    return Err(ToolGroupError::ParameterConversionFailed(
                        format!("{} takes no arguments, got {}", tool_name, args.len())
                    ));
                }
                Ok(Value::Object(serde_json::Map::new()))
            }

            // Tools that take a single string argument
            "list_files" | "read_file" | "semantic_search" | "search_by_content" | "search_documents" => {
                if args.len() != 1 {
                    return Err(ToolGroupError::ParameterConversionFailed(
                        format!("{} requires exactly 1 argument, got {}", tool_name, args.len())
                    ));
                }
                let mut params = serde_json::Map::new();
                match tool_name {
                    "list_files" | "read_file" => {
                        params.insert("path".to_string(), Value::String(args[0].clone()));
                    }
                    "semantic_search" | "search_by_content" | "search_documents" => {
                        params.insert("query".to_string(), Value::String(args[0].clone()));
                    }
                    _ => {}
                }
                Ok(Value::Object(params))
            }

            // Tools that take multiple arguments
            "search_by_tags" => {
                if args.is_empty() {
                    return Err(ToolGroupError::ParameterConversionFailed(
                        "search_by_tags requires at least 1 argument (tag list)".to_string()
                    ));
                }
                let mut params = serde_json::Map::new();
                let tags: Vec<Value> = args.iter().map(|s| Value::String(s.clone())).collect();
                params.insert("tags".to_string(), Value::Array(tags));
                Ok(Value::Object(params))
            }

            // execute_command takes command and optional args
            "execute_command" => {
                if args.is_empty() {
                    return Err(ToolGroupError::ParameterConversionFailed(
                        "execute_command requires at least a command".to_string()
                    ));
                }
                let mut params = serde_json::Map::new();
                params.insert("command".to_string(), Value::String(args[0].clone()));
                if args.len() > 1 {
                    let cmd_args: Vec<Value> = args[1..].iter().map(|s| Value::String(s.clone())).collect();
                    params.insert("args".to_string(), Value::Array(cmd_args));
                }
                Ok(Value::Object(params))
            }

            // create_note: path, title, content, optional tags
            "create_note" => {
                if args.len() < 3 {
                    return Err(ToolGroupError::ParameterConversionFailed(
                        "create_note requires at least 3 arguments: path, title, content".to_string()
                    ));
                }
                let mut params = serde_json::Map::new();
                params.insert("path".to_string(), Value::String(args[0].clone()));
                params.insert("title".to_string(), Value::String(args[1].clone()));
                params.insert("content".to_string(), Value::String(args[2].clone()));
                if args.len() > 3 {
                    let tags: Vec<Value> = args[3..].iter().map(|s| Value::String(s.clone())).collect();
                    params.insert("tags".to_string(), Value::Array(tags));
                }
                Ok(Value::Object(params))
            }

            // Default: pass args as array
            _ => {
                let args_json: Vec<Value> = args.iter().map(|s| Value::String(s.clone())).collect();
                Ok(Value::Array(args_json))
            }
        }
    }

    fn validate_params(&self, _tool_name: &str, _params: &Value) -> ToolGroupResult<()> {
        // Basic validation - could be expanded to use JSON schemas
        Ok(())
    }
}

impl SystemToolGroup {
    /// Convert crucible_tools ToolResult to REPL ToolResult
    fn convert_crucible_result_to_tool_result(&self, tool_name: &str, result: crucible_tools::ToolResult) -> ToolGroupResult<ToolResult> {
        // Convert to REPL ToolResult format
        if result.success {
            let output = match result.data {
                Some(data) => {
                    // Pretty print the data
                    serde_json::to_string_pretty(&data)
                        .unwrap_or_else(|_| format!("Data: {:?}", data))
                }
                None => format!("{} executed successfully", tool_name),
            };
            Ok(ToolResult::success(output))
        } else {
            let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
            Ok(ToolResult::error(error_msg))
        }
    }
}

impl ResultConverter for SystemToolGroup {
    fn convert_to_tool_result(&self, _tool_name: &str, _raw_result: Value) -> ToolGroupResult<ToolResult> {
        // This method is required by the trait but not used in SystemToolGroup
        // since we handle crucible_tools::ToolResult directly
        Err(ToolGroupError::ResultConversionFailed(
            "SystemToolGroup uses direct crucible_tools::ToolResult conversion".to_string()
        ))
    }
}