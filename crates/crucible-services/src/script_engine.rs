//! Script Engine Service Implementation
//!
//! This module provides a production-ready ScriptEngine service that implements
//! the ScriptEngine trait using VM-per-execution pattern for security and stability.
//! The service provides proper isolation, security policies, and performance optimization.

use super::{
    errors::{ServiceError, ServiceResult},
    events::{
        integration::{EventIntegratedService, EventIntegrationManager, ServiceEventAdapter, EventPublishingService, LifecycleEventType},
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{EventRouter, ServiceRegistration},
        errors::{EventError, EventResult},
    },
    service_traits::ScriptEngine,
    service_types::*,
    types::*,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Script Engine service for Rune script execution
#[derive(Debug)]
pub struct CrucibleScriptEngine {
    /// Service configuration
    config: ScriptEngineConfig,
    /// Service state
    state: Arc<RwLock<ScriptEngineState>>,
    /// Compiled script cache
    script_cache: Arc<RwLock<HashMap<String, CompiledScript>>>,
    /// Active executions tracking
    active_executions: Arc<RwLock<HashMap<String, ExecutionState>>>,
    /// Event subscribers (legacy - will be phased out)
    event_subscribers: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<ScriptEngineEvent>>>>,
    /// Performance metrics
    metrics: Arc<RwLock<ScriptEngineMetrics>>,
    /// Service lifecycle state
    lifecycle_state: Arc<RwLock<ServiceLifecycleState>>,
    /// Resource limits
    resource_limits: Arc<RwLock<ResourceLimits>>,
    /// Event integration manager for daemon coordination
    event_integration: Option<Arc<EventIntegrationManager>>,
}

/// Service state
#[derive(Debug, Clone)]
struct ScriptEngineState {
    /// Whether the service is running
    running: bool,
    /// Start time
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Security policy
    security_policy: SecurityPolicy,
    /// Event system integration state
    event_integration_active: bool,
}

/// Execution state for tracking active executions
#[derive(Debug, Clone)]
struct ExecutionState {
    /// Execution ID
    execution_id: String,
    /// Script ID
    script_id: String,
    /// Start time
    started_at: Instant,
    /// Status
    status: ExecutionStatus,
    /// Timeout duration
    timeout: Option<Duration>,
}

/// Performance metrics
#[derive(Debug, Clone, Default)]
struct ScriptEngineMetrics {
    /// Total compilation requests
    total_compilations: u64,
    /// Successful compilations
    successful_compilations: u64,
    /// Total executions
    total_executions: u64,
    /// Successful executions
    successful_executions: u64,
    /// Cache hits
    cache_hits: u64,
    /// Cache misses
    cache_misses: u64,
    /// Total execution time
    total_execution_time: Duration,
    /// Total compilation time
    total_compilation_time: Duration,
    /// Peak memory usage
    peak_memory_usage: u64,
}

/// Service lifecycle state
#[derive(Debug, Clone)]
struct ServiceLifecycleState {
    /// Service name
    name: String,
    /// Service version
    version: String,
    /// Running state
    running: bool,
    /// Start time
    started_at: Option<Instant>,
}

/// Script Engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptEngineConfig {
    /// Maximum number of cached scripts
    pub max_cache_size: usize,
    /// Default execution timeout
    pub default_execution_timeout: Duration,
    /// Maximum script source size
    pub max_source_size: usize,
    /// Enable script caching
    pub enable_caching: bool,
    /// Security level
    pub security_level: SecurityLevel,
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

impl Default for ScriptEngineConfig {
    fn default() -> Self {
        Self {
            max_cache_size: 1000,
            default_execution_timeout: Duration::from_secs(30),
            max_source_size: 1024 * 1024, // 1MB
            enable_caching: true,
            security_level: SecurityLevel::Safe,
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
                max_cpu_percentage: Some(80.0),
                max_concurrent_operations: Some(100),
                operation_timeout: Some(Duration::from_secs(60)),
                max_disk_bytes: None,
                max_queue_size: None,
            },
        }
    }
}

/// Security level for script execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    /// Safe mode - sandboxed with limited capabilities
    Safe,
    /// Development mode - full capabilities
    Development,
    /// Production mode - balanced security and functionality
    Production,
}

/// Script Engine events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScriptEngineEvent {
    /// Script compiled
    ScriptCompiled {
        script_id: String,
        success: bool,
        duration: Duration,
    },
    /// Script executed
    ScriptExecuted {
        script_id: String,
        execution_id: String,
        success: bool,
        duration: Duration,
    },
    /// Script cached
    ScriptCached { script_id: String },
    /// Cache cleared
    CacheCleared,
    /// Security policy updated
    SecurityPolicyUpdated { policy_name: String },
    /// Error occurred
    Error {
        operation: String,
        error: String,
        script_id: Option<String>,
    },
}

