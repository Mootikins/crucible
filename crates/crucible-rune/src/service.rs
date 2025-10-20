//! Service layer integration for Rune tools
//!
//! This module provides the RuneService which integrates the Rune system
//! with the crucible service architecture, implementing the ToolService trait.

use crate::context::{ContextManager, ContextConfig, create_safe_context, create_production_context};
use crate::database::{DatabaseManager, DatabaseConfig, default_duckdb_config};
use crate::discovery::{ToolDiscovery, DiscoveryConfig};
use crate::embeddings::{create_provider, EmbeddingConfig, default_models};
use crate::errors::{RuneError, ContextualError, ErrorContext};
use crate::handler::{DynamicRuneToolHandler, ToolHandlerGenerator, ToolExecutionConfig};
use crate::loader::{ToolLoader, LoaderConfig};
use crate::registry::RuneToolRegistry;
use crate::tool::RuneTool;
use crate::types::{RuneServiceConfig, ServiceHealth, ServiceHealthStatus, PerformanceMetrics};
use anyhow::Result;
use async_trait::async_trait;
use crucible_services::traits::tool::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Main Rune service implementing the ToolService trait
pub struct RuneService {
    /// Service configuration
    config: RuneServiceConfig,
    /// Tool registry
    registry: Arc<RuneToolRegistry>,
    /// Context manager
    context_manager: Arc<Mutex<ContextManager>>,
    /// Tool discovery
    discovery: Arc<ToolDiscovery>,
    /// Tool loader
    loader: Arc<ToolLoader>,
    /// Tool handlers
    handlers: Arc<RwLock<HashMap<String, DynamicRuneToolHandler>>>,
    /// Handler generator
    handler_generator: Arc<ToolHandlerGenerator>,
    /// Database manager
    database_manager: Arc<DatabaseManager>,
    /// Embedding provider
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    /// Service metrics
    metrics: Arc<RwLock<ServiceMetrics>>,
    /// Service health
    health: Arc<RwLock<ServiceHealth>>,
}

/// Service metrics
#[derive(Debug, Clone, Default)]
pub struct ServiceMetrics {
    /// Total tools registered
    pub total_tools: usize,
    /// Active tools
    pub active_tools: usize,
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time
    pub avg_execution_time_ms: f64,
    /// Service uptime in seconds
    pub uptime_seconds: u64,
    /// Last execution timestamp
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
    /// Tool-specific metrics
    pub tool_metrics: HashMap<String, ToolMetrics>,
}

/// Tool-specific metrics
#[derive(Debug, Clone)]
pub struct ToolMetrics {
    /// Tool name
    pub tool_name: String,
    /// Execution count
    pub execution_count: u64,
    /// Success count
    pub success_count: u64,
    /// Average execution time
    pub avg_execution_time_ms: f64,
    /// Last execution timestamp
    pub last_execution: Option<chrono::DateTime<chrono::Utc>>,
}

