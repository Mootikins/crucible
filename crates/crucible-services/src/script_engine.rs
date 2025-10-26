//! Script Engine Service Implementation
//!
//! This module provides a simplified ScriptEngine service that implements
//! the ScriptEngine trait using VM-per-execution pattern for security and stability.
//! The service provides proper isolation, security policies, and performance optimization.

use super::{
    errors::{ServiceError, ServiceResult},
    logging::{EventMetrics, EventTracer},
    service_traits::{HealthCheck, ScriptEngine, ServiceLifecycle},
    service_types::*,
    types::{ExecutionStatus, ServiceHealth, ServiceStatus},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

/// Script Engine service for Rune script execution
#[derive(Debug)]
pub struct CrucibleScriptEngine {
    /// Service configuration
    #[allow(dead_code)]
    config: ScriptEngineConfig,
    /// Service state
    state: Arc<RwLock<ScriptEngineState>>,
    /// Compiled script cache
    script_cache: Arc<RwLock<HashMap<String, CompiledScript>>>,
    /// Active executions tracking
    active_executions: Arc<RwLock<HashMap<String, ExecutionState>>>,
    /// Performance metrics
    metrics: Arc<RwLock<ScriptEngineMetrics>>,
    /// Service lifecycle state
    lifecycle_state: Arc<RwLock<ServiceLifecycleState>>,
    /// Resource limits
    #[allow(dead_code)]
    resource_limits: Arc<RwLock<ResourceLimits>>,
    /// Event tracer for debugging
    event_tracer: EventTracer,
    /// Event processing metrics
    event_metrics: Arc<RwLock<EventMetrics>>,
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
}

/// Execution state for tracking active executions
#[derive(Debug, Clone)]
struct ExecutionState {
    /// Start time
    started_at: Instant,
    /// Status
    status: ExecutionStatus,
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
#[derive(Debug, Clone, PartialEq)]
enum ServiceLifecycleState {
    Uninitialized,
    Starting,
    Running,
    Stopping,
    Stopped,
}

impl Default for ServiceLifecycleState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl CrucibleScriptEngine {
    /// Create a new script engine service
    pub fn new(config: ScriptEngineConfig) -> Self {
        let limits = config.default_security_context.limits.clone();
        Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(ScriptEngineState {
                running: false,
                started_at: None,
                security_policy: SecurityPolicy::default(),
            })),
            script_cache: Arc::new(RwLock::new(HashMap::new())),
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ScriptEngineMetrics::default())),
            lifecycle_state: Arc::new(RwLock::new(ServiceLifecycleState::Uninitialized)),
            resource_limits: Arc::new(RwLock::new(limits)),
            event_tracer: EventTracer::new("CrucibleScriptEngine"),
            event_metrics: Arc::new(RwLock::new(EventMetrics::default())),
        }
    }

    /// Generate a script hash for caching
    fn generate_script_hash(&self, source: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Update performance metrics
    async fn update_metrics<F>(&self, updater: F)
    where
        F: FnOnce(&mut ScriptEngineMetrics),
    {
        let mut metrics = self.metrics.write().await;
        updater(&mut metrics);
    }

    /// Check if execution should timeout
    #[allow(dead_code)]
    async fn check_execution_timeout(&self, _execution_id: &str) -> bool {
        // Simple implementation - always return false for now
        false
    }

    /// Clean up completed executions
    async fn cleanup_completed_executions(&self) {
        let mut executions = self.active_executions.write().await;
        let before_count = executions.len();

        executions.retain(|execution_id, state| {
            let keep = matches!(state.status, ExecutionStatus::Running)
                && state.started_at.elapsed() < Duration::from_secs(300); // 5 minute max lifetime

            if !keep {
                debug!(
                    execution_id = %execution_id,
                    status = ?state.status,
                    age_seconds = state.started_at.elapsed().as_secs(),
                    "Cleaning up completed execution"
                );
            }

            keep
        });

        let cleaned_count = before_count - executions.len();
        if cleaned_count > 0 {
            debug!(
                cleaned_count = cleaned_count,
                active_count = executions.len(),
                "Execution cleanup completed"
            );
        }
    }

    /// Validate script security
    async fn validate_script_security(&self, source: &str) -> SecurityValidationResult {
        let state = self.state.read().await;
        let _policy = &state.security_policy;

        // Basic security validation
        let mut issues = Vec::new();
        let mut valid = true;

        // Check for potentially dangerous operations
        if source.contains("std::process::") {
            issues.push(SecurityIssue {
                issue_type: "dangerous_import".to_string(),
                severity: SecurityLevel::Dangerous,
                description: "Script uses process operations".to_string(),
                location: Some("import".to_string()),
            });
            valid = false;
        }

        if source.contains("std::fs::remove_dir_all") {
            issues.push(SecurityIssue {
                issue_type: "dangerous_operation".to_string(),
                severity: SecurityLevel::Dangerous,
                description: "Script uses dangerous file operations".to_string(),
                location: Some("file_operation".to_string()),
            });
            valid = false;
        }

        SecurityValidationResult {
            security_level: if valid {
                SecurityLevel::Safe
            } else {
                SecurityLevel::Untrusted
            },
            valid,
            issues: issues.clone(),
            recommendations: if issues.is_empty() {
                vec!["Script passed security validation".to_string()]
            } else {
                vec!["Review and remove dangerous operations".to_string()]
            },
        }
    }

    /// Execute script in isolated VM
    async fn execute_in_vm(
        &self,
        script: &CompiledScript,
        context: ExecutionContext,
    ) -> ExecutionResult {
        let start_time = Instant::now();
        let execution_id = &context.execution_id;

        // Log event start
        debug!(
            execution_id = %execution_id,
            script_id = %script.script_id,
            "Starting script execution in isolated VM"
        );

        self.event_tracer.trace_event_start(
            execution_id,
            "script_execution",
            Some(&serde_json::json!({
                "script_id": script.script_id,
                "script_name": script.script_name,
                "parameters": context.parameters
            })),
        );

        // TODO: Implement actual script execution with Rune VM
        // For now, simulate execution with a simple mock result
        let mock_result = serde_json::json!({
            "message": "Script executed successfully",
            "script_id": script.script_id,
            "parameters": context.parameters
        });

        let duration = start_time.elapsed();
        let duration_ms = duration.as_millis() as u64;

        let execution_result = ExecutionResult {
            execution_id: execution_id.clone(),
            success: true,
            result: Some(mock_result.clone()),
            error: None,
            duration_ms,
            memory_used_bytes: 1024, // Mock memory usage
            output: Some("Script execution completed".to_string()),
        };

        // Update metrics
        self.update_metrics(|metrics| {
            metrics.total_executions += 1;
            if execution_result.success {
                metrics.successful_executions += 1;
            }
            metrics.total_execution_time += duration;
        })
        .await;

        // Update event metrics
        {
            let mut event_metrics = self.event_metrics.write().await;
            event_metrics.record_event(duration_ms, execution_result.success);
        }

        // Log event completion
        debug!(
            execution_id = %execution_id,
            script_id = %script.script_id,
            duration_ms = duration_ms,
            success = execution_result.success,
            "Script execution completed"
        );

        self.event_tracer
            .trace_event_complete(execution_id, duration_ms, execution_result.success);

        // Log routing decision (script execution to result processing)
        self.event_tracer.trace_routing(
            execution_id,
            "script_engine",
            "result_processor",
            "execution_completed",
        );

        trace!(
            execution_id = %execution_id,
            result = %serde_json::to_string(&mock_result).unwrap_or_default(),
            "Script execution result details"
        );

        execution_result
    }
}

