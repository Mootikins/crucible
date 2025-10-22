//! # Health Monitor
//!
//! This module implements the HealthMonitor which performs regular health checks
//! on plugin instances, detects anomalies, and manages recovery strategies.

use super::config::{HealthMonitoringConfig, RecoveryConfig, BackoffStrategy, EscalationConfig};
use super::error::{PluginError, PluginResult, ErrorContext};
use super::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};

/// ============================================================================
/// HEALTH MONITOR TRAIT
/// ============================================================================

#[async_trait]
pub trait HealthMonitor: Send + Sync {
    /// Start health monitoring
    async fn start(&mut self) -> PluginResult<()>;

    /// Stop health monitoring
    async fn stop(&mut self) -> PluginResult<()>;

    /// Register a plugin instance for monitoring
    async fn register_instance(&mut self, instance_id: String, plugin_id: String, config: HealthCheckConfig) -> PluginResult<()>;

    /// Unregister a plugin instance
    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()>;

    /// Get current health status of an instance
    async fn get_instance_health(&self, instance_id: &str) -> PluginResult<PluginHealthStatus>;

    /// Get health status of all instances
    async fn get_all_health(&self) -> PluginResult<HashMap<String, PluginHealthStatus>>;

    /// Perform manual health check on instance
    async fn perform_health_check(&self, instance_id: &str) -> PluginResult<HealthCheckResult>;

    /// Update health check configuration for instance
    async fn update_instance_config(&mut self, instance_id: &str, config: HealthCheckConfig) -> PluginResult<()>;

    /// Get health metrics
    async fn get_health_metrics(&self) -> PluginResult<HealthMetrics>;

    /// Subscribe to health events
    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<HealthEvent>;

    /// Force health status update for instance
    async fn force_health_update(&mut self, instance_id: &str, status: PluginHealthStatus, reason: Option<String>) -> PluginResult<()>;

    /// Get system health summary
    async fn get_system_health(&self) -> PluginResult<SystemHealthSummary>;
}

/// ============================================================================
/// HEALTH TYPES
/// ============================================================================

/// Health check configuration for a plugin instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check strategies
    pub strategies: Vec<HealthCheckStrategy>,
    /// Check interval
    pub check_interval: Duration,
    /// Check timeout
    pub check_timeout: Duration,
    /// Unhealthy threshold (consecutive failures)
    pub unhealthy_threshold: u32,
    /// Recovery configuration
    pub recovery: RecoveryConfig,
    /// Custom health check parameters
    pub custom_parameters: HashMap<String, serde_json::Value>,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Instance ID
    pub instance_id: String,
    /// Check timestamp
    pub timestamp: SystemTime,
    /// Overall health status
    pub status: PluginHealthStatus,
    /// Individual strategy results
    pub strategy_results: Vec<StrategyResult>,
    /// Response time
    pub response_time: Duration,
    /// Error message (if unhealthy)
    pub error_message: Option<String>,
    /// Additional metrics
    pub metrics: HashMap<String, f64>,
}

/// Individual strategy result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyResult {
    /// Strategy name
    pub strategy_name: String,
    /// Strategy type
    pub strategy_type: HealthCheckType,
    /// Success status
    pub success: bool,
    /// Execution time
    pub execution_time: Duration,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Strategy-specific data
    pub data: HashMap<String, serde_json::Value>,
}

/// Health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Total health checks performed
    pub total_checks: u64,
    /// Successful checks
    pub successful_checks: u64,
    /// Failed checks
    pub failed_checks: u64,
    /// Average response time
    pub average_response_time: Duration,
    /// Checks by status
    pub checks_by_status: HashMap<PluginHealthStatus, u64>,
    /// Unhealthy instances
    pub unhealthy_instances: Vec<String>,
    /// Degraded instances
    pub degraded_instances: Vec<String>,
    /// Healthy instances
    pub healthy_instances: Vec<String>,
    /// Recovery attempts
    pub recovery_attempts: u64,
    /// Successful recoveries
    pub successful_recoveries: u64,
    /// Metrics collection timestamp
    pub timestamp: SystemTime,
}