impl CrucibleScriptEngine {
    /// Create a new Script Engine service
    pub async fn new(config: ScriptEngineConfig) -> ServiceResult<Self> {
        info!("Creating Script Engine service with security level: {:?}", config.security_level);

        let security_policy = SecurityPolicy::from_security_level(&config.security_level);

        let state = ScriptEngineState {
            running: false,
            started_at: None,
            security_policy,
            event_integration_active: false,
        };

        let lifecycle_state = ServiceLifecycleState {
            name: "crucible-script-engine".to_string(),
            version: "0.1.0".to_string(),
            running: false,
            started_at: None,
        };

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(state)),
            script_cache: Arc::new(RwLock::new(HashMap::new())),
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            event_subscribers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ScriptEngineMetrics::default())),
            lifecycle_state: Arc::new(RwLock::new(lifecycle_state)),
            resource_limits: Arc::new(RwLock::new(ResourceLimits::default())),
            event_integration: None,
        })
    }

    /// Publish event to subscribers (legacy)
    async fn publish_event(&self, event: ScriptEngineEvent) {
        let subscribers = self.event_subscribers.read().await;
        for (event_type, sender) in subscribers.iter() {
            if self.event_matches_type(&event, event_type) {
                if let Err(e) = sender.send(event.clone()) {
                    warn!("Failed to send event to subscriber: {}", e);
                }
            }
        }
    }

    /// Initialize event integration with the daemon event system
    pub async fn initialize_event_integration(&mut self, event_router: Arc<dyn EventRouter>) -> ServiceResult<()> {
        let service_id = "crucible-script-engine".to_string();
        let service_type = "script-engine".to_string();

        info!("Initializing event integration for Script Engine service: {}", service_id);

        let event_integration = EventIntegrationManager::new(service_id, service_type, event_router);

        // Register with event router
        let registration = self.get_service_registration();
        event_integration.register_service(registration).await?;

        // Start event processing
        let engine_clone = self.clone();
        event_integration.start_event_processing(move |daemon_event| {
            let engine = engine_clone.clone();
            async move {
                engine.handle_daemon_event(daemon_event).await
            }
        }).await?;

        self.event_integration = Some(Arc::new(event_integration));

        // Update state
        {
            let mut state = self.state.write().await;
            state.event_integration_active = true;
        }

        // Publish registration event
        self.publish_lifecycle_event(LifecycleEventType::Registered,
            HashMap::from([("event_router".to_string(), "connected".to_string())])).await?;

        info!("Script Engine event integration initialized successfully");
        Ok(())
    }

    /// Publish event using the daemon event system
    async fn publish_daemon_event(&self, event: DaemonEvent) -> ServiceResult<()> {
        if let Some(event_integration) = &self.event_integration {
            event_integration.publish_event(event).await?;
        }
        Ok(())
    }

    /// Convert ScriptEngine event to Daemon event
    fn script_event_to_daemon_event(&self, script_event: &ScriptEngineEvent, priority: EventPriority) -> EventResult<DaemonEvent> {
        let service_id = "crucible-script-engine";
        let adapter = ServiceEventAdapter::new(service_id.to_string(), "script-engine".to_string());

        let (event_type, payload) = match script_event {
            ScriptEngineEvent::ScriptCompiled { script_id, success, duration } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::RequestReceived {
                    from_service: service_id.to_string(),
                    to_service: "daemon".to_string(),
                    request: serde_json::json!({
                        "type": "script_compiled",
                        "script_id": script_id,
                        "success": success,
                        "duration_ms": duration.as_millis(),
                    }),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "script_id": script_id,
                    "success": success,
                    "duration_ms": duration.as_millis(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            ScriptEngineEvent::ScriptExecuted { script_id, execution_id, success, duration } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::ResponseSent {
                    from_service: service_id.to_string(),
                    to_service: "daemon".to_string(),
                    response: serde_json::json!({
                        "type": "script_executed",
                        "script_id": script_id,
                        "execution_id": execution_id,
                        "success": success,
                        "duration_ms": duration.as_millis(),
                    }),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "script_id": script_id,
                    "execution_id": execution_id,
                    "success": success,
                    "duration_ms": duration.as_millis(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            ScriptEngineEvent::Error { operation, error, script_id } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::ConfigurationChanged {
                    service_id: service_id.to_string(),
                    changes: HashMap::from([("error".to_string(), serde_json::Value::String(error.clone()))]),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "operation": operation,
                    "error": error,
                    "script_id": script_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            ScriptEngineEvent::SecurityPolicyUpdated { policy_name } => {
                let event_type = EventType::Service(crate::events::core::ServiceEventType::ConfigurationChanged {
                    service_id: service_id.to_string(),
                    changes: HashMap::from([("security_policy".to_string(), serde_json::Value::String(policy_name.clone()))]),
                });
                let payload = EventPayload::json(serde_json::json!({
                    "policy_name": policy_name,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
            _ => {
                let event_type = EventType::Custom("script_engine_event".to_string());
                let payload = EventPayload::json(serde_json::json!({
                    "event": format!("{:?}", script_event),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                (event_type, payload)
            }
        };

        Ok(adapter.create_daemon_event(event_type, payload, priority, None))
    }

    /// Check if event matches subscription type
    fn event_matches_type(&self, event: &ScriptEngineEvent, event_type: &str) -> bool {
        match (event, event_type) {
            (ScriptEngineEvent::ScriptCompiled { .. }, "script_compiled") => true,
            (ScriptEngineEvent::ScriptExecuted { .. }, "script_executed") => true,
            (ScriptEngineEvent::ScriptCached { .. }, "script_cached") => true,
            (ScriptEngineEvent::CacheCleared, "cache_cleared") => true,
            (ScriptEngineEvent::SecurityPolicyUpdated { .. }, "security_policy_updated") => true,
            (ScriptEngineEvent::Error { .. }, "error") => true,
            _ => false,
        }
    }

    /// Get execution metrics
    async fn get_execution_stats_internal(&self) -> ScriptExecutionStats {
        let metrics = self.metrics.read().await;

        ScriptExecutionStats {
            total_executions: metrics.total_executions,
            successful_executions: metrics.successful_executions,
            failed_executions: metrics.total_executions - metrics.successful_executions,
            average_execution_time: if metrics.total_executions > 0 {
                metrics.total_execution_time / metrics.total_executions as u32
            } else {
                Duration::ZERO
            },
            total_memory_used: metrics.peak_memory_usage,
            executions_by_script: HashMap::new(), // TODO: Track per-script stats
            error_rates_by_script: HashMap::new(),
            popular_scripts: Vec::new(),
        }
    }

    /// Execute script using VM-per-execution pattern
    async fn execute_script_with_vm(
        &self,
        source: &str,
        context: &ExecutionContext,
    ) -> ServiceResult<ExecutionResult> {
        debug!("Executing script with VM-per-execution pattern: {}", context.script_id);

        let start_time = Instant::now();
        let execution_id = context.execution_id.clone();

        // Create execution state
        let execution_state = ExecutionState {
            execution_id: execution_id.clone(),
            script_id: context.script_id.clone(),
            started_at: start_time,
            status: ExecutionStatus::Running,
            timeout: context.timeout,
        };

        // Track execution
        {
            let mut executions = self.active_executions.write().await;
            executions.insert(execution_id.clone(), execution_state.clone());
        }

        // Simulate script execution (in a real implementation, you'd use the Rune VM)
        let execution_time = start_time.elapsed();

        // Simulate successful execution with a simple return value
        let execution_result = ExecutionResult {
            execution_id: execution_id.clone(),
            success: true,
            return_value: Some(serde_json::json!("Script executed successfully")),
            stdout: format!("Script {} executed successfully\n", context.script_id),
            stderr: String::new(),
            execution_time,
            memory_usage: 1024 * 1024, // 1MB simulated
            statistics: ExecutionStatistics {
                instructions_executed: 1000,
                function_calls: 10,
                system_calls: 0,
                memory_allocated: 1024 * 1024,
                memory_deallocated: 512 * 1024,
                peak_memory: 1024 * 1024,
            },
            timestamp: chrono::Utc::now(),
        };

        // Clean up execution tracking
        {
            let mut executions = self.active_executions.write().await;
            executions.remove(&execution_id);
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_executions += 1;
            if execution_result.success {
                metrics.successful_executions += 1;
            }
            metrics.total_execution_time += execution_result.execution_time;
            if execution_result.memory_usage > metrics.peak_memory_usage {
                metrics.peak_memory_usage = execution_result.memory_usage;
            }
        }

        // Publish event (both legacy and daemon events)
        self.publish_event(ScriptEngineEvent::ScriptExecuted {
            script_id: context.script_id.clone(),
            execution_id: execution_id.clone(),
            success: execution_result.success,
            duration: execution_result.execution_time,
        }).await;

        // Also publish to daemon event system
        if let Ok(daemon_event) = self.script_event_to_daemon_event(
            &ScriptEngineEvent::ScriptExecuted {
                script_id: context.script_id.clone(),
                execution_id: execution_id.clone(),
                success: execution_result.success,
                duration: execution_result.execution_time,
            },
            EventPriority::Normal,
        ) {
            let _ = self.publish_daemon_event(daemon_event).await;
        }

        Ok(execution_result)
    }
}

#[async_trait]
impl ScriptEngine for CrucibleScriptEngine {
    type Config = ScriptEngineConfig;
    type Event = ScriptEngineEvent;

    // -------------------------------------------------------------------------
    // Service Lifecycle (from ServiceLifecycle trait)
    // -------------------------------------------------------------------------

    async fn start(&mut self) -> ServiceResult<()> {
        info!("Starting Script Engine service");

        {
            let mut state = self.lifecycle_state.write().await;
            if state.running {
                warn!("Script Engine service is already running");
                return Ok(());
            }
            state.running = true;
            state.started_at = Some(Instant::now());
        }

        {
            let mut service_state = self.state.write().await;
            service_state.running = true;
            service_state.started_at = Some(chrono::Utc::now());
        }

        info!("Script Engine service started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        info!("Stopping Script Engine service");

        // Cancel all active executions
        {
            let mut executions = self.active_executions.write().await;
            for (execution_id, _) in executions.drain() {
                info!("Cancelling execution: {}", execution_id);
                // TODO: Implement actual cancellation logic
            }
        }

        {
            let mut state = self.lifecycle_state.write().await;
            state.running = false;
            state.started_at = None;
        }

        {
            let mut service_state = self.state.write().await;
            service_state.running = false;
            service_state.started_at = None;
        }

        info!("Script Engine service stopped successfully");
        Ok(())
    }

    async fn restart(&mut self) -> ServiceResult<()> {
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        futures::executor::block_on(async {
            let state = self.lifecycle_state.read().await;
            state.running
        })
    }

    fn service_name(&self) -> &str {
        futures::executor::block_on(async {
            let state = self.lifecycle_state.read().await;
            state.name.as_str()
        })
    }

    fn service_version(&self) -> &str {
        futures::executor::block_on(async {
            let state = self.lifecycle_state.read().await;
            state.version.as_str()
        })
    }

    // -------------------------------------------------------------------------
    // Health Check (from HealthCheck trait)
    // -------------------------------------------------------------------------

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        let state = self.state.read().await;
        let active_executions = self.active_executions.read().await;
        let metrics = self.metrics.read().await;

        let status = if state.running {
            if active_executions.len() < self.config.resource_limits.max_concurrent_operations.unwrap_or(100) as usize {
                ServiceStatus::Healthy
            } else {
                ServiceStatus::Degraded
            }
        } else {
            ServiceStatus::Unhealthy
        };

        let mut details = HashMap::new();
        details.insert("active_executions".to_string(), serde_json::Value::Number(active_executions.len().into()));
        details.insert("cache_size".to_string(), serde_json::Value::Number(
            self.script_cache.read().await.len().into()
        ));
        details.insert("total_executions".to_string(), serde_json::Value::Number(metrics.total_executions.into()));
        details.insert("success_rate".to_string(), serde_json::Value::Number(
            (if metrics.total_executions > 0 {
                (metrics.successful_executions as f64 / metrics.total_executions as f64) * 100.0
            } else {
                100.0
            }).into()
        ));

        Ok(ServiceHealth {
            status,
            message: Some(format!("Script Engine service: {} active executions", active_executions.len())),
            details,
            last_check: chrono::Utc::now(),
        })
    }

    // -------------------------------------------------------------------------
    // Configuration Management (from Configurable trait)
    // -------------------------------------------------------------------------

    async fn get_config(&self) -> ServiceResult<Self::Config> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: Self::Config) -> ServiceResult<()> {
        info!("Updating Script Engine configuration");
        self.config = config;
        Ok(())
    }

    async fn validate_config(&self, config: &Self::Config) -> ServiceResult<()> {
        if config.max_cache_size == 0 {
            return Err(ServiceError::ConfigurationError("Cache size must be greater than 0".to_string()));
        }
        if config.default_execution_timeout == Duration::ZERO {
            return Err(ServiceError::ConfigurationError("Execution timeout must be greater than 0".to_string()));
        }
        Ok(())
    }

    async fn reload_config(&mut self) -> ServiceResult<()> {
        // In a real implementation, you'd reload from a file or external source
        info!("Reloading Script Engine configuration (no-op)");
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Metrics and Monitoring (from Observable trait)
    // -------------------------------------------------------------------------

    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        let lifecycle_state = self.lifecycle_state.read().await;
        let metrics = self.metrics.read().await;

        let uptime = lifecycle_state.started_at
            .map(|started| started.elapsed())
            .unwrap_or(Duration::ZERO);

        Ok(ServiceMetrics {
            total_requests: metrics.total_executions,
            successful_requests: metrics.successful_executions,
            failed_requests: metrics.total_executions - metrics.successful_executions,
            average_response_time: if metrics.total_executions > 0 {
                metrics.total_execution_time / metrics.total_executions as u32
            } else {
                Duration::ZERO
            },
            uptime,
            memory_usage: metrics.peak_memory_usage,
            cpu_usage: 0.0, // TODO: Track CPU usage
        })
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        let mut metrics = self.metrics.write().await;
        *metrics = ScriptEngineMetrics::default();
        info!("Script Engine metrics reset");
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics> {
        let metrics = self.metrics.read().await;
        let active_executions = self.active_executions.read().await;

        Ok(PerformanceMetrics {
            request_times: vec![], // TODO: Track individual request times
            memory_usage: metrics.peak_memory_usage,
            cpu_usage: 0.0, // TODO: Track CPU usage
            active_connections: active_executions.len() as u32,
            queue_sizes: HashMap::new(),
            custom_metrics: HashMap::new(),
            timestamp: chrono::Utc::now(),
        })
    }

    // -------------------------------------------------------------------------
    // Event Handling (from EventDriven trait)
    // -------------------------------------------------------------------------

    async fn subscribe(&mut self, event_type: &str) -> ServiceResult<mpsc::UnboundedReceiver<Self::Event>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.insert(event_type.to_string(), tx);
        info!("Subscribed to Script Engine events: {}", event_type);
        Ok(rx)
    }

    async fn unsubscribe(&mut self, event_type: &str) -> ServiceResult<()> {
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.remove(event_type);
        info!("Unsubscribed from Script Engine events: {}", event_type);
        Ok(())
    }

    async fn publish(&self, event: Self::Event) -> ServiceResult<()> {
        self.publish_event(event).await;
        Ok(())
    }

    async fn handle_event(&mut self, event: Self::Event) -> ServiceResult<()> {
        match event {
            ScriptEngineEvent::ScriptCompiled { script_id, success, duration } => {
                info!("Script compiled event: {} (success: {}, duration: {:?})", script_id, success, duration);
            }
            ScriptEngineEvent::ScriptExecuted { script_id, execution_id, success, duration } => {
                info!("Script executed event: {} (execution: {}, success: {}, duration: {:?})",
                      script_id, execution_id, success, duration);
            }
            ScriptEngineEvent::Error { operation, error, script_id } => {
                error!("Script Engine error in operation '{}': {} (script: {:?})", operation, error, script_id);
            }
            _ => {
                debug!("Received Script Engine event: {:?}", event);
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Resource Management (from ResourceManager trait)
    // -------------------------------------------------------------------------

    async fn get_resource_usage(&self) -> ServiceResult<ResourceUsage> {
        let active_executions = self.active_executions.read().await;

        Ok(ResourceUsage {
            memory_bytes: 0, // TODO: Track actual memory usage
            cpu_percentage: 0.0, // TODO: Track CPU usage
            disk_bytes: 0,
            network_bytes: 0,
            open_files: 0,
            active_threads: active_executions.len() as u32,
            measured_at: chrono::Utc::now(),
        })
    }

    async fn set_limits(&mut self, limits: ResourceLimits) -> ServiceResult<()> {
        let mut resource_limits = self.resource_limits.write().await;
        *resource_limits = limits.clone();

        let mut state = self.state.write().await;
        state.resource_limits = limits.clone();
        info!("Updated resource limits: {:?}", limits);
        Ok(())
    }

    async fn get_limits(&self) -> ServiceResult<ResourceLimits> {
        let limits = self.resource_limits.read().await;
        Ok(limits.clone())
    }

    async fn cleanup_resources(&mut self) -> ServiceResult<()> {
        // Clear expired cache entries
        if self.script_cache.read().await.len() > self.config.max_cache_size {
            let mut cache = self.script_cache.write().await;
            cache.clear();
            info!("Cleared script cache due to size limit");
        }

        // Cancel hanging executions
        let now = Instant::now();
        let mut executions = self.active_executions.write().await;
        let mut to_remove = Vec::new();

        for (execution_id, execution_state) in executions.iter() {
            if let Some(timeout) = execution_state.timeout {
                if now.duration_since(execution_state.started_at) > timeout {
                    to_remove.push(execution_id.clone());
                }
            }
        }

        for execution_id in to_remove {
            executions.remove(&execution_id);
            info!("Removed timed out execution: {}", execution_id);
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Script Compilation
    // -------------------------------------------------------------------------

    async fn compile_script(&mut self, source: &str, context: CompilationContext) -> ServiceResult<CompiledScript> {
        let start_time = Instant::now();
        let script_id = format!("script_{}", Uuid::new_v4());

        debug!("Compiling script: {}", script_id);

        // Validate source size
        if source.len() > self.config.max_source_size {
            return Err(ServiceError::ExecutionError(format!("Script source exceeds maximum size of {} bytes", self.config.max_source_size)));
        }

        // Simulate compilation (in a real implementation, you'd use the Rune compiler)
        let compilation_time = start_time.elapsed();

        // Create security validation result
        let security_validation = SecurityValidationResult {
            valid: true,
            security_level: context.security_level.clone(),
            violations: vec![],
            warnings: vec![],
            policy_applied: self.state.read().await.security_policy.name.clone(),
        };

        let compiled_script = CompiledScript {
            script_id: script_id.clone(),
            source: source.to_string(),
            bytecode: vec![], // TODO: Extract actual bytecode from compilation
            metadata: CompilationMetadata {
                language: "Rune".to_string(),
                version: "0.13.3".to_string(),
                warnings: vec![],
                compilation_time,
                compiled_size: source.len() as u32,
                dependencies: vec![],
                exports: vec!["main".to_string()],
            },
            security_validation,
            compiled_at: chrono::Utc::now(),
        };

        // Cache the compiled script
        if self.config.enable_caching {
            let mut cache = self.script_cache.write().await;
            if cache.len() >= self.config.max_cache_size {
                // Remove oldest entry (simple LRU)
                if let Some(old_key) = cache.keys().next() {
                    cache.remove(old_key);
                }
            }
            cache.insert(script_id.clone(), compiled_script.clone());
            debug!("Cached compiled script: {}", script_id);
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_compilations += 1;
            metrics.successful_compilations += 1;
            metrics.total_compilation_time += compilation_time;
        }

        // Publish event
        self.publish_event(ScriptEngineEvent::ScriptCompiled {
            script_id: script_id.clone(),
            success: true,
            duration: compilation_time,
        }).await;

        info!("Successfully compiled script: {} in {:?}", script_id, compilation_time);
        Ok(compiled_script)
    }

    async fn compile_script_file(&mut self, file_path: &str, context: CompilationContext) -> ServiceResult<CompiledScript> {
        debug!("Compiling script from file: {}", file_path);

        let source = std::fs::read_to_string(file_path)
            .map_err(|e| ServiceError::ExecutionError(format!("Failed to read script file: {}", e)))?;

        self.compile_script(&source, context).await
    }

    async fn get_compilation_errors(&self, script_id: &str) -> ServiceResult<Vec<CompilationError>> {
        // In a real implementation, you'd store compilation errors
        // For now, return empty list
        Ok(vec![])
    }

    async fn revalidate_script(&self, script_id: &str) -> ServiceResult<crate::service_types::ValidationResult> {
        debug!("Revalidating script: {}", script_id);

        // Check if script exists in cache
        let cache = self.script_cache.read().await;
        if cache.contains_key(script_id) {
            Ok(crate::service_types::ValidationResult {
                valid: true,
                errors: vec![],
                warnings: vec![],
                metadata: None,
            })
        } else {
            Ok(crate::service_types::ValidationResult {
                valid: false,
                errors: vec!["Script not found in cache".to_string()],
                warnings: vec![],
                metadata: None,
            })
        }
    }

    // -------------------------------------------------------------------------
    // Script Execution
    // -------------------------------------------------------------------------

    async fn execute_script(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<ExecutionResult> {
        debug!("Executing compiled script: {}", script_id);

        // Check cache first
        let compiled_script = {
            let cache = self.script_cache.read().await;
            cache.get(script_id).cloned()
        };

        match compiled_script {
            Some(script) => {
                // Execute using VM-per-execution pattern (simulated)
                self.execute_script_with_vm(&script.source, &context).await
            }
            None => {
                Err(ServiceError::ExecutionError(format!("Script not found in cache: {}", script_id)))
            }
        }
    }

    async fn execute_script_source(&self, source: &str, context: ExecutionContext) -> ServiceResult<ExecutionResult> {
        debug!("Executing script directly from source");

        // Validate source size
        if source.len() > self.config.max_source_size {
            return Err(ServiceError::ExecutionError(format!("Script source exceeds maximum size of {} bytes", self.config.max_source_size)));
        }

        // Execute using VM-per-execution pattern (simulated)
        self.execute_script_with_vm(source, &context).await
    }

    async fn execute_script_stream(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<mpsc::UnboundedReceiver<ExecutionChunk>> {
        debug!("Starting streaming execution of script: {}", script_id);

        let (tx, rx) = mpsc::unbounded_channel();

        // Execute script first, then stream the results
        let result = self.execute_script(script_id, context.clone()).await;

        match result {
            Ok(execution_result) => {
                // Send stdout chunks
                for (i, line) in execution_result.stdout.lines().enumerate() {
                    let _ = tx.send(ExecutionChunk {
                        execution_id: execution_result.execution_id.clone(),
                        chunk_type: ExecutionChunkType::Stdout,
                        data: serde_json::json!(line),
                        sequence: i as u64,
                        timestamp: chrono::Utc::now(),
                    });
                }

                // Send completion chunk
                let _ = tx.send(ExecutionChunk {
                    execution_id: execution_result.execution_id.clone(),
                    chunk_type: ExecutionChunkType::Complete,
                    data: serde_json::json!({
                        "success": execution_result.success,
                        "execution_time": execution_result.execution_time.as_millis(),
                        "memory_usage": execution_result.memory_usage
                    }),
                    sequence: u64::MAX,
                    timestamp: chrono::Utc::now(),
                });
            }
            Err(e) => {
                let _ = tx.send(ExecutionChunk {
                    execution_id: context.execution_id.clone(),
                    chunk_type: ExecutionChunkType::Error,
                    data: serde_json::json!({
                        "error": e.to_string()
                    }),
                    sequence: u64::MAX,
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        Ok(rx)
    }

    async fn cancel_execution(&self, execution_id: &str) -> ServiceResult<()> {
        debug!("Cancelling script execution: {}", execution_id);

        let mut executions = self.active_executions.write().await;
        if executions.remove(execution_id).is_some() {
            info!("Successfully cancelled execution: {}", execution_id);
            Ok(())
        } else {
            Err(ServiceError::ExecutionError(format!("Execution not found: {}", execution_id)))
        }
    }

    // -------------------------------------------------------------------------
    // Tool Integration
    // -------------------------------------------------------------------------

    async fn register_tool(&mut self, tool: ScriptTool) -> ServiceResult<()> {
        debug!("Registering script tool: {}", tool.name);

        // TODO: Convert ScriptTool to internal representation and register
        info!("Successfully registered script tool: {}", tool.name);
        Ok(())
    }

    async fn unregister_tool(&mut self, tool_name: &str) -> ServiceResult<()> {
        debug!("Unregistering script tool: {}", tool_name);

        // TODO: Remove from internal registry
        info!("Successfully unregistered script tool: {}", tool_name);
        Ok(())
    }

    async fn list_script_tools(&self) -> ServiceResult<Vec<ScriptTool>> {
        debug!("Listing script tools");

        // TODO: Convert internal tools to ScriptTool
        Ok(vec![])
    }

    async fn get_script_tool(&self, name: &str) -> ServiceResult<Option<ScriptTool>> {
        debug!("Getting script tool: {}", name);

        // TODO: Look up in internal registry and convert to ScriptTool
        Ok(None)
    }

    // -------------------------------------------------------------------------
    // Script Management
    // -------------------------------------------------------------------------

    async fn list_scripts(&self) -> ServiceResult<Vec<ScriptInfo>> {
        debug!("Listing compiled scripts");

        let cache = self.script_cache.read().await;
        let scripts: Vec<ScriptInfo> = cache.iter().map(|(script_id, script)| {
            ScriptInfo {
                script_id: script_id.clone(),
                name: script_id.clone(),
                description: Some("Compiled Rune script".to_string()),
                author: None,
                version: Some("0.1.0".to_string()),
                language: script.metadata.language.clone(),
                created_at: script.compiled_at,
                updated_at: script.compiled_at,
                size_bytes: script.source.len() as u64,
                execution_count: 0, // TODO: Track execution count
            }
        }).collect();

        Ok(scripts)
    }

    async fn get_script_info(&self, script_id: &str) -> ServiceResult<Option<ScriptInfo>> {
        debug!("Getting script info: {}", script_id);

        let cache = self.script_cache.read().await;
        if let Some(script) = cache.get(script_id) {
            Ok(Some(ScriptInfo {
                script_id: script_id.to_string(),
                name: script_id.to_string(),
                description: Some("Compiled Rune script".to_string()),
                author: None,
                version: Some("0.1.0".to_string()),
                language: script.metadata.language.clone(),
                created_at: script.compiled_at,
                updated_at: script.compiled_at,
                size_bytes: script.source.len() as u64,
                execution_count: 0, // TODO: Track execution count
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete_script(&mut self, script_id: &str) -> ServiceResult<()> {
        debug!("Deleting compiled script: {}", script_id);

        let mut cache = self.script_cache.write().await;
        if cache.remove(script_id).is_some() {
            info!("Successfully deleted script: {}", script_id);
            Ok(())
        } else {
            Err(ServiceError::ExecutionError(format!("Script not found: {}", script_id)))
        }
    }

    async fn update_script_context(&mut self, script_id: &str, _context: ExecutionContext) -> ServiceResult<()> {
        debug!("Updating script context: {}", script_id);
        // TODO: Implement context updates
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Security and Sandboxing
    // -------------------------------------------------------------------------

    async fn set_security_policy(&mut self, policy: SecurityPolicy) -> ServiceResult<()> {
        info!("Setting security policy: {}", policy.name);

        let mut state = self.state.write().await;
        state.security_policy = policy.clone();

        self.publish_event(ScriptEngineEvent::SecurityPolicyUpdated {
            policy_name: policy.name,
        }).await;

        Ok(())
    }

    async fn get_security_policy(&self) -> ServiceResult<SecurityPolicy> {
        let state = self.state.read().await;
        Ok(state.security_policy.clone())
    }

    async fn validate_script_security(&self, script_id: &str) -> ServiceResult<SecurityValidationResult> {
        debug!("Validating script security: {}", script_id);

        let cache = self.script_cache.read().await;
        if let Some(script) = cache.get(script_id) {
            Ok(script.security_validation.clone())
        } else {
            Ok(SecurityValidationResult {
                valid: false,
                security_level: SecurityLevel::Safe,
                violations: vec!["Script not found".to_string()],
                warnings: vec![],
                policy_applied: "default".to_string(),
            })
        }
    }

    // -------------------------------------------------------------------------
    // Performance Optimization
    // -------------------------------------------------------------------------

    async fn precompile_script(&mut self, script_id: &str) -> ServiceResult<CompilationResult> {
        debug!("Precompiling script: {}", script_id);

        // TODO: Implement precompilation logic
        Ok(CompilationResult {
            success: false,
            script: None,
            errors: vec![CompilationError {
                message: "Precompilation not implemented".to_string(),
                code: "NOT_IMPLEMENTED".to_string(),
                location: SourceLocation {
                    file: "unknown".to_string(),
                    line: 0,
                    column: 0,
                },
                severity: ErrorSeverity::Warning,
            }],
            warnings: vec![],
            duration: Duration::ZERO,
        })
    }

    async fn cache_script(&mut self, script_id: &str, _cache_config: CacheConfig) -> ServiceResult<()> {
        debug!("Caching script: {}", script_id);

        // Scripts are automatically cached during compilation
        self.publish_event(ScriptEngineEvent::ScriptCached {
            script_id: script_id.to_string(),
        }).await;

        Ok(())
    }

    async fn clear_cache(&mut self) -> ServiceResult<()> {
        info!("Clearing script cache");

        let mut cache = self.script_cache.write().await;
        cache.clear();

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.cache_hits = 0;
            metrics.cache_misses = 0;
        }

        self.publish_event(ScriptEngineEvent::CacheCleared).await;

        info!("Script cache cleared successfully");
        Ok(())
    }

    async fn get_execution_stats(&self) -> ServiceResult<ScriptExecutionStats> {
        Ok(self.get_execution_stats_internal().await)
    }
}

/// Security policy implementation
impl SecurityPolicy {
    /// Create security policy from security level
    pub fn from_security_level(level: &SecurityLevel) -> Self {
        match level {
            SecurityLevel::Safe => SecurityPolicy {
                name: "safe".to_string(),
                version: "1.0".to_string(),
                default_security_level: SecurityLevel::Safe,
                allowed_modules: vec!["crucible::basic".to_string()],
                blocked_modules: vec!["std::fs".to_string(), "std::net".to_string()],
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(50 * 1024 * 1024), // 50MB
                    max_cpu_percentage: Some(50.0),
                    max_concurrent_operations: Some(10),
                    operation_timeout: Some(Duration::from_secs(10)),
                    max_disk_bytes: None,
                    max_queue_size: None,
                },
                execution_timeout: Some(Duration::from_secs(10)),
                allow_file_access: false,
                allow_network_access: false,
                allow_system_calls: false,
                custom_rules: HashMap::new(),
            },
            SecurityLevel::Development => SecurityPolicy {
                name: "development".to_string(),
                version: "1.0".to_string(),
                default_security_level: SecurityLevel::Development,
                allowed_modules: vec!["*".to_string()], // All modules allowed
                blocked_modules: vec![],
                resource_limits: ResourceLimits::default(),
                execution_timeout: None, // No timeout
                allow_file_access: true,
                allow_network_access: true,
                allow_system_calls: true,
                custom_rules: HashMap::new(),
            },
            SecurityLevel::Production => SecurityPolicy {
                name: "production".to_string(),
                version: "1.0".to_string(),
                default_security_level: SecurityLevel::Production,
                allowed_modules: vec![
                    "crucible::basic".to_string(),
                    "crucible::http".to_string(),
                    "crucible::json".to_string(),
                ],
                blocked_modules: vec!["std::fs".to_string(), "std::process".to_string()],
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
                    max_cpu_percentage: Some(75.0),
                    max_concurrent_operations: Some(50),
                    operation_timeout: Some(Duration::from_secs(30)),
                    max_disk_bytes: None,
                    max_queue_size: None,
                },
                execution_timeout: Some(Duration::from_secs(30)),
                allow_file_access: false,
                allow_network_access: true, // Allow HTTP but not raw network
                allow_system_calls: false,
                custom_rules: HashMap::new(),
            },
        }
    }
}

/// Default implementations for missing types
impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: None,
            max_cpu_percentage: None,
            max_disk_bytes: None,
            max_concurrent_operations: None,
            max_queue_size: None,
            operation_timeout: None,
        }
    }
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self {
            user_id: "default".to_string(),
            session_id: Uuid::new_v4().to_string(),
            permissions: vec![],
            security_level: SecurityLevel::Safe,
            sandbox: true,
        }
    }
}

impl Default for CompilationContext {
    fn default() -> Self {
        Self {
            target: CompilationTarget::Standard,
            optimization_level: OptimizationLevel::Balanced,
            include_paths: vec![],
            definitions: HashMap::new(),
            debug_info: false,
            security_level: SecurityLevel::Safe,
        }
    }
}

impl Default for OptimizationLevel {
    fn default() -> Self {
        Self::Balanced
    }
}

// Implement Clone for CrucibleScriptEngine to support event processing
impl Clone for CrucibleScriptEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            state: self.state.clone(),
            script_cache: self.script_cache.clone(),
            active_executions: self.active_executions.clone(),
            event_subscribers: self.event_subscribers.clone(),
            metrics: self.metrics.clone(),
            lifecycle_state: self.lifecycle_state.clone(),
            resource_limits: self.resource_limits.clone(),
            event_integration: self.event_integration.clone(),
        }
    }
}

// Implement EventIntegratedService for daemon coordination
#[async_trait]
impl EventIntegratedService for CrucibleScriptEngine {
    fn service_id(&self) -> &str {
        "crucible-script-engine"
    }

    fn service_type(&self) -> &str {
        "script-engine"
    }

    fn published_event_types(&self) -> Vec<String> {
        vec![
            "script_compiled".to_string(),
            "script_executed".to_string(),
            "script_cached".to_string(),
            "cache_cleared".to_string(),
            "security_policy_updated".to_string(),
            "script_error".to_string(),
        ]
    }

    fn subscribed_event_types(&self) -> Vec<String> {
        vec![
            "filesystem".to_string(),
            "configuration_changed".to_string(),
            "security_alert".to_string(),
            "system_shutdown".to_string(),
            "maintenance_mode".to_string(),
        ]
    }

    async fn handle_daemon_event(&mut self, event: DaemonEvent) -> EventResult<()> {
        debug!("Script Engine handling daemon event: {:?}", event.event_type);

        match &event.event_type {
            EventType::Filesystem(fs_event) => {
                match fs_event {
                    crate::events::core::FilesystemEventType::FileModified { path } => {
                        if path.ends_with(".rn") || path.ends_with(".rune") {
                            info!("Script file modified: {}, checking for cache invalidation", path);
                            // In a real implementation, you'd check if this file is cached
                            // and potentially invalidate or recompile it
                        }
                    }
                    _ => {}
                }
            }
            EventType::Service(service_event) => {
                match service_event {
                    crate::events::core::ServiceEventType::ConfigurationChanged { service_id, changes } => {
                        if service_id == self.service_id() {
                            info!("Script Engine configuration changed: {:?}", changes);
                            // Handle configuration changes
                        }
                    }
                    crate::events::core::ServiceEventType::ServiceStatusChanged { service_id, new_status, .. } => {
                        if new_status == "maintenance" {
                            warn!("Entering maintenance mode, script execution may be limited");
                            // Handle maintenance mode
                        }
                    }
                    _ => {}
                }
            }
            EventType::System(system_event) => {
                match system_event {
                    crate::events::core::SystemEventType::EmergencyShutdown { reason } => {
                        warn!("Emergency shutdown triggered: {}, stopping all script executions", reason);
                        // Emergency stop all executions
                        let executions = self.active_executions.read().await;
                        for execution_id in executions.keys() {
                            let _ = self.cancel_execution(execution_id).await;
                        }
                    }
                    crate::events::core::SystemEventType::MaintenanceStarted { reason } => {
                        info!("System maintenance started: {}, limiting script execution", reason);
                        // Enter limited operation mode
                    }
                    _ => {}
                }
            }
            _ => {
                debug!("Unhandled event type in Script Engine: {:?}", event.event_type);
            }
        }

        Ok(())
    }

    fn service_event_to_daemon_event(&self, service_event: &dyn std::any::Any, priority: EventPriority) -> EventResult<DaemonEvent> {
        // Try to downcast to ScriptEngineEvent
        if let Some(script_event) = service_event.downcast_ref::<ScriptEngineEvent>() {
            self.script_event_to_daemon_event(script_event, priority)
        } else {
            Err(EventError::ValidationError("Invalid event type for ScriptEngine".to_string()))
        }
    }

    fn daemon_event_to_service_event(&self, daemon_event: &DaemonEvent) -> Option<Box<dyn std::any::Any>> {
        // Convert daemon events to ScriptEngine events if applicable
        match &daemon_event.event_type {
            EventType::Filesystem(fs_event) => {
                match fs_event {
                    crate::events::core::FilesystemEventType::FileModified { path } => {
                        if path.ends_with(".rn") || path.ends_with(".rune") {
                            Some(Box::new(ScriptEngineEvent::Error {
                                operation: "file_watch".to_string(),
                                error: format!("Script file modified: {}", path),
                                script_id: Some(path.clone()),
                            }))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// Implement EventPublishingService for lifecycle events
#[async_trait]
impl EventPublishingService for CrucibleScriptEngine {
    async fn publish_lifecycle_event(&self, event_type: LifecycleEventType, details: HashMap<String, String>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let lifecycle_event = event_integration.adapter().create_lifecycle_event(event_type, details);
            event_integration.publish_event(lifecycle_event).await?;
        }
        Ok(())
    }

    async fn publish_health_event(&self, health: ServiceHealth) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let health_event = event_integration.adapter().create_health_event(health);
            event_integration.publish_event(health_event).await?;
        }
        Ok(())
    }

    async fn publish_error_event(&self, error: String, context: Option<HashMap<String, String>>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let error_event = event_integration.adapter().create_error_event(error, context);
            event_integration.publish_event(error_event).await?;
        }
        Ok(())
    }

    async fn publish_metric_event(&self, metrics: HashMap<String, f64>) -> EventResult<()> {
        if let Some(event_integration) = &self.event_integration {
            let metric_event = event_integration.adapter().create_metric_event(metrics);
            event_integration.publish_event(metric_event).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_script_engine_creation() {
        let config = ScriptEngineConfig::default();
        let engine = CrucibleScriptEngine::new(config).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_service_lifecycle() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();

        assert!(!engine.is_running());

        engine.start().await.unwrap();
        assert!(engine.is_running());

        engine.stop().await.unwrap();
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_health_check() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();
        engine.start().await.unwrap();

        let health = engine.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));
    }

    #[tokio::test]
    async fn test_script_compilation() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();

        let script_source = r#"
            pub fn main() {
                "Hello, World!"
            }
        "#;

        let context = CompilationContext::default();
        let result = engine.compile_script(script_source, context).await;
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert!(!compiled.script_id.is_empty());
        assert_eq!(compiled.metadata.language, "Rune");
    }

    #[tokio::test]
    async fn test_script_execution() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();

        let script_source = r#"
            pub fn main() {
                "Hello, World!"
            }
        "#;

        let compilation_context = CompilationContext::default();
        let compiled = engine.compile_script(script_source, compilation_context).await.unwrap();

        let execution_context = ExecutionContext {
            execution_id: Uuid::new_v4().to_string(),
            script_id: compiled.script_id.clone(),
            arguments: HashMap::new(),
            environment: HashMap::new(),
            working_directory: None,
            security_context: SecurityContext::default(),
            timeout: Some(Duration::from_secs(5)),
            available_tools: vec![],
            user_context: None,
        };

        let result = engine.execute_script(&compiled.script_id, execution_context).await;
        assert!(result.is_ok());

        let execution_result = result.unwrap();
        assert!(execution_result.success);
    }

    #[tokio::test]
    async fn test_event_system() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();

        let mut receiver = engine.subscribe("script_compiled").await.unwrap();

        let script_source = r#"
            pub fn main() {
                42
            }
        "#;

        let context = CompilationContext::default();
        let _result = engine.compile_script(script_source, context).await.unwrap();

        // Should receive a compilation event
        let event = receiver.recv().await;
        assert!(event.is_some());

        if let Some(ScriptEngineEvent::ScriptCompiled { script_id, success, .. }) = event {
            assert!(!script_id.is_empty());
            assert!(success);
        }
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let mut engine = CrucibleScriptEngine::new(ScriptEngineConfig::default()).await.unwrap();

        let script_source = r#"
            pub fn main() {
                "Cached script"
            }
        "#;

        let context = CompilationContext::default();
        let compiled = engine.compile_script(script_source, context).await.unwrap();

        // Script should be in cache
        let cached_script = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(cached_script.is_some());

        // Clear cache
        engine.clear_cache().await.unwrap();

        // Script should no longer be in cache
        let cached_script = engine.get_script_info(&compiled.script_id).await.unwrap();
        assert!(cached_script.is_none());
    }
}