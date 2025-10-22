//! Simple Rune service implementation
//!
//! This provides a clean interface for working with Rune tools
//! without the over-engineered service-oriented architecture.

use crate::discovery;
use crate::loader;
use crate::rune_registry;
use crate::context_factory::ContextFactory;
use crate::types::{RuneServiceConfig, SystemInfo};
use crate::types::{ToolDefinition, ToolExecutionRequest, ToolExecutionResult, ContextRef, ToolService, ServiceResult, ServiceError, ServiceHealth, ServiceMetrics, ValidationResult, ServiceStatus};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Simple Rune service for tool discovery and execution
#[derive(Debug)]
pub struct RuneService {
    /// Service configuration
    config: RuneServiceConfig,
    /// Tool registry
    registry: Arc<RwLock<rune_registry::RuneToolRegistry>>,
    /// Tool loader
    loader: Arc<loader::ToolLoader>,
    /// Tool discovery
    discovery: Arc<discovery::ToolDiscovery>,
    /// Context factory for creating fresh contexts
    context_factory: Arc<ContextFactory>,
}

impl RuneService {
    /// Create a new Rune service with the given configuration
    pub async fn new(config: RuneServiceConfig) -> Result<Self> {
        info!("Creating Rune service with configuration: {}", config.service_name);

        // Initialize core components
        let registry = Arc::new(RwLock::new(rune_registry::RuneToolRegistry::new()?));
        let loader = Arc::new(loader::ToolLoader::from_service_config(&config)?);
        let discovery = Arc::new(discovery::ToolDiscovery::new(&config.discovery)?);
        let context_factory = Arc::new(ContextFactory::new()?);

        let service = Self {
            config,
            registry,
            loader,
            discovery,
            context_factory,
        };

        // Discover tools from configured directories
        if !service.config.discovery.tool_directories.is_empty() {
            service.discover_tools_from_directories(&service.config.discovery.tool_directories).await?;
        }

        info!("Rune service initialized successfully");
        Ok(service)
    }

    /// Discover tools from a single directory
    pub async fn discover_tools_from_directory<P: AsRef<Path>>(&self, path: P) -> Result<usize> {
        let path = path.as_ref();
        debug!("Discovering tools in directory: {}", path.display());

        if !path.exists() {
            warn!("Tool directory does not exist: {}", path.display());
            return Ok(0);
        }

        let discoveries = self.discovery.discover_from_directory(path).await?;
        let mut total_tools = 0;

        for discovery in discoveries {
            info!("Discovered {} tools in {:?}", discovery.tools.len(), discovery.directory);

            for tool in discovery.tools {
                match self.registry.write().await.register_tool(tool).await {
                    Ok(_) => total_tools += 1,
                    Err(e) => warn!("Failed to register tool: {}", e),
                }
            }
        }

        info!("Successfully discovered and registered {} tools from {}", total_tools, path.display());
        Ok(total_tools)
    }

    /// Discover tools from multiple directories
    pub async fn discover_tools_from_directories(&self, directories: &[impl AsRef<Path>]) -> Result<usize> {
        let mut total_tools = 0;

        for directory in directories {
            total_tools += self.discover_tools_from_directory(directory).await?;
        }

        Ok(total_tools)
    }

    /// Get a list of all registered tools
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> {
        let registry = self.registry.read().await;
        let tools = registry.list_tools()
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                category: Some("rune".to_string()),
                version: tool.version.clone(),
                author: None,
                tags: tool.tags.clone(),
                enabled: tool.enabled,
                parameters: vec![], // Could be populated from the input schema
            })
            .collect();

        Ok(tools)
    }

    /// Get information about the service
    pub fn system_info(&self) -> SystemInfo {
        SystemInfo {
            version: "0.1.0".to_string(),
            rune_version: "0.13.3".to_string(),
            supported_extensions: vec!["rn".to_string(), "rune".to_string()],
            default_directories: self.config.discovery.tool_directories
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }
    }
}

#[async_trait::async_trait]
impl ToolService for RuneService {
    /// Execute a tool by name
    async fn execute_tool(&self, request: ToolExecutionRequest) -> ServiceResult<ToolExecutionResult> {
        debug!("Executing tool: {}", request.tool_name);

        let registry = self.registry.read().await;
        let tool = registry.get_tool(&request.tool_name).await
            .ok_or_else(|| ServiceError::ToolNotFound(request.tool_name.clone()))?;

        // Create a fresh context for each execution
        let fresh_context = self.context_factory.create_fresh_context(&request.tool_name)
            .await
            .map_err(|e| ServiceError::ExecutionError(format!("Failed to create context: {}", e)))?;

        // Execute the tool using the fresh context
        let result = tool.call(request.parameters, &fresh_context)
            .await
            .map_err(|e| ServiceError::ExecutionError(e.to_string()))?;

        debug!("Tool execution completed successfully with fresh context");
        Ok(ToolExecutionResult {
            success: true,
            result: Some(result),
            error: None,
            execution_time: std::time::Duration::from_millis(0), // TODO: track actual execution time
            tool_name: request.tool_name,
            context_ref: Some(ContextRef::new()),
        })
    }

    /// List all available tools
    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>> {
        let tools = self.registry.read().await.list_tools().await
            .map_err(|e| ServiceError::ExecutionError(e.to_string()))?;

        let tool_definitions = tools.into_iter().map(|tool| tool.to_tool_definition()).collect();
        Ok(tool_definitions)
    }

    /// Get tool definition by name
    async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>> {
        let registry = self.registry.read().await;
        let tool = registry.get_tool(name).await
            .map_err(|e| ServiceError::ExecutionError(e.to_string()))?;

        Ok(tool.map(|tool| ToolDefinition {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            category: Some("rune".to_string()),
            version: tool.version.clone(),
            author: None,
            tags: tool.tags.clone(),
            enabled: tool.enabled,
            parameters: vec![],
        }))
    }

    /// Validate a tool without executing it
    async fn validate_tool(&self, name: &str) -> ServiceResult<ValidationResult> {
        let registry = self.registry.read().await;
        let tool = registry.get_tool(name).await
            .map_err(|e| ServiceError::ExecutionError(e.to_string()))?;

        match tool {
            Some(tool) => Ok(ValidationResult {
                valid: true,
                errors: vec![],
                warnings: vec![],
                tool_name: name.to_string(),
                metadata: None,
            }),
            None => Ok(ValidationResult {
                valid: false,
                errors: vec![format!("Tool '{}' not found", name)],
                warnings: vec![],
                tool_name: name.to_string(),
                metadata: None,
            }),
        }
    }

    /// Get service health and status
    async fn service_health(&self) -> ServiceResult<ServiceHealth> {
        let tools = self.registry.read().await.list_tools().await.unwrap_or_default();
        let tool_count = tools.len();

        Ok(ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some(format!("Rune service running with {} tools", tool_count)),
            details: std::collections::HashMap::new(),
            last_check: chrono::Utc::now(),
        })
    }

    /// Get performance metrics
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        // Simple metrics implementation
        Ok(ServiceMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: std::time::Duration::from_millis(0),
            uptime: std::time::Duration::from_secs(0), // TODO: track actual uptime
            memory_usage: 0,
            cpu_usage: 0.0,
        })
    }
}