/// System health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSummary {
    /// Overall system health
    pub overall_health: PluginHealthStatus,
    /// Total instances
    pub total_instances: u32,
    /// Healthy instances count
    pub healthy_count: u32,
    /// Degraded instances count
    pub degraded_count: u32,
    /// Unhealthy instances count
    pub unhealthy_count: u32,
    /// Unknown instances count
    pub unknown_count: u32,
    /// System uptime
    pub uptime: Duration,
    /// Last health check timestamp
    pub last_check: SystemTime,
    /// Health score (0-100)
    pub health_score: u32,
    /// Critical issues
    pub critical_issues: Vec<HealthIssue>,
}

/// Health issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthIssue {
    /// Issue ID
    pub issue_id: String,
    /// Instance ID
    pub instance_id: String,
    /// Issue type
    pub issue_type: HealthIssueType,
    /// Severity
    pub severity: HealthIssueSeverity,
    /// Description
    pub description: String,
    /// First detected timestamp
    pub first_detected: SystemTime,
    /// Last detected timestamp
    pub last_detected: SystemTime,
    /// Occurrence count
    pub occurrence_count: u32,
    /// Recommended action
    pub recommended_action: String,
}

/// Health issue type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthIssueType {
    /// Process not running
    ProcessNotRunning,
    /// Not responding to health checks
    NotResponding,
    /// Resource exhaustion
    ResourceExhaustion,
    /// High resource usage
    HighResourceUsage,
    /// Communication failure
    CommunicationFailure,
    /// Startup timeout
    StartupTimeout,
    /// Crash loop
    CrashLoop,
    /// Anomalous behavior
    AnomalousBehavior,
    /// Custom issue type
    Custom(String),
}

/// Health issue severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthIssueSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Health event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthEvent {
    /// Health status changed
    StatusChanged {
        instance_id: String,
        old_status: PluginHealthStatus,
        new_status: PluginHealthStatus,
        reason: Option<String>,
    },
    /// Health check completed
    CheckCompleted {
        instance_id: String,
        result: HealthCheckResult,
    },
    /// Recovery initiated
    RecoveryInitiated {
        instance_id: String,
        recovery_type: RecoveryType,
        reason: String,
    },
    /// Recovery completed
    RecoveryCompleted {
        instance_id: String,
        success: bool,
        duration: Duration,
    },
    /// Health issue detected
    IssueDetected {
        instance_id: String,
        issue: HealthIssue,
    },
    /// Health issue resolved
    IssueResolved {
        instance_id: String,
        issue_id: String,
    },
    /// System health changed
    SystemHealthChanged {
        old_health: PluginHealthStatus,
        new_health: PluginHealthStatus,
    },
}

/// Recovery type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryType {
    /// Restart instance
    Restart,
    /// Recreate instance
    Recreate,
    /// Scale resources
    ScaleResources,
    /// Clear cache
    ClearCache,
    /// Reset connections
    ResetConnections,
    /// Custom recovery
    Custom(String),
}

/// Monitored instance information
#[derive(Debug, Clone)]
struct MonitoredInstance {
    /// Instance ID
    instance_id: String,
    /// Plugin ID
    plugin_id: String,
    /// Health configuration
    config: HealthCheckConfig,
    /// Current health status
    current_status: PluginHealthStatus,
    /// Previous health status
    previous_status: PluginHealthStatus,
    /// Last health check timestamp
    last_check: Option<SystemTime>,
    /// Consecutive failures
    consecutive_failures: u32,
    /// Total failures
    total_failures: u64,
    /// Last successful check timestamp
    last_success: Option<SystemTime>,
    /// Recovery attempt count
    recovery_attempts: u32,
    /// Last recovery timestamp
    last_recovery: Option<SystemTime>,
    /// Active health issues
    active_issues: HashMap<String, HealthIssue>,
    /// Health history (last N checks)
    health_history: Vec<HealthCheckResult>,
}