#[async_trait]
impl ServiceLifecycle for CrucibleScriptEngine {
    async fn start(&mut self) -> ServiceResult<()> {
        info!("Starting Script Engine service");

        // Update lifecycle state
        {
            let mut state = self.lifecycle_state.write().await;
            *state = ServiceLifecycleState::Starting;
        }

        // Initialize script cache
        {
            let mut cache = self.script_cache.write().await;
            cache.clear();
        }

        // Initialize metrics
        {
            let mut metrics = self.metrics.write().await;
            *metrics = ScriptEngineMetrics::default();
        }

        // Update service state
        {
            let mut state = self.state.write().await;
            state.running = true;
            state.started_at = Some(chrono::Utc::now());
        }

        // Update lifecycle state
        {
            let mut state = self.lifecycle_state.write().await;
            *state = ServiceLifecycleState::Running;
        }

        info!("Script Engine service started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        info!("Stopping Script Engine service");

        // Update lifecycle state
        {
            let mut state = self.lifecycle_state.write().await;
            *state = ServiceLifecycleState::Stopping;
        }

        // Cancel all active executions
        {
            let mut executions = self.active_executions.write().await;
            for (_, execution_state) in executions.iter_mut() {
                execution_state.status = ExecutionStatus::Cancelled;
            }
            executions.clear();
        }

        // Clear script cache
        {
            let mut cache = self.script_cache.write().await;
            cache.clear();
        }

        // Update service state
        {
            let mut state = self.state.write().await;
            state.running = false;
        }

        // Update lifecycle state
        {
            let mut state = self.lifecycle_state.write().await;
            *state = ServiceLifecycleState::Stopped;
        }

        info!("Script Engine service stopped successfully");
        Ok(())
    }

    fn is_running(&self) -> bool {
        // Note: This is a synchronous method, so we can't access the async state
        // In a real implementation, you might use an atomic boolean or other sync primitive
        true
    }

    fn service_name(&self) -> &str {
        "CrucibleScriptEngine"
    }
}

#[async_trait]
impl HealthCheck for CrucibleScriptEngine {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        let state = self.lifecycle_state.read().await;