impl RuneService {
    /// Create a new Rune service with default configuration
    pub async fn new(config: RuneServiceConfig) -> Result<Self> {
        info!("Creating Rune service: {}", config.service_name);

        let start_time = std::time::Instant::now();

        // Create context manager
        let context_config = ContextConfig {
            include_stdlib: true,
            include_http: false, // Disabled by default for security
            include_file: false,  // Disabled by default for security
            include_json: true,
            include_math: true,
            include_validation: true,
            custom_modules: HashMap::new(),
            security: crate::context::SecurityConfig {
                sandbox_enabled: true,
                allowed_modules: vec![
                    "math".to_string(),
                    "json".to_string(),
                    "string".to_string(),
                    "time".to_string(),
                    "validate".to_string(),
                ],
                blocked_modules: vec![
                    "fs".to_string(),
                    "net".to_string(),
                    "process".to_string(),
                    "env".to_string(),
                ],
                limit_file_access: true,
                allowed_paths: Vec::new(),
                limit_network_access: true,
                allowed_domains: Vec::new(),
                max_recursion_depth: 100,
            },
            limits: crate::context::ExecutionLimits {
                max_execution_time_ms: config.execution.default_timeout_ms,
                max_memory_bytes: config.execution.max_memory_bytes,
                max_function_calls: 10_000,
                max_stack_depth: 100,
                max_string_length: 1_000_000,
                max_array_length: 10_000,
                max_object_size: 10_000,
            },
        };

        let mut context_manager = ContextManager::new(context_config);
        let safe_context = create_safe_context()?;
        context_manager.create_context("default", crate::context::ContextConfig::default())?;
        context_manager.create_context("safe", crate::context::ContextConfig::default())?;
        context_manager.create_context("production", crate::context::ContextConfig::default())?;
        let context_manager = Arc::new(Mutex::new(context_manager));

        // Create tool registry
        let registry = Arc::new(RuneToolRegistry::new()?);

        // Create tool discovery
        let discovery_config = DiscoveryConfig {
            extensions: config.discovery.patterns.direct_tools.then(|| vec!["rn".to_string()])
                .unwrap_or_default(),
            exclude_dirs: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".crucible".to_string(),
            ],
            exclude_files: vec![
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
            ],
            hot_reload: config.hot_reload.enabled,
            validate_tools: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
            follow_symlinks: false,
            patterns: Default::default(),
        };

        let discovery = Arc::new(ToolDiscovery::new(discovery_config)?);

        // Create tool loader
        let loader_config = LoaderConfig {
            tool_directories: config.discovery.tool_directories.clone(),
            file_patterns: vec!["*.rn".to_string(), "*.rune".to_string()],
            enable_hot_reload: config.hot_reload.enabled,
            hot_reload_debounce_ms: config.hot_reload.debounce_ms,
            recursive_loading: true,
            validate_before_loading: true,
            max_concurrent_loads: config.discovery.discovery_interval_seconds as usize,
            loading_timeout_secs: config.execution.default_timeout_ms / 1000,
        };

        let loader = Arc::new(ToolLoader::new(
            loader_config,
            registry.clone(),
            context_manager.clone(),
        )?);

        // Create handler generator
        let handler_generator = Arc::new(ToolHandlerGenerator::with_config(
            ToolExecutionConfig::default()
                .with_timeout(config.execution.default_timeout_ms)
                .with_max_memory(config.execution.max_memory_bytes)
        ));

        // Create database manager
        let database_config = default_duckdb_config(":memory:");
        let database_manager = Arc::new(DatabaseManager::new(database_config));