/// ============================================================================
/// DEFAULT HEALTH MONITOR
/// ============================================================================

/// Default implementation of HealthMonitor
#[derive(Debug)]
pub struct DefaultHealthMonitor {
    /// Configuration
    config: Arc<HealthMonitoringConfig>,
    /// Monitored instances
    instances: Arc<RwLock<HashMap<String, MonitoredInstance>>>,
    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<HealthEvent>>>>,
    /// Metrics
    metrics: Arc<RwLock<HealthMetrics>>,
    /// Health check strategies
    strategies: Arc<RwLock<HashMap<String, Box<dyn HealthCheckStrategy>>>>,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Monitoring handle
    monitoring_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Start time
    start_time: Arc<RwLock<Option<SystemTime>>>,
}

impl DefaultHealthMonitor {
    /// Create a new health monitor
    pub fn new(config: HealthMonitoringConfig) -> Self {
        let mut strategies: HashMap<String, Box<dyn HealthCheckStrategy>> = HashMap::new();

        // Register default strategies
        strategies.insert("process".to_string(), Box::new(ProcessHealthCheck::new()));
        strategies.insert("resource".to_string(), Box::new(ResourceHealthCheck::new()));
        strategies.insert("ping".to_string(), Box::new(PingHealthCheck::new()));

        Self {
            config: Arc::new(config),
            instances: Arc::new(RwLock::new(HashMap::new())),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(HealthMetrics {
                total_checks: 0,
                successful_checks: 0,
                failed_checks: 0,
                average_response_time: Duration::ZERO,
                checks_by_status: HashMap::new(),
                unhealthy_instances: Vec::new(),
                degraded_instances: Vec::new(),
                healthy_instances: Vec::new(),
                recovery_attempts: 0,
                successful_recoveries: 0,
                timestamp: SystemTime::now(),
            })),
            strategies: Arc::new(RwLock::new(strategies)),
            running: Arc::new(RwLock::new(false)),
            monitoring_handle: Arc::new(RwLock::new(None)),
            start_time: Arc::new(RwLock::new(None)),
        }
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: HealthEvent) {
        let mut subscribers = self.event_subscribers.read().await;
        let mut to_remove = Vec::new();

        for (i, sender) in subscribers.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                to_remove.push(i);
            }
        }

        // Remove dead subscribers
        for i in to_remove.into_iter().rev() {
            subscribers.remove(i);
        }
    }

    /// Start monitoring loop
    async fn start_monitoring_loop(&self) {
        let config = self.config.clone();
        let instances = self.instances.clone();
        let strategies = self.strategies.clone();
        let metrics = self.metrics.clone();
        let event_subscribers = self.event_subscribers.clone();
        let running = self.running.clone();

        let handle = tokio::spawn(async move {
            let mut interval = interval(config.check_interval);

            loop {
                interval.tick().await;

                // Check if still running
                if !*running.read().await {
                    break;
                }

                // Perform health checks for all instances
                let instance_ids: Vec<String> = {
                    let instances_guard = instances.read().await;
                    instances_guard.keys().cloned().collect()
                };

                for instance_id in instance_ids {
                    if let Err(e) = Self::perform_health_check_internal(
                        &instance_id,
                        &instances,
                        &strategies,
                        &metrics,
                        &event_subscribers,
                    ).await {
                        error!("Health check failed for instance {}: {}", instance_id, e);
                    }
                }

                // Update overall metrics
                Self::update_system_metrics(&instances, &metrics).await;
            }
        });

        let mut monitoring_handle = self.monitoring_handle.write().await;
        *monitoring_handle = Some(handle);
    }

    /// Perform health check on a specific instance
    async fn perform_health_check_internal(
        instance_id: &str,
        instances: &Arc<RwLock<HashMap<String, MonitoredInstance>>>,
        strategies: &Arc<RwLock<HashMap<String, Box<dyn HealthCheckStrategy>>>>,
        metrics: &Arc<RwLock<HealthMetrics>>,
        event_subscribers: &Arc<RwLock<Vec<mpsc::UnboundedSender<HealthEvent>>>>,
    ) -> PluginResult<()> {
        let (instance_config, current_status) = {
            let instances_guard = instances.read().await;
            let instance = instances_guard.get(instance_id)
                .ok_or_else(|| PluginError::health_monitoring(format!("Instance {} not found", instance_id)))?;

            (instance.config.clone(), instance.current_status.clone())
        };

        let start_time = SystemTime::now();
        let mut strategy_results = Vec::new();
        let mut overall_success = true;
        let mut error_message = None;
        let mut check_metrics = HashMap::new();

        // Execute all configured strategies
        for strategy_config in &instance_config.strategies {
            if !strategy_config.enabled {
                continue;
            }

            let strategy_result = Self::execute_strategy(
                instance_id,
                strategy_config,
                strategies,
            ).await?;

            if !strategy_result.success {
                overall_success = false;
                if error_message.is_none() {
                    error_message = strategy_result.error_message.clone();
                }
            }

            // Collect metrics
            for (key, value) in &strategy_result.data {
                if let Some(num_value) = value.as_f64() {
                    check_metrics.insert(format!("{}_{}", strategy_config.strategy_name, key), *num_value);
                }
            }

            strategy_results.push(strategy_result);
        }

        let response_time = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

        // Determine overall health status
        let new_status = if overall_success {
            PluginHealthStatus::Healthy
        } else {
            PluginHealthStatus::Unhealthy
        };

        let result = HealthCheckResult {
            instance_id: instance_id.to_string(),
            timestamp: start_time,
            status: new_status.clone(),
            strategy_results,
            response_time,
            error_message,
            metrics: check_metrics,
        };

        // Update instance state
        {
            let mut instances_guard = instances.write().await;
            if let Some(instance) = instances_guard.get_mut(instance_id) {
                let old_status = std::mem::replace(&mut instance.current_status, new_status.clone());
                instance.previous_status = old_status.clone();
                instance.last_check = Some(start_time);

                if new_status == PluginHealthStatus::Healthy {
                    instance.consecutive_failures = 0;
                    instance.last_success = Some(start_time);
                } else {
                    instance.consecutive_failures += 1;
                    instance.total_failures += 1;
                }

                // Add to health history (keep last 100)
                instance.health_history.push(result.clone());
                if instance.health_history.len() > 100 {
                    instance.health_history.remove(0);
                }

                // Check if status changed
                if old_status != new_status {
                    Self::handle_status_change(
                        instance_id,
                        &old_status,
                        &new_status,
                        &instance.config,
                        instances,
                        event_subscribers,
                    ).await;
                }

                // Update metrics
                {
                    let mut metrics_guard = metrics.write().await;
                    metrics_guard.total_checks += 1;
                    if new_status == PluginHealthStatus::Healthy {
                        metrics_guard.successful_checks += 1;
                    } else {
                        metrics_guard.failed_checks += 1;
                    }

                    // Update average response time
                    let total_checks = metrics_guard.total_checks;
                    let current_avg = metrics_guard.average_response_time;
                    metrics_guard.average_response_time = Duration::from_nanos(
                        (current_avg.as_nanos() as u64 * (total_checks - 1) + response_time.as_nanos() as u64) / total_checks
                    );

                    *metrics_guard.checks_by_status.entry(new_status.clone()).or_insert(0) += 1;
                }
            }
        }

        // Publish event
        let event = HealthEvent::CheckCompleted {
            instance_id: instance_id.to_string(),
            result,
        };

        let mut subscribers = event_subscribers.read().await;
        for sender in subscribers.iter() {
            let _ = sender.send(event.clone());
        }

        Ok(())
    }

    /// Execute a single health check strategy
    async fn execute_strategy(
        instance_id: &str,
        strategy_config: &HealthCheckStrategy,
        strategies: &Arc<RwLock<HashMap<String, Box<dyn HealthCheckStrategy>>>>,
    ) -> PluginResult<StrategyResult> {
        let strategies_guard = strategies.read().await;
        let strategy = strategies_guard.get(&strategy_config.strategy_name)
            .ok_or_else(|| PluginError::health_monitoring(format!("Strategy {} not found", strategy_config.strategy_name)))?;

        strategy.execute(instance_id, strategy_config).await
    }

    /// Handle health status change
    async fn handle_status_change(
        instance_id: &str,
        old_status: &PluginHealthStatus,
        new_status: &PluginHealthStatus,
        config: &HealthCheckConfig,
        instances: &Arc<RwLock<HashMap<String, MonitoredInstance>>>,
        event_subscribers: &Arc<RwLock<Vec<mpsc::UnboundedSender<HealthEvent>>>>,
    ) {
        info!("Health status changed for instance {}: {:?} -> {:?}", instance_id, old_status, new_status);

        // Publish status change event
        let event = HealthEvent::StatusChanged {
            instance_id: instance_id.to_string(),
            old_status: old_status.clone(),
            new_status: new_status.clone(),
            reason: None,
        };

        let mut subscribers = event_subscribers.read().await;
        for sender in subscribers.iter() {
            let _ = sender.send(event.clone());
        }

        // Check if recovery is needed
        if matches!(new_status, PluginHealthStatus::Unhealthy) && config.recovery.enabled {
            Self::initiate_recovery(instance_id, config, instances, event_subscribers).await;
        }
    }

    /// Initiate recovery for unhealthy instance
    async fn initiate_recovery(
        instance_id: &str,
        config: &HealthCheckConfig,
        instances: &Arc<RwLock<HashMap<String, MonitoredInstance>>>,
        event_subscribers: &Arc<RwLock<Vec<mpsc::UnboundedSender<HealthEvent>>>>,
    ) {
        let recovery_reason = format!("Instance unhealthy for {} consecutive checks", config.unhealthy_threshold);

        // Check if recovery should be attempted
        let should_recover = {
            let instances_guard = instances.read().await;
            if let Some(instance) = instances_guard.get(instance_id) {
                instance.recovery_attempts < config.recovery.max_restart_attempts &&
                instance.consecutive_failures >= config.unhealthy_threshold
            } else {
                false
            }
        };

        if !should_recover {
            return;
        }

        info!("Initiating recovery for instance: {}", instance_id);

        // Update recovery attempt count
        {
            let mut instances_guard = instances.write().await;
            if let Some(instance) = instances_guard.get_mut(instance_id) {
                instance.recovery_attempts += 1;
                instance.last_recovery = Some(SystemTime::now());
            }
        }

        // Publish recovery initiated event
        let recovery_event = HealthEvent::RecoveryInitiated {
            instance_id: instance_id.to_string(),
            recovery_type: RecoveryType::Restart,
            reason: recovery_reason,
        };

        let mut subscribers = event_subscribers.read().await;
        for sender in subscribers.iter() {
            let _ = sender.send(recovery_event.clone());
        }

        // In a real implementation, you would coordinate with the PluginManager
        // to actually perform the recovery (restart, recreate, etc.)
        // For now, just log the intent
        warn!("Recovery initiated for instance {} - implementation needed", instance_id);
    }

    /// Update system-level metrics
    async fn update_system_metrics(
        instances: &Arc<RwLock<HashMap<String, MonitoredInstance>>>,
        metrics: &Arc<RwLock<HealthMetrics>>,
    ) {
        let instances_guard = instances.read().await;
        let mut healthy_instances = Vec::new();
        let mut degraded_instances = Vec::new();
        let mut unhealthy_instances = Vec::new();

        for (instance_id, instance) in instances_guard.iter() {
            match instance.current_status {
                PluginHealthStatus::Healthy => healthy_instances.push(instance_id.clone()),
                PluginHealthStatus::Degraded => degraded_instances.push(instance_id.clone()),
                PluginHealthStatus::Unhealthy => unhealthy_instances.push(instance_id.clone()),
                PluginHealthStatus::Unknown => {} // Skip unknown status
            }
        }

        let mut metrics_guard = metrics.write().await;
        metrics_guard.healthy_instances = healthy_instances;
        metrics_guard.degraded_instances = degraded_instances;
        metrics_guard.unhealthy_instances = unhealthy_instances;
        metrics_guard.timestamp = SystemTime::now();
    }
}

