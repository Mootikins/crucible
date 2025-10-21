//! # Service Implementation Examples
//!
//! This module provides example implementations and usage patterns for the service traits.
//! These examples demonstrate how to properly implement the comprehensive service interfaces
//! with proper error handling, resource management, and performance considerations.

use crate::{
    errors::{ServiceError, ServiceResult},
    service_traits::*,
    service_types::*,
    types::ServiceHealth,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

/// ============================================================================
/// MCP GATEWAY EXAMPLE
/// ============================================================================

/// Example MCP Gateway implementation
pub struct ExampleMcpGateway {
    /// Service state
    state: Arc<RwLock<McpGatewayState>>,
    /// Configuration
    config: McpGatewayConfig,
    /// Metrics
    metrics: Arc<RwLock<ServiceMetrics>>,
}

/// MCP Gateway internal state
#[derive(Debug, Clone)]
pub struct McpGatewayState {
    /// Service running status
    running: bool,
    /// Active sessions
    sessions: HashMap<String, McpSession>,
    /// Registered tools
    tools: HashMap<String, ToolDefinition>,
    /// Active executions
    executions: HashMap<String, ActiveExecution>,
    /// Event subscribers
    subscribers: HashMap<String, mpsc::UnboundedSender<McpGatewayEvent>>,
}

/// MCP Gateway configuration
#[derive(Debug, Clone)]
pub struct McpGatewayConfig {
    /// Maximum concurrent sessions
    pub max_sessions: u32,
    /// Session timeout
    pub session_timeout: Duration,
    /// Maximum concurrent executions
    pub max_executions: u32,
    /// Execution timeout
    pub execution_timeout: Duration,
}

/// MCP Gateway event
#[derive(Debug, Clone)]
pub struct McpGatewayEvent {
    /// Event type
    pub event_type: String,
    /// Event data
    pub data: serde_json::Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ExampleMcpGateway {
    /// Create a new MCP Gateway
    pub fn new(config: McpGatewayConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(McpGatewayState {
                running: false,
                sessions: HashMap::new(),
                tools: HashMap::new(),
                executions: HashMap::new(),
                subscribers: HashMap::new(),
            })),
            config,
            metrics: Arc::new(RwLock::new(ServiceMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                average_response_time: Duration::ZERO,
                uptime: Duration::ZERO,
                memory_usage: 0,
                cpu_usage: 0.0,
            })),
        }
    }

    /// Generate unique session ID
    fn generate_session_id(&self) -> String {
        format!("session_{}", uuid::Uuid::new_v4())
    }

    /// Generate unique execution ID
    fn generate_execution_id(&self) -> String {
        format!("exec_{}", uuid::Uuid::new_v4())
    }

    /// Validate session limits
    async fn validate_session_limits(&self) -> ServiceResult<()> {
        let state = self.state.read().await;
        if state.sessions.len() >= self.config.max_sessions as usize {
            return Err(ServiceError::rate_limit_exceeded(
                "Maximum concurrent sessions reached"
            ));
        }
        Ok(())
    }

    /// Validate execution limits
    async fn validate_execution_limits(&self) -> ServiceResult<()> {
        let state = self.state.read().await;
        if state.executions.len() >= self.config.max_executions as usize {
            return Err(ServiceError::rate_limit_exceeded(
                "Maximum concurrent executions reached"
            ));
        }
        Ok(())
    }

    /// Record metrics
    async fn record_metrics(&self, success: bool, duration: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }

        // Update average response time
        let total_requests = metrics.total_requests as f64;
        let current_avg = metrics.average_response_time.as_secs_f64();
        let new_duration = duration.as_secs_f64();
        metrics.average_response_time = Duration::from_secs_f64(
            (current_avg * (total_requests - 1.0) + new_duration) / total_requests
        );
    }
}

#[async_trait]
impl ServiceLifecycle for ExampleMcpGateway {
    async fn start(&mut self) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        if state.running {
            return Err(ServiceError::configuration_error("Service already running"));
        }

        state.running = true;
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        if !state.running {
            return Err(ServiceError::configuration_error("Service not running"));
        }

        // Close all sessions
        state.sessions.clear();
        state.executions.clear();
        state.running = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        // Note: In a real implementation, this would be async
        // For this example, we'll use a tokio block_on
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.state.read().await.running
            })
        })
    }

    fn service_name(&self) -> &str {
        "McpGateway"
    }

    fn service_version(&self) -> &str {
        "1.0.0"
    }
}

#[async_trait]
impl HealthCheck for ExampleMcpGateway {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        let state = self.state.read().await;
        let metrics = self.metrics.read().await;