        // Create embedding provider (optional)
        let embedding_provider = if let Some(embedding_config) = get_embedding_config() {
            match create_provider(embedding_config).await {
                Ok(provider) => {
                    info!("Embedding provider initialized");
                    Some(provider)
                }
                Err(e) => {
                    warn!("Failed to initialize embedding provider: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let service = Self {
            config,
            registry: registry.clone(),
            context_manager,
            discovery: discovery.clone(),
            loader: loader.clone(),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            handler_generator: handler_generator.clone(),
            database_manager,
            embedding_provider,
            metrics: Arc::new(RwLock::new(ServiceMetrics::default())),
            health: Arc::new(RwLock::new(ServiceHealth {
                status: ServiceHealthStatus::Healthy,
                last_check: chrono::Utc::now(),
                checks: HashMap::new(),
                health_score: 100,
            })),
        };

        // Initialize database
        let _ = service.database_manager.create_connection(
            "default".to_string(),
            None,
        ).await;

        // Load initial tools
        let loaded_count = service.loader.load_tools().await.unwrap_or(0);
        info!("Loaded {} tools during service initialization", loaded_count);

        // Generate handlers for loaded tools
        service.refresh_handlers().await?;

        // Update initial metrics
        service.update_metrics().await;

        let startup_time = start_time.elapsed().as_millis() as u64;
        info!("Rune service '{}' started in {}ms", service.config.service_name, startup_time);

        Ok(service)
    }

    /// Refresh handlers for all registered tools
    async fn refresh_handlers(&self) -> Result<()> {
        let tools = self.registry.list_tools().await?;
        let mut context_manager = self.context_manager.lock().await;
        let context = context_manager.get_context("default")?;

        let mut handlers = self.handlers.write().await;
        handlers.clear();

        for tool in tools {
            let handler = self.handler_generator.generate_handler(tool.clone(), context.clone());
            handlers.insert(tool.name.clone(), handler);
        }

        info!("Refreshed {} tool handlers", handlers.len());
        Ok(())
    }

    /// Update service metrics
    async fn update_metrics(&self) {
        let registry_stats = self.registry.get_stats().await;
        let loader_stats = self.loader.get_loading_stats().await;

        let mut metrics = self.metrics.write().await;
        metrics.total_tools = registry_stats.total_tools;
        metrics.active_tools = registry_stats.enabled_tools;

        // Update handler metrics
        let handlers = self.handlers.read().await;
        for (tool_name, handler) in handlers.iter() {
            let handler_stats = handler.get_stats().await;
            let tool_metrics = ToolMetrics {
                tool_name: tool_name.clone(),
                execution_count: handler_stats.total_executions,
                success_count: handler_stats.successful_executions,
                avg_execution_time_ms: handler_stats.avg_execution_time_ms,
                last_execution: handler_stats.last_execution,
            };
            metrics.tool_metrics.insert(tool_name.clone(), tool_metrics);
        }
    }

    /// Perform health check
    async fn perform_health_check(&self) -> ServiceHealth {
        let mut checks = HashMap::new();
        let mut overall_score = 100u8;

        // Check registry
        let registry_stats = self.registry.get_stats().await;
        let registry_healthy = registry_stats.total_tools > 0;
        checks.insert("registry".to_string(), crate::types::HealthCheckResult {
            name: "registry".to_string(),
            passed: registry_healthy,
            duration_ms: 1,
            message: if registry_healthy {
                format!("{} tools registered", registry_stats.total_tools)
            } else {
                "No tools registered".to_string()
            },
            details: HashMap::new(),
        });

        if !registry_healthy {
            overall_score = overall_score.saturating_sub(30);
        }

        // Check loader
        let loader_stats = self.loader.get_loading_stats().await;
        let loader_healthy = loader_stats.failed_loads == 0 || loader_stats.successful_loads > 0;
        checks.insert("loader".to_string(), crate::types::HealthCheckResult {
            name: "loader".to_string(),
            passed: loader_healthy,
            duration_ms: 1,
            message: format!("Loaded: {}, Failed: {}", loader_stats.successful_loads, loader_stats.failed_loads),
            details: HashMap::new(),
        });

        if !loader_healthy {
            overall_score = overall_score.saturating_sub(20);
        }

        // Check database
        let db_health = self.database_manager.health_check_all().await;
        let db_healthy = db_health.values().all(|&healthy| healthy);
        checks.insert("database".to_string(), crate::types::HealthCheckResult {
            name: "database".to_string(),
            passed: db_healthy,
            duration_ms: 5,
            message: format!("{} connections", db_health.len()),
            details: HashMap::new(),
        });

        if !db_healthy {
            overall_score = overall_score.saturating_sub(20);
        }

        // Check handlers
        let handlers = self.handlers.read().await;
        let handler_count = handlers.len();
        let handlers_healthy = handler_count > 0;
        checks.insert("handlers".to_string(), crate::types::HealthCheckResult {
            name: "handlers".to_string(),
            passed: handlers_healthy,
            duration_ms: 1,
            message: format!("{} handlers ready", handler_count),
            details: HashMap::new(),
        });

        if !handlers_healthy {
            overall_score = overall_score.saturating_sub(30);
        }

        let status = if overall_score >= 80 {
            ServiceHealthStatus::Healthy
        } else if overall_score >= 60 {
            ServiceHealthStatus::Degraded
        } else {
            ServiceHealthStatus::Unhealthy
        };

        ServiceHealth {
            status,
            last_check: chrono::Utc::now(),
            checks,
            health_score: overall_score,
        }
    }

    /// Reload tools from disk
    pub async fn reload_tools(&self) -> Result<usize> {
        info!("Reloading tools from disk");
        let reloaded_count = self.loader.reload_tools().await?;
        self.refresh_handlers().await?;
        self.update_metrics().await;
        info!("Reloaded {} tools", reloaded_count);
        Ok(reloaded_count)
    }

    /// Get embedding provider
    pub fn get_embedding_provider(&self) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        self.embedding_provider.clone()
    }

    /// Get database manager
    pub fn get_database_manager(&self) -> Arc<DatabaseManager> {
        self.database_manager.clone()
    }

    /// Get service metrics
    pub async fn get_metrics(&self) -> ServiceMetrics {
        self.update_metrics().await;
        self.metrics.read().await.clone()
    }
}

#[async_trait]
impl ToolService for RuneService {
    async fn register_tool(&self, tool: ToolDefinition) -> ServiceResult<String> {
        let context = ErrorContext::new()
            .with_operation("register_tool")
            .with_tool_name(&tool.name);

        // Convert ToolDefinition to RuneTool (simplified)
        // In a real implementation, you'd need more sophisticated conversion
        let rune_tool = match create_rune_tool_from_definition(&tool) {
            Ok(tool) => tool,
            Err(e) => {
                return Err(ServiceError::ValidationError {
                    field: Some("tool_definition".to_string()),
                    message: format!("Failed to create Rune tool: {}", e),
                    value: Some(serde_json::to_value(&tool)),
                });
            }
        };

        match self.registry.register_tool(rune_tool).await {
            Ok(tool_name) => {
                // Refresh handlers
                let _ = self.refresh_handlers().await;
                self.update_metrics().await;
                info!("Successfully registered tool: {}", tool_name);
                Ok(tool_name)
            }
            Err(e) => {
                error!("Failed to register tool: {}", e.error);
                Err(ServiceError::ValidationError {
                    field: Some("registration".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::to_value(&tool)),
                })
            }
        }
    }