#[async_trait]
impl HealthMonitor for DefaultHealthMonitor {
    async fn start(&mut self) -> PluginResult<()> {
        info!("Starting health monitor");

        {
            let mut running = self.running.write().await;
            if *running {
                return Err(PluginError::health_monitoring("Health monitor is already running".to_string()));
            }
            *running = true;
        }

        // Set start time
        {
            let mut start_time = self.start_time.write().await;
            *start_time = Some(SystemTime::now());
        }

        // Start monitoring loop
        self.start_monitoring_loop().await;

        info!("Health monitor started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        info!("Stopping health monitor");

        {
            let mut running = self.running.write().await;
            *running = false;
        }

        // Stop monitoring loop
        {
            let mut monitoring_handle = self.monitoring_handle.write().await;
            if let Some(handle) = monitoring_handle.take() {
                handle.abort();
            }
        }

        info!("Health monitor stopped");
        Ok(())
    }

    async fn register_instance(&mut self, instance_id: String, plugin_id: String, config: HealthCheckConfig) -> PluginResult<()> {
        debug!("Registering instance {} for health monitoring", instance_id);

        let instance = MonitoredInstance {
            instance_id: instance_id.clone(),
            plugin_id,
            config,
            current_status: PluginHealthStatus::Unknown,
            previous_status: PluginHealthStatus::Unknown,
            last_check: None,
            consecutive_failures: 0,
            total_failures: 0,
            last_success: None,
            recovery_attempts: 0,
            last_recovery: None,
            active_issues: HashMap::new(),
            health_history: Vec::new(),
        };

        {
            let mut instances = self.instances.write().await;
            instances.insert(instance_id.clone(), instance);
        }

        info!("Registered instance {} for health monitoring", instance_id);
        Ok(())
    }

    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        debug!("Unregistering instance {} from health monitoring", instance_id);

        {
            let mut instances = self.instances.write().await;
            instances.remove(instance_id);
        }

        info!("Unregistered instance {} from health monitoring", instance_id);
        Ok(())
    }