        let status = match *state {
            ServiceLifecycleState::Running => ServiceStatus::Healthy,
            ServiceLifecycleState::Starting | ServiceLifecycleState::Stopping => {
                ServiceStatus::Degraded
            }
            ServiceLifecycleState::Stopped => ServiceStatus::Unhealthy,
            ServiceLifecycleState::Uninitialized => ServiceStatus::Unhealthy,
        };

        Ok(ServiceHealth {
            status,
            message: Some(format!("Service is {:?}", state)),
            last_check: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl ScriptEngine for CrucibleScriptEngine {
    async fn compile_script(&mut self, source: &str) -> ServiceResult<CompiledScript> {
        let start_time = Instant::now();
        let source_preview = if source.len() > 100 {
            format!("{}...", &source[..100])
        } else {
            source.to_string()
        };

        debug!(
            source_preview = %source_preview,
            source_length = source.len(),
            "Starting script compilation"
        );

        // Validate script security
        let security_result = self.validate_script_security(source).await;
        if !security_result.valid {
            error!(
                security_issues = ?security_result.issues,
                "Script failed security validation"
            );
            return Err(ServiceError::ValidationError(format!(
                "Script failed security validation: {:?}",
                security_result.issues
            )));
        }

        trace!("Script security validation passed");

        // Generate script hash
        let script_hash = self.generate_script_hash(source);

        // Check cache first
        {
            let cache = self.script_cache.read().await;
            if let Some(cached_script) = cache.get(&script_hash) {
                self.update_metrics(|metrics| {
                    metrics.cache_hits += 1;
                })
                .await;

                debug!(
                    script_hash = %script_hash,
                    "Script cache hit, returning cached compiled script"
                );

                return Ok(cached_script.clone());
            }
        }

        self.update_metrics(|metrics| {
            metrics.cache_misses += 1;
        })
        .await;

        debug!(
            script_hash = %script_hash,
            "Script cache miss, proceeding with compilation"
        );

        // Compile the script
        let script_id = Uuid::new_v4().to_string();

        trace!(
            script_id = %script_id,
            "Generating new script ID"
        );

        // TODO: Implement actual Rune compilation
        // For now, create a mock compiled script without actual Rune compilation
        let compiled_script = CompiledScript {
            script_id: script_id.clone(),
            script_name: format!("script_{}", script_id),
            compiled_at: chrono::Utc::now(),
            script_hash: script_hash.clone(),
            security_validated: true,
        };

        // Cache the compiled script
        {
            let mut cache = self.script_cache.write().await;
            cache.insert(compiled_script.script_hash.clone(), compiled_script.clone());
        }

        let compilation_time = start_time.elapsed();

        // Update metrics
        self.update_metrics(|metrics| {
            metrics.total_compilations += 1;
            metrics.successful_compilations += 1;
            metrics.total_compilation_time += compilation_time;
        })
        .await;

        info!(
            script_id = %script_id,
            script_hash = %script_hash,
            compilation_time_ms = compilation_time.as_millis(),
            "Script compilation completed successfully"
        );

        Ok(compiled_script)
    }

    async fn execute_script(
        &self,
        script_id: &str,
        context: ExecutionContext,
    ) -> ServiceResult<ExecutionResult> {
        let execution_id = context.execution_id.clone();

        debug!(
            execution_id = %execution_id,
            script_id = %script_id,
            "Starting script execution request"
        );

        // Find the compiled script
        let compiled_script = {
            let cache = self.script_cache.read().await;
            cache
                .values()
                .find(|script| script.script_id == script_id)
                .cloned()
                .ok_or_else(|| {
                    error!(
                        execution_id = %execution_id,
                        script_id = %script_id,
                        "Script not found in cache"
                    );
                    ServiceError::ToolNotFound(script_id.to_string())
                })?
        };

        trace!(
            execution_id = %execution_id,
            script_name = %compiled_script.script_name,
            "Found compiled script in cache"
        );

        // Create execution state
        let execution_state = ExecutionState {
            started_at: Instant::now(),
            status: ExecutionStatus::Running,
        };

        // Track active execution
        let active_count = {
            let mut executions = self.active_executions.write().await;
            executions.insert(execution_id.clone(), execution_state.clone());
            executions.len()
        };

        debug!(
            execution_id = %execution_id,
            active_executions = %active_count,
            "Added execution to active tracking"
        );

        // Log routing decision (request to execution)
        self.event_tracer.trace_routing(
            &execution_id,
            "execution_request",
            "script_engine",
            "script_found",
        );

        // Execute the script
        let result = self.execute_in_vm(&compiled_script, context).await;

        // Clean up execution tracking
        {
            let mut executions = self.active_executions.write().await;
            executions.remove(&result.execution_id);
        }

        // Periodic cleanup
        self.cleanup_completed_executions().await;

        info!(
            execution_id = %execution_id,
            script_id = %script_id,
            success = result.success,
            duration_ms = result.duration_ms,
            "Script execution request completed"
        );

        Ok(result)
    }

    async fn register_tool(&mut self, tool: ScriptTool) -> ServiceResult<()> {
        // TODO: Implement tool registration
        info!("Registering tool: {}", tool.name);
        Ok(())
    }

    async fn list_tools(&self) -> ServiceResult<Vec<ScriptTool>> {
        // TODO: Implement tool listing
        Ok(Vec::new())
    }

    async fn get_execution_stats(&self) -> ServiceResult<ScriptExecutionStats> {
        let metrics = self.metrics.read().await;
        let event_metrics = self.event_metrics.read().await;

        let avg_execution_time = if metrics.total_executions > 0 {
            metrics.total_execution_time.as_millis() as f64 / metrics.total_executions as f64
        } else {
            0.0
        };

        // Log event metrics
        event_metrics.log_metrics("CrucibleScriptEngine");

        Ok(ScriptExecutionStats {
            total_executions: metrics.total_executions,
            successful_executions: metrics.successful_executions,
            failed_executions: metrics.total_executions - metrics.successful_executions,
            avg_execution_time_ms: avg_execution_time,
            total_memory_used_bytes: metrics.peak_memory_usage,
            last_updated: chrono::Utc::now(),
        })
    }
}

/// Default security policy
impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            allowed_operations: vec![
                "read".to_string(),
                "write".to_string(),
                "compute".to_string(),
            ],
            denied_operations: vec![
                "std::process::".to_string(),
                "std::fs::remove_dir_all".to_string(),
                "unsafe".to_string(),
            ],
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
                max_cpu_percentage: Some(80.0),
                operation_timeout: Some(Duration::from_secs(30)),
            },
            sandbox_requirements: vec![
                "isolate_filesystem".to_string(),
                "limit_network_access".to_string(),
            ],
        }
    }
}