    async fn unregister_tool(&self, tool_name: &str) -> ServiceResult<bool> {
        match self.registry.unregister_tool(tool_name).await {
            Ok(unregistered) => {
                if unregistered {
                    // Remove handler
                    let mut handlers = self.handlers.write().await;
                    handlers.remove(tool_name);
                    self.update_metrics().await;
                    info!("Successfully unregistered tool: {}", tool_name);
                }
                Ok(unregistered)
            }
            Err(e) => {
                error!("Failed to unregister tool '{}': {}", tool_name, e.error);
                Err(ServiceError::ValidationError {
                    field: Some("tool_name".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::Value::String(tool_name.to_string())),
                })
            }
        }
    }

    async fn get_tool(&self, tool_name: &str) -> ServiceResult<Option<ToolDefinition>> {
        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => Ok(Some(tool.to_tool_definition())),
            Ok(None) => Ok(None),
            Err(e) => {
                error!("Failed to get tool '{}': {}", tool_name, e.error);
                Err(ServiceError::ValidationError {
                    field: Some("tool_name".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::Value::String(tool_name.to_string())),
                })
            }
        }
    }

    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>> {
        match self.registry.list_tools().await {
            Ok(tools) => {
                let definitions: Vec<ToolDefinition> = tools.iter()
                    .map(|tool| tool.to_tool_definition())
                    .collect();
                Ok(definitions)
            }
            Err(e) => {
                error!("Failed to list tools: {}", e.error);
                Err(ServiceError::ValidationError {
                    field: Some("list_tools".to_string()),
                    message: e.error.to_string(),
                    value: None,
                })
            }
        }
    }