    async fn get_instance_health(&self, instance_id: &str) -> PluginResult<PluginHealthStatus> {
        let instances = self.instances.read().await;
        instances.get(instance_id)
            .map(|instance| instance.current_status.clone())
            .ok_or_else(|| PluginError::health_monitoring(format!("Instance {} not found", instance_id)))
    }

    async fn get_all_health(&self) -> PluginResult<HashMap<String, PluginHealthStatus>> {
        let instances = self.instances.read().await;
        let mut health_map = HashMap::new();

        for (instance_id, instance) in instances.iter() {
            health_map.insert(instance_id.clone(), instance.current_status.clone());
        }

        Ok(health_map)
    }

    async fn perform_health_check(&self, instance_id: &str) -> PluginResult<HealthCheckResult> {
        let instances = self.instances.clone();
        let strategies = self.strategies.clone();
        let metrics = self.metrics.clone();
        let event_subscribers = self.event_subscribers.clone();

        Self::perform_health_check_internal(
            instance_id,
            &instances,
            &strategies,
            &metrics,
            &event_subscribers,
        ).await?;

        // Return the latest result
        let instances_guard = instances.read().await;
        let instance = instances_guard.get(instance_id)
            .ok_or_else(|| PluginError::health_monitoring(format!("Instance {} not found", instance_id)))?;

        instance.health_history.last()
            .cloned()
            .ok_or_else(|| PluginError::health_monitoring("No health check results available".to_string()))
    }