        let status = if state.running {
            if metrics.failed_requests as f64 / metrics.total_requests as f64 > 0.1 {
                ServiceStatus::Degraded
            } else {
                ServiceStatus::Healthy
            }
        } else {
            ServiceStatus::Unhealthy
        };

        let mut details = HashMap::new();
        details.insert("sessions".to_string(), state.sessions.len().to_string());
        details.insert("executions".to_string(), state.executions.len().to_string());
        details.insert("tools".to_string(), state.tools.len().to_string());

        Ok(ServiceHealth {
            status,
            message: Some("MCP Gateway operational".to_string()),
            last_check: chrono::Utc::now(),
            details,
        })
    }
}

#[async_trait]
impl Configurable for ExampleMcpGateway {
    type Config = McpGatewayConfig;

    async fn get_config(&self) -> ServiceResult<Self::Config> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: Self::Config) -> ServiceResult<()> {
        self.config = config;
        Ok(())
    }

    async fn validate_config(&self, config: &Self::Config) -> ServiceResult<()> {
        if config.max_sessions == 0 {
            return Err(ServiceError::validation_error("max_sessions must be > 0"));
        }
        if config.max_executions == 0 {
            return Err(ServiceError::validation_error("max_executions must be > 0"));
        }
        Ok(())
    }

    async fn reload_config(&mut self) -> ServiceResult<()> {
        // In a real implementation, this would load from a file or external source
        Ok(())
    }
}