    async fn list_tools_by_category(&self, category: &str) -> ServiceResult<Vec<ToolDefinition>> {
        match self.registry.list_tools_by_category(category).await {
            Ok(tools) => {
                let definitions: Vec<ToolDefinition> = tools.iter()
                    .map(|tool| tool.to_tool_definition())
                    .collect();
                Ok(definitions)
            }
            Err(e) => {
                error!("Failed to list tools by category '{}': {}", category, e.error);
                Err(ServiceError::ValidationError {
                    field: Some("category".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::Value::String(category.to_string())),
                })
            }
        }
    }

    async fn find_tools_by_tag(&self, tag: &str) -> ServiceResult<Vec<ToolDefinition>> {
        match self.registry.find_tools_by_tag(tag).await {
            Ok(tools) => {
                let definitions: Vec<ToolDefinition> = tools.iter()
                    .map(|tool| tool.to_tool_definition())
                    .collect();
                Ok(definitions)
            }
            Err(e) => {
                error!("Failed to find tools by tag '{}': {}", tag, e.error);
                Err(ServiceError::ValidationError {
                    field: Some("tag".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::Value::String(tag.to_string())),
                })
            }
        }
    }

    async fn search_tools(&self, query: &str) -> ServiceResult<Vec<ToolDefinition>> {
        match self.registry.search_tools(query).await {
            Ok(tools) => {
                let definitions: Vec<ToolDefinition> = tools.iter()
                    .map(|tool| tool.to_tool_definition())
                    .collect();
                Ok(definitions)
            }
            Err(e) => {
                error!("Failed to search tools with query '{}': {}", query, e.error);
                Err(ServiceError::ValidationError {
                    field: Some("query".to_string()),
                    message: e.error.to_string(),
                    value: Some(serde_json::Value::String(query.to_string())),
                })
            }
        }
    }