    async fn update_instance_config(&mut self, instance_id: &str, config: HealthCheckConfig) -> PluginResult<()> {
        debug!("Updating health check configuration for instance {}", instance_id);

        {
            let mut instances = self.instances.write().await;
            let instance = instances.get_mut(instance_id)
                .ok_or_else(|| PluginError::health_monitoring(format!("Instance {} not found", instance_id)))?;

            instance.config = config;
        }

        info!("Updated health check configuration for instance {}", instance_id);
        Ok(())
    }

    async fn get_health_metrics(&self) -> PluginResult<HealthMetrics> {
        let metrics = self.metrics.read().await;
        Ok(metrics.clone())
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<HealthEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.push(tx);
        rx
    }

    async fn force_health_update(&mut self, instance_id: &str, status: PluginHealthStatus, reason: Option<String>) -> PluginResult<()> {
        info!("Forcing health update for instance {}: {:?} (reason: {:?})",
              instance_id, status, reason);

        let old_status = {
            let mut instances = self.instances.write().await;
            let instance = instances.get_mut(instance_id)
                .ok_or_else(|| PluginError::health_monitoring(format!("Instance {} not found", instance_id)))?;

            let old_status = instance.current_status.clone();
            instance.current_status = status.clone();
            instance.last_check = Some(SystemTime::now());

            if status == PluginHealthStatus::Healthy {
                instance.consecutive_failures = 0;
                instance.last_success = Some(SystemTime::now());
            }

            old_status
        };

        // Publish status change event
        self.publish_event(HealthEvent::StatusChanged {
            instance_id: instance_id.to_string(),
            old_status,
            new_status: status,
            reason,
        }).await;

        Ok(())
    }