#[async_trait]
impl Observable for ExampleMcpGateway {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        let mut metrics = self.metrics.read().await.clone();
        metrics.uptime = Duration::from_secs(3600); // Example uptime
        Ok(metrics)
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests = 0;
        metrics.successful_requests = 0;
        metrics.failed_requests = 0;
        metrics.average_response_time = Duration::ZERO;
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics> {
        let state = self.state.read().await;
        Ok(PerformanceMetrics {
            request_times: vec![100.0, 150.0, 120.0], // Example response times
            memory_usage: 1024 * 1024 * 100, // 100MB
            cpu_usage: 15.5,
            active_connections: state.sessions.len() as u32,
            queue_sizes: HashMap::new(),
            custom_metrics: HashMap::new(),
            timestamp: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl EventDriven for ExampleMcpGateway {
    type Event = McpGatewayEvent;

    async fn subscribe(&mut self, event_type: &str) -> ServiceResult<mpsc::UnboundedReceiver<Self::Event>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut state = self.state.write().await;
        state.subscribers.insert(event_type.to_string(), tx);
        Ok(rx)
    }

    async fn unsubscribe(&mut self, event_type: &str) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        state.subscribers.remove(event_type);
        Ok(())
    }

    async fn publish(&self, event: Self::Event) -> ServiceResult<()> {
        let state = self.state.read().await;
        if let Some(tx) = state.subscribers.get(&event.event_type) {
            let _ = tx.send(event);
        }
        Ok(())
    }

    async fn handle_event(&mut self, event: Self::Event) -> ServiceResult<()> {
        match event.event_type.as_str() {
            "session_created" => {
                log::info!("New session created: {:?}", event.data);
            }
            "tool_executed" => {
                log::info!("Tool executed: {:?}", event.data);
            }
            _ => {
                log::debug!("Unknown event type: {}", event.event_type);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ResourceManager for ExampleMcpGateway {
    async fn get_resource_usage(&self) -> ServiceResult<ResourceUsage> {
        let state = self.state.read().await;
        Ok(ResourceUsage {
            memory_bytes: 1024 * 1024 * 100, // 100MB
            cpu_percentage: 15.5,
            disk_bytes: 1024 * 1024 * 500, // 500MB
            network_bytes: 1024 * 1024 * 10, // 10MB
            open_files: 25,
            active_threads: 4,
            measured_at: chrono::Utc::now(),
        })
    }

    async fn set_limits(&mut self, limits: ResourceLimits) -> ServiceResult<()> {
        // Update configuration based on limits
        if let Some(max_concurrent) = limits.max_concurrent_operations {
            self.config.max_sessions = max_concurrent;
            self.config.max_executions = max_concurrent;
        }
        Ok(())
    }

    async fn get_limits(&self) -> ServiceResult<ResourceLimits> {
        Ok(ResourceLimits {
            max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
            max_cpu_percentage: Some(80.0),
            max_disk_bytes: Some(1024 * 1024 * 1024 * 10), // 10GB
            max_concurrent_operations: Some(self.config.max_sessions),
            max_queue_size: Some(1000),
            operation_timeout: Some(Duration::from_secs(30)),
        })
    }

    async fn cleanup_resources(&mut self) -> ServiceResult<()> {
        let mut state = self.state.write().await;

        // Clean up expired sessions
        let now = chrono::Utc::now();
        state.sessions.retain(|_, session| {
            now.signed_duration_since(session.last_activity).num_seconds() < self.config.session_timeout.as_secs() as i64
        });

        // Clean up completed executions
        state.executions.retain(|_, execution| {
            matches!(execution.status, ExecutionStatus::Running | ExecutionStatus::Pending)
        });

        Ok(())
    }
}

#[async_trait]
impl McpGateway for ExampleMcpGateway {
    type Config = McpGatewayConfig;
    type Event = McpGatewayEvent;

    async fn initialize_connection(&self, client_id: &str, capabilities: McpCapabilities) -> ServiceResult<McpSession> {
        self.validate_session_limits().await?;

        let session_id = self.generate_session_id();
        let session = McpSession {
            session_id: session_id.clone(),
            client_id: client_id.to_string(),
            status: McpSessionStatus::Active,
            server_capabilities: McpCapabilities {
                tools: Some(ToolCapabilities {
                    list_tools: Some(true),
                    call_tool: Some(true),
                    subscribe_to_tools: Some(false),
                }),
                resources: None,
                logging: None,
                sampling: None,
            },
            client_capabilities: capabilities,
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
        };

        let mut state = self.state.write().await;
        state.sessions.insert(session_id.clone(), session.clone());

        // Publish session created event
        drop(state);
        self.publish(McpGatewayEvent {
            event_type: "session_created".to_string(),
            data: serde_json::json!({
                "session_id": session_id,
                "client_id": client_id
            }),
            timestamp: chrono::Utc::now(),
        }).await?;

        Ok(session)
    }

    async fn close_connection(&self, session_id: &str) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        state.sessions.remove(session_id);

        // Cancel any active executions for this session
        state.executions.retain(|_, execution| {
            execution.session_id != session_id
        });

        Ok(())
    }

    async fn list_connections(&self) -> ServiceResult<Vec<McpSession>> {
        let state = self.state.read().await;
        Ok(state.sessions.values().cloned().collect())
    }

    async fn send_notification(&self, session_id: &str, notification: McpNotification) -> ServiceResult<()> {
        let state = self.state.read().await;
        if !state.sessions.contains_key(session_id) {
            return Err(ServiceError::tool_error(format!("Session {} not found", session_id)));
        }

        // In a real implementation, this would send the notification to the client
        log::info!("Sending notification to session {}: {:?}", session_id, notification);
        Ok(())
    }

    async fn handle_request(&self, session_id: &str, request: McpRequest) -> ServiceResult<McpResponse> {
        let start_time = Instant::now();
        let result = self.handle_request_internal(session_id, request).await;
        let duration = start_time.elapsed();

        self.record_metrics(result.is_ok(), duration).await;

        result
    }

    async fn register_tool(&mut self, tool: ToolDefinition) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        state.tools.insert(tool.name.clone(), tool);
        Ok(())
    }

    async fn unregister_tool(&mut self, tool_name: &str) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        state.tools.remove(tool_name);
        Ok(())
    }

    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>> {
        let state = self.state.read().await;
        Ok(state.tools.values().cloned().collect())
    }

    async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>> {
        let state = self.state.read().await;
        Ok(state.tools.get(name).cloned())
    }

    async fn update_tool(&mut self, tool: ToolDefinition) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        state.tools.insert(tool.name.clone(), tool);
        Ok(())
    }

    async fn execute_tool(&self, request: McpToolRequest) -> ServiceResult<McpToolResponse> {
        self.validate_execution_limits().await?;

        let execution_id = self.generate_execution_id();
        let start_time = Instant::now();

        let execution = ActiveExecution {
            execution_id: execution_id.clone(),
            tool_name: request.tool_name.clone(),
            session_id: request.session_id.clone(),
            status: ExecutionStatus::Running,
            started_at: chrono::Utc::now(),
            progress: Some(0.0),
        };

        let mut state = self.state.write().await;
        state.executions.insert(execution_id.clone(), execution);
        drop(state);

        // Simulate tool execution
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = match self.execute_tool_internal(&request).await {
            Ok(result) => result,
            Err(e) => {
                let mut state = self.state.write().await;
                if let Some(execution) = state.executions.get_mut(&execution_id) {
                    execution.status = ExecutionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        let duration = start_time.elapsed();

        let mut state = self.state.write().await;
        if let Some(execution) = state.executions.get_mut(&execution_id) {
            execution.status = ExecutionStatus::Completed;
            execution.progress = Some(100.0);
        }

        Ok(McpToolResponse {
            request_id: request.request_id,
            result: Some(result),
            error: None,
            execution_time: duration,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn cancel_execution(&self, execution_id: &str) -> ServiceResult<()> {
        let mut state = self.state.write().await;
        if let Some(execution) = state.executions.get_mut(execution_id) {
            execution.status = ExecutionStatus::Cancelled;
        }
        Ok(())
    }

    async fn get_execution_status(&self, execution_id: &str) -> ServiceResult<ExecutionStatus> {
        let state = self.state.read().await;
        state.executions
            .get(execution_id)
            .map(|execution| execution.status.clone())
            .ok_or_else(|| ServiceError::tool_error(format!("Execution {} not found", execution_id)))
    }

    async fn list_active_executions(&self) -> ServiceResult<Vec<ActiveExecution>> {
        let state = self.state.read().await;
        Ok(state.executions.values().cloned().collect())
    }

    async fn get_capabilities(&self) -> ServiceResult<McpCapabilities> {
        Ok(McpCapabilities {
            tools: Some(ToolCapabilities {
                list_tools: Some(true),
                call_tool: Some(true),
                subscribe_to_tools: Some(false),
            }),
            resources: None,
            logging: Some(LoggingCapabilities {
                set_log_level: Some(true),
                get_log_messages: Some(true),
            }),
            sampling: None,
        })
    }

    async fn set_capabilities(&mut self, capabilities: McpCapabilities) -> ServiceResult<()> {
        // In a real implementation, this would update the server capabilities
        log::info!("Updating MCP capabilities: {:?}", capabilities);
        Ok(())
    }

    async fn negotiate_capabilities(&self, client_capabilities: McpCapabilities) -> ServiceResult<McpCapabilities> {
        // In a real implementation, this would negotiate with the client
        Ok(client_capabilities)
    }

    async fn get_mcp_resources(&self) -> ServiceResult<McpResourceUsage> {
        let state = self.state.read().await;
        Ok(McpResourceUsage {
            active_sessions: state.sessions.len() as u32,
            active_executions: state.executions.len() as u32,
            registered_tools: state.tools.len() as u32,
            memory_usage: 1024 * 1024 * 50, // 50MB
            network_usage: 1024 * 1024 * 5, // 5MB
        })
    }

    async fn configure_protocol(&mut self, settings: McpProtocolSettings) -> ServiceResult<()> {
        // Update configuration based on protocol settings
        if let Some(max_sessions) = settings.max_sessions {
            self.config.max_sessions = max_sessions;
        }
        if let Some(timeout) = settings.session_timeout_seconds {
            self.config.session_timeout = Duration::from_secs(timeout);
        }
        Ok(())
    }
}

impl ExampleMcpGateway {
    /// Internal request handler
    async fn handle_request_internal(&self, session_id: &str, request: McpRequest) -> ServiceResult<McpResponse> {
        let state = self.state.read().await;
        if !state.sessions.contains_key(session_id) {
            return Err(ServiceError::tool_error(format!("Session {} not found", session_id)));
        }

        match request.method.as_str() {
            "tools/list" => {
                let tools: Vec<_> = state.tools.values().cloned().collect();
                Ok(McpResponse {
                    id: request.id.unwrap_or_default(),
                    result: Some(serde_json::json!({
                        "tools": tools
                    })),
                    error: None,
                    timestamp: chrono::Utc::now(),
                })
            }
            _ => Ok(McpResponse {
                id: request.id.unwrap_or_default(),
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
                timestamp: chrono::Utc::now(),
            }),
        }
    }

    /// Internal tool execution
    async fn execute_tool_internal(&self, request: &McpToolRequest) -> ServiceResult<serde_json::Value> {
        let state = self.state.read().await;

        let tool = state.tools.get(&request.tool_name)
            .ok_or_else(|| ServiceError::tool_error(format!("Tool {} not found", request.tool_name)))?;

        // In a real implementation, this would execute the actual tool
        // For this example, we'll just return a mock result
        Ok(serde_json::json!({
            "result": format!("Executed tool: {}", request.tool_name),
            "arguments": request.arguments
        }))
    }
}

/// ============================================================================
/// USAGE EXAMPLES
/// ============================================================================

/// Example of using the MCP Gateway service
pub async fn example_mcp_gateway_usage() -> ServiceResult<()> {
    // Create configuration
    let config = McpGatewayConfig {
        max_sessions: 100,
        session_timeout: Duration::from_secs(3600),
        max_executions: 50,
        execution_timeout: Duration::from_secs(30),
    };

    // Create and start the service
    let mut gateway = ExampleMcpGateway::new(config);
    gateway.start().await?;

    // Register a tool
    let tool = ToolDefinition {
        name: "echo".to_string(),
        description: "Echoes the input text".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"}
            },
            "required": ["text"]
        }),
        category: Some("utility".to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("example".to_string()),
        tags: vec!["text".to_string(), "echo".to_string()],
        enabled: true,
        parameters: vec![],
    };

    gateway.register_tool(tool).await?;

    // Initialize a connection
    let session = gateway.initialize_connection(
        "client_123",
        McpCapabilities {
            tools: Some(ToolCapabilities {
                list_tools: Some(true),
                call_tool: Some(true),
                subscribe_to_tools: Some(false),
            }),
            resources: None,
            logging: None,
            sampling: None,
        }
    ).await?;

    // Execute a tool
    let tool_request = McpToolRequest {
        tool_name: "echo".to_string(),
        arguments: {
            let mut args = HashMap::new();
            args.insert("text".to_string(), serde_json::Value::String("Hello, World!".to_string()));
            args
        },
        session_id: session.session_id.clone(),
        request_id: "req_123".to_string(),
        timeout_ms: Some(5000),
    };

    let response = gateway.execute_tool(tool_request).await?;
    println!("Tool execution result: {:?}", response);

    // Check health
    let health = gateway.health_check().await?;
    println!("Service health: {:?}", health);

    // Get metrics
    let metrics = gateway.get_metrics().await?;
    println!("Service metrics: {:?}", metrics);

    // Stop the service
    gateway.stop().await?;

    Ok(())
}

/// Example of service composition and coordination
pub async fn example_service_coordination() -> ServiceResult<()> {
    // This example shows how multiple services can work together
    // In a real implementation, you would have actual implementations of all services

    println!("=== Service Coordination Example ===");

    // 1. Start all services
    println!("Starting services...");

    let mcp_config = McpGatewayConfig {
        max_sessions: 50,
        session_timeout: Duration::from_secs(1800),
        max_executions: 25,
        execution_timeout: Duration::from_secs(15),
    };

    let mut mcp_gateway = ExampleMcpGateway::new(mcp_config);
    mcp_gateway.start().await?;

    // 2. Set up event communication between services
    println!("Setting up event communication...");

    let mut mcp_events = mcp_gateway.subscribe("tool_executed").await?;

    // 3. Coordinate service health monitoring
    println!("Monitoring service health...");

    let mcp_health = mcp_gateway.health_check().await?;
    println!("MCP Gateway Health: {:?}", mcp_health.status);

    // 4. Handle cross-service operations
    println!("Performing cross-service operations...");

    // Simulate a workflow that uses multiple services
    // - MCP Gateway receives a tool execution request
    // - Script Engine executes the tool script
    // - Inference Engine processes the results
    // - Data Store stores the execution record

    // 5. Cleanup
    println!("Cleaning up services...");
    mcp_gateway.stop().await?;

    println!("Service coordination completed successfully!");
    Ok(())
}

/// Example of resource management and monitoring
pub async fn example_resource_management() -> ServiceResult<()> {
    println!("=== Resource Management Example ===");

    let config = McpGatewayConfig {
        max_sessions: 10,
        session_timeout: Duration::from_secs(300),
        max_executions: 5,
        execution_timeout: Duration::from_secs(10),
    };

    let mut gateway = ExampleMcpGateway::new(config);
    gateway.start().await?;

    // Monitor resource usage
    let usage = gateway.get_resource_usage().await?;
    println!("Current resource usage:");
    println!("  Memory: {} MB", usage.memory_bytes / 1024 / 1024);
    println!("  CPU: {:.1}%", usage.cpu_percentage);
    println!("  Active sessions: {}", gateway.list_connections().await?.len());

    // Set resource limits
    let limits = ResourceLimits {
        max_memory_bytes: Some(1024 * 1024 * 500), // 500MB
        max_cpu_percentage: Some(75.0),
        max_concurrent_operations: Some(20),
        max_queue_size: Some(100),
        operation_timeout: Some(Duration::from_secs(60)),
        max_disk_bytes: None,
    };

    gateway.set_limits(limits).await?;
    println!("Resource limits updated");

    // Get performance metrics
    let perf_metrics = gateway.get_performance_metrics().await?;
    println!("Performance metrics:");
    println!("  Average response time: {:.1}ms",
             perf_metrics.request_times.iter().sum::<f64>() / perf_metrics.request_times.len() as f64);
    println!("  Active connections: {}", perf_metrics.active_connections);

    // Cleanup resources
    gateway.cleanup_resources().await?;
    println!("Resource cleanup completed");

    gateway.stop().await?;
    Ok(())
}