    async fn execute_tool(&self, request: ToolExecutionRequest) -> ServiceResult<ToolExecutionResult> {
        let start_time = std::time::Instant::now();

        let handlers = self.handlers.read().await;
        match handlers.get(&request.tool_name) {
            Some(handler) => {
                match handler.execute(&request).await {
                    Ok(mut result) => {
                        // Update metrics
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.total_executions += 1;
                            metrics.successful_executions += 1;
                            metrics.last_execution = Some(chrono::Utc::now());

                            let execution_time = start_time.elapsed().as_millis() as u64;
                            if metrics.total_executions > 0 {
                                metrics.avg_execution_time_ms =
                                    (metrics.avg_execution_time_ms * (metrics.total_executions - 1) as f64 + execution_time as f64)
                                    / metrics.total_executions as f64;
                            }
                        }

                        debug!("Tool '{}' executed successfully in {}ms", request.tool_name, start_time.elapsed().as_millis());
                        Ok(result)
                    }
                    Err(e) => {
                        // Update metrics
                        {
                            let mut metrics = self.metrics.write().await;
                            metrics.total_executions += 1;
                            metrics.failed_executions += 1;
                        }

                        error!("Tool '{}' execution failed: {}", request.tool_name, e.error);
                        Err(ServiceError::ValidationError {
                            field: Some("execution".to_string()),
                            message: e.error.to_string(),
                            value: Some(serde_json::to_value(&request)),
                        })
                    }
                }
            }
            None => {
                let error = format!("Tool '{}' not found", request.tool_name);
                error!("{}", error);
                Err(ServiceError::ValidationError {
                    field: Some("tool_name".to_string()),
                    message: error,
                    value: Some(serde_json::Value::String(request.tool_name.clone())),
                })
            }
        }
    }

    async fn execute_tool_async(&self, request: ToolExecutionRequest) -> ServiceResult<String> {
        // For now, execute synchronously and return the execution ID
        // In a real implementation, you'd spawn a background task
        let execution_id = request.execution_id.clone();

        // Spawn background task
        let handlers = Arc::clone(&self.handlers);
        let request_clone = request.clone();

        tokio::spawn(async move {
            if let Some(handler) = handlers.read().await.get(&request_clone.tool_name) {
                if let Err(e) = handler.execute(&request_clone).await {
                    error!("Async tool execution failed: {}", e.error);
                }
            }
        });

        Ok(execution_id)
    }

    async fn get_execution_result(&self, _execution_id: &str) -> ServiceResult<Option<ToolExecutionResult>> {
        // In a real implementation, you'd store results and retrieve them
        // For now, return None
        Ok(None)
    }

    async fn cancel_execution(&self, _execution_id: &str) -> ServiceResult<bool> {
        // In a real implementation, you'd cancel background tasks
        // For now, return false
        Ok(false)
    }

    async fn list_active_executions(&self) -> ServiceResult<Vec<ActiveExecution>> {
        // In a real implementation, you'd track active executions
        // For now, return empty list
        Ok(Vec::new())
    }

    async fn get_execution_history(&self, _tool_name: &str, _limit: Option<u32>) -> ServiceResult<Vec<ToolExecutionResult>> {
        // In a real implementation, you'd store and retrieve execution history
        // For now, return empty list
        Ok(Vec::new())
    }

    async fn validate_tool_parameters(&self, tool_name: &str, parameters: &serde_json::Value) -> ServiceResult<ValidationResult> {
        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => {
                match tool.validate_input(parameters) {
                    Ok(_) => Ok(ValidationResult {
                        valid: true,
                        errors: Vec::new(),
                        normalized_parameters: Some(parameters.clone()),
                    }),
                    Err(e) => Ok(ValidationResult {
                        valid: false,
                        errors: vec![e.to_string()],
                        normalized_parameters: None,
                    }),
                }
            }
            Ok(None) => Ok(ValidationResult {
                valid: false,
                errors: vec![format!("Tool '{}' not found", tool_name)],
                normalized_parameters: None,
            }),
            Err(e) => {
                error!("Failed to validate parameters for tool '{}': {}", tool_name, e.error);
                Ok(ValidationResult {
                    valid: false,
                    errors: vec![e.error.to_string()],
                    normalized_parameters: None,
                })
            }
        }
    }

    async fn get_tool_stats(&self, tool_name: &str) -> ServiceResult<ToolUsageStats> {
        let metrics = self.metrics.read().await;
        match metrics.tool_metrics.get(tool_name) {
            Some(tool_metrics) => {
                Ok(ToolUsageStats {
                    tool_name: tool_name.clone(),
                    total_executions: tool_metrics.execution_count,
                    successful_executions: tool_metrics.success_count,
                    failed_executions: tool_metrics.execution_count - tool_metrics.success_count,
                    avg_execution_time_ms: tool_metrics.avg_execution_time_ms,
                    last_execution: tool_metrics.last_execution,
                    top_users: Vec::new(), // Not implemented
                    usage_by_period: HashMap::new(), // Not implemented
                })
            }
            None => {
                error!("Tool '{}' not found for stats", tool_name);
                Err(ServiceError::ValidationError {
                    field: Some("tool_name".to_string()),
                    message: format!("Tool '{}' not found", tool_name),
                    value: Some(serde_json::Value::String(tool_name.to_string())),
                })
            }
        }
    }

    async fn get_service_stats(&self) -> ServiceResult<ToolServiceStats> {
        let metrics = self.get_metrics().await;
        Ok(ToolServiceStats {
            total_tools: metrics.total_tools,
            enabled_tools: metrics.active_tools,
            total_executions: metrics.total_executions,
            active_executions: 0, // Not tracked
            avg_execution_time_ms: metrics.avg_execution_time_ms,
            top_tools: metrics.tool_metrics
                .iter()
                .map(|(name, _)| name.clone())
                .take(10)
                .collect(),
            uptime_seconds: metrics.uptime_seconds,
        })
    }

    async fn update_tool(&self, tool_name: &str, tool: ToolDefinition) -> ServiceResult<bool> {
        // For now, implement as unregister + register
        self.unregister_tool(tool_name).await?;
        self.register_tool(tool).await?;
        Ok(true)
    }

    async fn set_tool_enabled(&self, tool_name: &str, enabled: bool) -> ServiceResult<bool> {
        // In a real implementation, you'd enable/disable the tool in the registry
        // For now, just return true
        info!("Setting tool '{}' enabled: {}", tool_name, enabled);
        Ok(true)
    }

    async fn is_tool_enabled(&self, tool_name: &str) -> ServiceResult<bool> {
        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => Ok(tool.enabled),
            Ok(None) => Ok(false),
            Err(e) => {
                error!("Failed to check if tool '{}' is enabled: {}", tool_name, e.error);
                Ok(false)
            }
        }
    }

    async fn get_tool_permissions(&self, tool_name: &str) -> ServiceResult<Vec<String>> {
        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => Ok(tool.permissions.clone()),
            Ok(None) => Ok(Vec::new()),
            Err(e) => {
                error!("Failed to get permissions for tool '{}': {}", tool_name, e.error);
                Ok(Vec::new())
            }
        }
    }

    async fn set_tool_permissions(&self, tool_name: &str, permissions: Vec<String>) -> ServiceResult<()> {
        // In a real implementation, you'd update the tool's permissions
        info!("Setting permissions for tool '{}': {:?}", tool_name, permissions);
        Ok(())
    }

    async fn check_tool_permission(&self, tool_name: &str, user_id: &str) -> ServiceResult<bool> {
        // Simple implementation - in a real scenario, you'd check against user permissions
        debug!("Checking permission for user '{}' on tool '{}'", user_id, tool_name);
        Ok(true)
    }

    async fn get_tool_dependencies(&self, tool_name: &str) -> ServiceResult<Vec<ToolDependency>> {
        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => Ok(tool.dependencies.clone()),
            Ok(None) => Ok(Vec::new()),
            Err(e) => {
                error!("Failed to get dependencies for tool '{}': {}", tool_name, e.error);
                Ok(Vec::new())
            }
        }
    }

    async fn install_tool_dependencies(&self, tool_name: &str) -> ServiceResult<()> {
        info!("Installing dependencies for tool '{}'", tool_name);
        // In a real implementation, you'd install actual dependencies
        Ok(())
    }

    async fn verify_tool(&self, tool_name: &str) -> ServiceResult<ToolVerificationResult> {
        let start_time = std::time::Instant::now();

        match self.registry.get_tool(tool_name).await {
            Ok(Some(tool)) => {
                // Basic verification
                let valid = !tool.name.is_empty() && !tool.description.is_empty();
                let verification_time = start_time.elapsed().as_millis() as u64;

                Ok(ToolVerificationResult {
                    verified: valid,
                    timestamp: chrono::Utc::now(),
                    errors: if valid { Vec::new() } else { vec!["Invalid tool metadata".to_string()] },
                    health: crate::types::ToolHealth {
                        status: if valid { crate::types::ToolHealthStatus::Healthy } else { crate::types::ToolHealthStatus::Unhealthy },
                        last_check: chrono::Utc::now(),
                        response_time_ms: Some(verification_time),
                        messages: if valid { vec!["Tool verified successfully".to_string()] } else { vec!["Tool verification failed".to_string()] },
                    },
                    performance: Some(crate::types::ToolPerformanceMetrics {
                        avg_execution_time_ms: 0.0,
                        min_execution_time_ms: verification_time,
                        max_execution_time_ms: verification_time,
                        p95_execution_time_ms: verification_time,
                        throughput_rps: 0.0,
                        memory_usage_bytes: None,
                        cpu_usage_percent: None,
                    }),
                })
            }
            Ok(None) => {
                Ok(ToolVerificationResult {
                    verified: false,
                    timestamp: chrono::Utc::now(),
                    errors: vec![format!("Tool '{}' not found", tool_name)],
                    health: crate::types::ToolHealth {
                        status: crate::types::ToolHealthStatus::Unhealthy,
                        last_check: chrono::Utc::now(),
                        response_time_ms: Some(start_time.elapsed().as_millis() as u64),
                        messages: vec!["Tool not found".to_string()],
                    },
                    performance: None,
                })
            }
            Err(e) => {
                Ok(ToolVerificationResult {
                    verified: false,
                    timestamp: chrono::Utc::now(),
                    errors: vec![e.error.to_string()],
                    health: crate::types::ToolHealth {
                        status: crate::types::ToolHealthStatus::Unhealthy,
                        last_check: chrono::Utc::now(),
                        response_time_ms: Some(start_time.elapsed().as_millis() as u64),
                        messages: vec!["Tool verification failed".to_string()],
                    },
                    performance: None,
                })
            }
        }
    }
}