    async fn get_system_health(&self) -> PluginResult<SystemHealthSummary> {
        let instances = self.instances.read().await;
        let metrics = self.metrics.read().await;
        let start_time = self.start_time.read().await;

        let mut healthy_count = 0;
        let mut degraded_count = 0;
        let mut unhealthy_count = 0;
        let mut unknown_count = 0;

        for instance in instances.values() {
            match instance.current_status {
                PluginHealthStatus::Healthy => healthy_count += 1,
                PluginHealthStatus::Degraded => degraded_count += 1,
                PluginHealthStatus::Unhealthy => unhealthy_count += 1,
                PluginHealthStatus::Unknown => unknown_count += 1,
            }
        }

        let total_instances = instances.len() as u32;

        // Calculate overall health
        let overall_health = if unhealthy_count > 0 {
            PluginHealthStatus::Unhealthy
        } else if degraded_count > 0 {
            PluginHealthStatus::Degraded
        } else if healthy_count > 0 {
            PluginHealthStatus::Healthy
        } else {
            PluginHealthStatus::Unknown
        };

        // Calculate health score (0-100)
        let health_score = if total_instances == 0 {
            100
        } else {
            ((healthy_count * 100 + degraded_count * 50) / total_instances).min(100)
        };

        // Calculate uptime
        let uptime = start_time
            .map(|start| SystemTime::now().duration_since(start).unwrap_or(Duration::ZERO))
            .unwrap_or(Duration::ZERO);

        let summary = SystemHealthSummary {
            overall_health,
            total_instances,
            healthy_count,
            degraded_count,
            unhealthy_count,
            unknown_count,
            uptime,
            last_check: metrics.timestamp,
            health_score,
            critical_issues: Vec::new(), // TODO: Implement critical issues detection
        };

        Ok(summary)
    }
}