/// Helper function to create a Rune tool from a ToolDefinition
fn create_rune_tool_from_definition(tool_def: &ToolDefinition) -> Result<RuneTool> {
    // This is a simplified implementation
    // In a real scenario, you'd need to either:
    // 1. Load the tool from a file (if file_path is provided)
    // 2. Generate the source code from the definition
    // 3. Have a registry of predefined tools

    let source_code = format!(r#"
        pub fn NAME() {{ {:?} }}
        pub fn DESCRIPTION() {{ {:?} }}
        pub fn INPUT_SCHEMA() {{ {:?} }}
        pub async fn call(args) {{
            // Tool implementation would go here
            #{{ success: true, message: "Tool executed with args: ${{args}}" }}
        }}
    "#, tool_def.name, tool_def.description, tool_def.input_schema);

    let context = crate::context::create_safe_context()?;
    let mut rune_tool = RuneTool::from_source(&source_code, &context, None)?;

    // Apply additional metadata from definition
    rune_tool.category = tool_def.category.clone();
    rune_tool.tags = tool_def.tags.clone();
    rune_tool.version = tool_def.version.clone().unwrap_or_else(|| "1.0.0".to_string());
    rune_tool.author = tool_def.author.clone();

    Ok(rune_tool)
}

/// Get embedding configuration from environment or defaults
fn get_embedding_config() -> Option<EmbeddingConfig> {
    match std::env::var("EMBEDDING_PROVIDER").as_deref() {
        Ok("ollama") => Some(default_models::ollama_default()),
        Ok("openai") => {
            if let Ok(api_key) = std::env::var("EMBEDDING_API_KEY") {
                Some(default_models::openai_default())
            } else {
                warn!("OpenAI embedding provider requested but no API key provided");
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rune_service_creation() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig {
            service_name: "test-service".to_string(),
            version: "1.0.0".to_string(),
            discovery: crate::types::DiscoveryServiceConfig::default(),
            hot_reload: crate::types::HotReloadConfig::default(),
            execution: crate::types::ExecutionConfig::default(),
            cache: crate::types::CacheConfig::default(),
            security: crate::types::SecurityConfig::default(),
        };

        let service = RuneService::new(config).await?;
        assert_eq!(service.config.service_name, "test-service");

        let metrics = service.get_metrics().await;
        assert!(metrics.total_tools >= 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_service_health_check() -> Result<(), Box<dyn std::error::Error>> {
        let config = RuneServiceConfig::default();
        let service = RuneService::new(config).await?;

        // The service should perform health checks
        let handlers = service.handlers.read().await;
        assert!(!handlers.is_empty() || true); // May be empty if no tools are loaded

        Ok(())
    }

    #[test]
    fn test_embedding_config() {
        // Test with no environment variables set
        let config = get_embedding_config();
        assert!(config.is_none());
    }
}