/// ============================================================================
/// HEALTH CHECK STRATEGIES
/// ============================================================================

/// Trait for health check strategies
#[async_trait]
pub trait HealthCheckStrategy: Send + Sync {
    /// Get strategy name
    fn name(&self) -> &str;

    /// Execute health check
    async fn execute(&self, instance_id: &str, config: &HealthCheckStrategy) -> PluginResult<StrategyResult>;
}

/// Process health check strategy
#[derive(Debug)]
pub struct ProcessHealthCheck {
    // Strategy-specific state could go here
}

impl ProcessHealthCheck {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl HealthCheckStrategy for ProcessHealthCheck {
    fn name(&self) -> &str {
        "process"
    }

    async fn execute(&self, instance_id: &str, config: &HealthCheckStrategy) -> PluginResult<StrategyResult> {
        let start_time = SystemTime::now();

        // In a real implementation, you would check if the process is actually running
        // For now, simulate a process check

        #[cfg(unix)]
        {
            use std::fs;
            // Try to read from /proc/{pid}/status to check if process exists
            // This would require having the PID stored somewhere
        }

        // Simulate check
        let success = true; // Placeholder
        let execution_time = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

        Ok(StrategyResult {
            strategy_name: self.name().to_string(),
            strategy_type: HealthCheckType::Process,
            success,
            execution_time,
            error_message: if success { None } else { Some("Process not found".to_string()) },
            data: HashMap::new(),
        })
    }
}

/// Resource health check strategy
#[derive(Debug)]
pub struct ResourceHealthCheck {
    // Strategy-specific state could go here
}

impl ResourceHealthCheck {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl HealthCheckStrategy for ResourceHealthCheck {
    fn name(&self) -> &str {
        "resource"
    }

    async fn execute(&self, instance_id: &str, config: &HealthCheckStrategy) -> PluginResult<StrategyResult> {
        let start_time = SystemTime::now();

        // Check resource usage
        // In a real implementation, you would get actual resource usage metrics
        let memory_usage = 50.0; // MB (placeholder)
        let cpu_usage = 25.0; // Percentage (placeholder)

        let success = memory_usage < 80.0 && cpu_usage < 80.0;
        let execution_time = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

        let mut data = HashMap::new();
        data.insert("memory_usage".to_string(), serde_json::Value::Number(memory_usage.into()));
        data.insert("cpu_usage".to_string(), serde_json::Value::Number(cpu_usage.into()));

        Ok(StrategyResult {
            strategy_name: self.name().to_string(),
            strategy_type: HealthCheckType::Resource,
            success,
            execution_time,
            error_message: if success { None } else { Some("High resource usage".to_string()) },
            data,
        })
    }
}

/// Ping health check strategy
#[derive(Debug)]
pub struct PingHealthCheck {
    // Strategy-specific state could go here
}

impl PingHealthCheck {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl HealthCheckStrategy for PingHealthCheck {
    fn name(&self) -> &str {
        "ping"
    }

    async fn execute(&self, instance_id: &str, config: &HealthCheckStrategy) -> PluginResult<StrategyResult> {
        let start_time = SystemTime::now();

        // Send ping message to instance
        // In a real implementation, you would use IPC to send a ping

        let success = true; // Placeholder
        let execution_time = SystemTime::now().duration_since(start_time).unwrap_or(Duration::ZERO);

        Ok(StrategyResult {
            strategy_name: self.name().to_string(),
            strategy_type: HealthCheckType::Ping,
            success,
            execution_time,
            error_message: if success { None } else { Some("No response to ping".to_string()) },
            data: HashMap::new(),
        })
    }
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Create a default health monitor
pub fn create_health_monitor(config: HealthMonitoringConfig) -> Box<dyn HealthMonitor> {
    Box::new(DefaultHealthMonitor::new(config))
}