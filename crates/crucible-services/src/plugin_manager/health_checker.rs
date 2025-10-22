//! # Plugin Health Checker
//!
//! This module provides comprehensive health checking for plugin processes,
//! including process health validation, IPC channel health checks, custom health
//! endpoints, and health-based recovery actions.

use super::error::{PluginError, PluginResult};
use super::types::{PluginInstance, PluginHealthStatus, ResourceUsage};
use super::config::{HealthMonitoringConfig, HealthCheckStrategy, HealthCheckType, RecoveryConfig};
use crate::events::{EventEmitter, Event};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::{RwLock, mpsc, Semaphore};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// ============================================================================
/// HEALTH CHECKING CORE
/// ============================================================================

/// Health check event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthCheckEvent {
    /// Health check started
    CheckStarted {
        instance_id: String,
        check_type: HealthCheckType,
    },
    /// Health check completed successfully
    CheckCompleted {
        instance_id: String,
        check_type: HealthCheckType,
        duration: Duration,
        status: PluginHealthStatus,
    },
    /// Health check failed
    CheckFailed {
        instance_id: String,
        check_type: HealthCheckType,
        error: String,
        duration: Duration,
    },
    /// Plugin health status changed
    HealthStatusChanged {
        instance_id: String,
        old_status: PluginHealthStatus,
        new_status: PluginHealthStatus,
        reason: String,
    },
    /// Health-based recovery action triggered
    RecoveryActionTriggered {
        instance_id: String,
        action: RecoveryAction,
        reason: String,
    },
    /// Health check schedule updated
    ScheduleUpdated {
        instance_id: String,
        interval: Duration,
        strategies: Vec<String>,
    },
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Check type
    pub check_type: HealthCheckType,
    /// Health status
    pub status: PluginHealthStatus,
    /// Check duration
    pub duration: Duration,
    /// Check timestamp
    pub timestamp: SystemTime,
    /// Additional details
    pub details: HashMap<String, serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Health metrics
    pub metrics: HealthMetrics,
}

/// Health metrics collected during checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Resource usage during check
    pub resource_usage: Option<ResourceUsage>,
    /// Connection status
    pub connection_status: ConnectionStatus,
    /// Process status
    pub process_status: ProcessStatus,
    /// Last successful check timestamp
    pub last_successful_check: Option<SystemTime>,
    /// Consecutive failures
    pub consecutive_failures: u32,
}

/// Connection status for IPC checks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connection is healthy
    Healthy,
    /// Connection is slow
    Slow,
    /// Connection is intermittent
    Intermittent,
    /// Connection is broken
    Broken,
    /// Connection timeout
    Timeout,
    /// Connection refused
    Refused,
}

/// Process status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is running normally
    Running,
    /// Process is sleeping
    Sleeping,
    /// Process is stopped
    Stopped,
    /// Process is zombie
    Zombie,
    /// Process is crashed
    Crashed,
    /// Process is unknown
    Unknown,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfiguration {
    /// Check interval
    pub interval: Duration,
    /// Check timeout
    pub timeout: Duration,
    /// Unhealthy threshold (consecutive failures)
    pub unhealthy_threshold: u32,
    /// Check strategies
    pub strategies: Vec<HealthCheckStrategy>,
    /// Recovery configuration
    pub recovery: RecoveryConfig,
    /// Enable adaptive checking
    pub adaptive_checking: bool,
    /// Parallel check limit
    pub max_parallel_checks: usize,
    /// Check jitter (to avoid thundering herd)
    pub jitter_percentage: f64,
}

/// Health-based recovery action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryAction {
    /// No action
    None,
    /// Restart the plugin
    Restart,
    /// Stop the plugin
    Stop,
    /// Disable the plugin
    Disable,
    /// Send notification
    Notify,
    /// Run custom recovery script
    CustomScript(String),
    /// Scale resources
    ScaleResources,
    /// Migrate to different host
    Migrate,
}

/// Health status history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatusHistory {
    /// Instance ID
    pub instance_id: String,
    /// Status history (oldest first)
    pub history: VecDeque<HealthStatusEntry>,
    /// Current status
    pub current_status: PluginHealthStatus,
    /// Status changed timestamp
    pub last_status_change: SystemTime,
    /// Total uptime duration
    pub total_uptime: Duration,
    /// Total downtime duration
    pub total_downtime: Duration,
}

/// Health status entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatusEntry {
    /// Status
    pub status: PluginHealthStatus,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Duration of this status
    pub duration: Duration,
    /// Reason for status change
    pub reason: Option<String>,
}

/// Health statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatistics {
    /// Total health checks performed
    pub total_checks: u64,
    /// Successful checks
    pub successful_checks: u64,
    /// Failed checks
    pub failed_checks: u64,
    /// Average check duration
    pub average_check_duration: Duration,
    /// Uptime percentage
    pub uptime_percentage: f64,
    /// Mean time between failures
    pub mean_time_between_failures: Option<Duration>,
    /// Mean time to recovery
    pub mean_time_to_recovery: Option<Duration>,
    /// Last successful check
    pub last_successful_check: Option<SystemTime>,
    /// Last failure
    pub last_failure: Option<SystemTime>,
}

/// ============================================================================
/// HEALTH CHECKER IMPLEMENTATION
/// ============================================================================

/// Plugin health checker
pub struct HealthChecker {
    /// Health checking configuration
    config: HealthCheckConfiguration,
    /// Active health check data
    health_data: Arc<RwLock<HashMap<String, PluginHealthData>>>,
    /// Event emitter for health events
    event_emitter: EventEmitter,
    /// Shutdown channel receiver
    shutdown_rx: Option<mpsc::Receiver<()>>,
    /// Shutdown channel sender
    shutdown_tx: mpsc::Sender<()>,
    /// Semaphore for limiting parallel health checks
    check_semaphore: Arc<Semaphore>,
    /// Health check strategies registry
    strategy_registry: HealthCheckStrategyRegistry,
}

/// Health data for a specific plugin
#[derive(Debug)]
struct PluginHealthData {
    /// Plugin instance reference
    instance: PluginInstance,
    /// Current health status
    current_status: PluginHealthStatus,
    /// Health status history
    status_history: VecDeque<HealthStatusEntry>,
    /// Last health check timestamp
    last_check: Option<SystemTime>,
    /// Check statistics
    statistics: HealthStatistics,
    /// Recovery state
    recovery_state: RecoveryState,
    /// Check schedule
    schedule: HealthCheckSchedule,
    /// Active health checks
    active_checks: HashMap<HealthCheckType, Instant>,
}

/// Recovery state tracking
#[derive(Debug, Clone)]
struct RecoveryState {
    /// Current recovery attempt count
    attempt_count: u32,
    /// Last recovery action
    last_action: Option<RecoveryAction>,
    /// Last recovery timestamp
    last_recovery: Option<SystemTime>,
    /// Recovery backoff multiplier
    backoff_multiplier: f64,
    /// Recovery cooldown period
    cooldown_until: Option<SystemTime>,
}

/// Health check schedule
#[derive(Debug, Clone)]
struct HealthCheckSchedule {
    /// Next check time
    next_check: Instant,
    /// Check interval
    interval: Duration,
    /// Last check completed
    last_completed: Option<SystemTime>,
    /// Jitter for next check
    jitter_applied: bool,
}

/// Health check strategy registry
#[derive(Debug)]
struct HealthCheckStrategyRegistry {
    /// Registered strategies
    strategies: HashMap<String, Box<dyn HealthCheckStrategyImpl>>,
}

/// Health check strategy implementation trait
#[async_trait::async_trait]
trait HealthCheckStrategyImpl: Send + Sync {
    /// Get the strategy type
    fn strategy_type(&self) -> HealthCheckType;

    /// Execute the health check
    async fn execute_check(&self, instance: &PluginInstance) -> PluginResult<HealthCheckResult>;

    /// Validate strategy configuration
    fn validate_config(&self, config: &HashMap<String, serde_json::Value>) -> PluginResult<()>;

    /// Get default configuration
    fn default_config(&self) -> HashMap<String, serde_json::Value>;
}

/// Process health check strategy
struct ProcessHealthCheckStrategy;

/// IPC health check strategy
struct IpcHealthCheckStrategy;

/// Resource health check strategy
struct ResourceHealthCheckStrategy;

/// Custom health check strategy
struct CustomHealthCheckStrategy {
    /// Custom check endpoint URL
    endpoint_url: Option<String>,
    /// Custom check script path
    script_path: Option<String>,
    /// Custom check timeout
    timeout: Duration,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(config: HealthCheckConfiguration, event_emitter: EventEmitter) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // Initialize strategy registry with default strategies
        let mut strategy_registry = HealthCheckStrategyRegistry::new();
        strategy_registry.register_strategy(Box::new(ProcessHealthCheckStrategy));
        strategy_registry.register_strategy(Box::new(IpcHealthCheckStrategy));
        strategy_registry.register_strategy(Box::new(ResourceHealthCheckStrategy));

        Self {
            config,
            health_data: Arc::new(RwLock::new(HashMap::new())),
            event_emitter,
            shutdown_rx: Some(shutdown_rx),
            shutdown_tx,
            check_semaphore: Arc::new(Semaphore::new(config.max_parallel_checks)),
            strategy_registry,
        }
    }

    /// Start health checking for a plugin instance
    pub async fn start_health_checking(&self, instance: PluginInstance) -> PluginResult<()> {
        let instance_id = instance.instance_id.clone();

        // Initialize health data
        let health_data = PluginHealthData {
            instance: instance.clone(),
            current_status: PluginHealthStatus::Unknown,
            status_history: VecDeque::new(),
            last_check: None,
            statistics: HealthStatistics::default(),
            recovery_state: RecoveryState::default(),
            schedule: HealthCheckSchedule {
                next_check: Instant::now() + self.calculate_initial_jitter(),
                interval: self.config.interval,
                last_completed: None,
                jitter_applied: false,
            },
            active_checks: HashMap::new(),
        };

        // Add to health data
        {
            let mut data = self.health_data.write().await;
            data.insert(instance_id.clone(), health_data);
        }

        // Emit health checking started event
        self.event_emitter.emit(Event::HealthCheck(
            HealthCheckEvent::ScheduleUpdated {
                instance_id: instance_id.clone(),
                interval: self.config.interval,
                strategies: self.config.strategies.iter().map(|s| s.name.clone()).collect(),
            }
        )).await?;

        tracing::info!("Started health checking for plugin instance: {}", instance_id);
        Ok(())
    }

    /// Stop health checking for a plugin instance
    pub async fn stop_health_checking(&self, instance_id: &str) -> PluginResult<()> {
        // Remove from active health checking
        {
            let mut data = self.health_data.write().await;
            data.remove(instance_id);
        }

        tracing::info!("Stopped health checking for plugin instance: {}", instance_id);
        Ok(())
    }

    /// Get current health status for a plugin
    pub async fn get_health_status(&self, instance_id: &str) -> PluginResult<Option<PluginHealthStatus>> {
        let data = self.health_data.read().await;
        Ok(data.get(instance_id).map(|d| d.current_status.clone()))
    }

    /// Get health status history for a plugin
    pub async fn get_health_history(&self, instance_id: &str) -> PluginResult<Option<HealthStatusHistory>> {
        let data = self.health_data.read().await;
        if let Some(plugin_data) = data.get(instance_id) {
            let history = HealthStatusHistory {
                instance_id: instance_id.to_string(),
                history: plugin_data.status_history.clone(),
                current_status: plugin_data.current_status.clone(),
                last_status_change: SystemTime::now(), // Would track actual changes
                total_uptime: Duration::from_secs(0),   // Would calculate from history
                total_downtime: Duration::from_secs(0), // Would calculate from history
            };
            Ok(Some(history))
        } else {
            Ok(None)
        }
    }

    /// Get health statistics for a plugin
    pub async fn get_health_statistics(&self, instance_id: &str) -> PluginResult<Option<HealthStatistics>> {
        let data = self.health_data.read().await;
        Ok(data.get(instance_id).map(|d| d.statistics.clone()))
    }

    /// Trigger an immediate health check for a plugin
    pub async fn trigger_health_check(&self, instance_id: &str, check_types: Option<Vec<HealthCheckType>>) -> PluginResult<Vec<HealthCheckResult>> {
        let data = self.health_data.read().await;
        if let Some(plugin_data) = data.get(instance_id) {
            let strategies_to_run = if let Some(types) = check_types {
                self.config.strategies.iter()
                    .filter(|s| types.contains(&s.strategy_type))
                    .cloned()
                    .collect()
            } else {
                self.config.strategies.clone()
            };

            let mut results = Vec::new();

            for strategy in strategies_to_run {
                if let Some(strategy_impl) = self.strategy_registry.get_strategy(&strategy.strategy_type) {
                    let result = self.execute_single_check(&plugin_data.instance, strategy_impl).await?;
                    results.push(result);
                }
            }

            Ok(results)
        } else {
            Err(PluginError::NotFound(format!("Plugin instance not found: {}", instance_id)))
        }
    }

    /// Start the health checking loop
    pub async fn start_health_checking_loop(&mut self) -> PluginResult<()> {
        let mut shutdown_rx = self.shutdown_rx.take()
            .ok_or_else(|| PluginError::internal("Health checking loop already started"))?;

        let health_data = self.health_data.clone();
        let config = self.config.clone();
        let event_emitter = self.event_emitter.clone();
        let check_semaphore = self.check_semaphore.clone();
        let strategy_registry = self.strategy_registry.clone();

        tracing::info!("Starting health checking loop with interval: {:?}", config.interval);

        let health_task = async move {
            let mut check_interval = tokio::time::interval(Duration::from_secs(1)); // Check every second for scheduled checks

            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        // Check for scheduled health checks
                        let now = Instant::now();
                        let instances_to_check = {
                            let data = health_data.read().await;
                            data.iter()
                                .filter(|(_, health_data)| now >= health_data.schedule.next_check)
                                .map(|(id, _)| id.clone())
                                .collect::<Vec<_>>()
                        };

                        // Process scheduled checks in parallel with semaphore limit
                        let mut tasks = Vec::new();

                        for instance_id in instances_to_check {
                            let health_data = health_data.clone();
                            let config = config.clone();
                            let event_emitter = event_emitter.clone();
                            let semaphore = check_semaphore.clone();
                            let strategy_registry = strategy_registry.clone();

                            let task = tokio::spawn(async move {
                                let _permit = semaphore.acquire().await;
                                if let Err(e) = Self::perform_scheduled_health_checks(
                                    instance_id,
                                    health_data,
                                    config,
                                    event_emitter,
                                    strategy_registry,
                                ).await {
                                    tracing::error!("Error performing scheduled health checks: {}", e);
                                }
                            });

                            tasks.push(task);
                        }

                        // Wait for all scheduled checks to complete
                        for task in tasks {
                            let _ = task.await;
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Health checking loop shutting down");
                        break;
                    }
                }
            }
        };

        tokio::spawn(health_task);
        Ok(())
    }

    /// Stop the health checking loop
    pub async fn stop_health_checking_loop(&self) -> PluginResult<()> {
        let _ = self.shutdown_tx.send(()).await;
        Ok(())
    }

    /// Perform scheduled health checks for a plugin instance
    async fn perform_scheduled_health_checks(
        instance_id: String,
        health_data: Arc<RwLock<HashMap<String, PluginHealthData>>>,
        config: HealthCheckConfiguration,
        event_emitter: EventEmitter,
        strategy_registry: HealthCheckStrategyRegistry,
    ) -> PluginResult<()> {
        // Get plugin data
        let (instance, strategies) = {
            let data = health_data.read().await;
            if let Some(plugin_data) = data.get(&instance_id) {
                (plugin_data.instance.clone(), config.strategies.clone())
            } else {
                return Ok(());
            }
        };

        // Execute all configured strategies
        let mut results = Vec::new();
        for strategy in &strategies {
            if let Some(strategy_impl) = strategy_registry.get_strategy(&strategy.strategy_type) {
                match Self::execute_single_check(&instance, strategy_impl).await {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        tracing::error!("Health check failed for {}: {}", instance_id, e);
                        // Emit check failed event
                        let _ = event_emitter.emit(Event::HealthCheck(
                            HealthCheckEvent::CheckFailed {
                                instance_id: instance_id.clone(),
                                check_type: strategy.strategy_type.clone(),
                                error: e.to_string(),
                                duration: Duration::from_millis(0),
                            }
                        )).await;
                    }
                }
            }
        }

        // Process results and update health status
        if !results.is_empty() {
            Self::process_health_check_results(
                instance_id,
                results,
                health_data,
                config,
                event_emitter,
            ).await?;
        }

        // Schedule next check
        {
            let mut data = health_data.write().await;
            if let Some(plugin_data) = data.get_mut(&instance_id) {
                let next_interval = Self::calculate_next_check_interval(
                    &plugin_data.recovery_state,
                    plugin_data.current_status,
                    config.interval,
                );

                plugin_data.schedule.next_check = Instant::now() + next_interval;
                plugin_data.schedule.last_completed = Some(SystemTime::now());
            }
        }

        Ok(())
    }

    /// Execute a single health check
    async fn execute_single_check(
        instance: &PluginInstance,
        strategy: &dyn HealthCheckStrategyImpl,
    ) -> PluginResult<HealthCheckResult> {
        let start_time = Instant::now();

        let result = strategy.execute_check(instance).await;
        let duration = start_time.elapsed();

        match result {
            Ok(mut check_result) => {
                check_result.duration = duration;
                Ok(check_result)
            }
            Err(e) => {
                Ok(HealthCheckResult {
                    check_type: strategy.strategy_type(),
                    status: PluginHealthStatus::Unhealthy,
                    duration,
                    timestamp: SystemTime::now(),
                    details: HashMap::new(),
                    error: Some(e.to_string()),
                    metrics: HealthMetrics {
                        response_time_ms: duration.as_millis() as u64,
                        resource_usage: None,
                        connection_status: ConnectionStatus::Broken,
                        process_status: ProcessStatus::Unknown,
                        last_successful_check: None,
                        consecutive_failures: 1,
                    },
                })
            }
        }
    }

    /// Process health check results and update status
    async fn process_health_check_results(
        instance_id: String,
        results: Vec<HealthCheckResult>,
        health_data: Arc<RwLock<HashMap<String, PluginHealthData>>>,
        config: HealthCheckConfiguration,
        event_emitter: EventEmitter,
    ) -> PluginResult<()> {
        // Determine overall health status
        let overall_status = Self::determine_overall_health(&results);

        // Update health data
        let (old_status, should_trigger_recovery) = {
            let mut data = health_data.write().await;
            if let Some(plugin_data) = data.get_mut(&instance_id) {
                let old_status = plugin_data.current_status.clone();
                plugin_data.current_status = overall_status.clone();

                // Update statistics
                plugin_data.statistics.total_checks += 1;
                let successful_checks = results.iter().filter(|r| r.status == PluginHealthStatus::Healthy).count();
                if successful_checks == results.len() {
                    plugin_data.statistics.successful_checks += 1;
                    plugin_data.statistics.last_successful_check = Some(SystemTime::now());
                } else {
                    plugin_data.statistics.failed_checks += 1;
                    plugin_data.statistics.last_failure = Some(SystemTime::now());
                }

                // Update status history
                let status_entry = HealthStatusEntry {
                    status: overall_status.clone(),
                    timestamp: SystemTime::now(),
                    duration: Duration::from_secs(0), // Would track actual duration
                    reason: Some("Health check completed".to_string()),
                };
                plugin_data.status_history.push_back(status_entry);

                // Trim history if needed
                if plugin_data.status_history.len() > 100 {
                    plugin_data.status_history.pop_front();
                }

                // Check if recovery should be triggered
                let should_trigger_recovery = Self::should_trigger_recovery(
                    &old_status,
                    &overall_status,
                    &plugin_data.recovery_state,
                    &config,
                );

                (old_status, should_trigger_recovery)
            } else {
                return Ok(());
            }
        };

        // Emit status change event if status changed
        if old_status != overall_status {
            let _ = event_emitter.emit(Event::HealthCheck(
                HealthCheckEvent::HealthStatusChanged {
                    instance_id: instance_id.clone(),
                    old_status,
                    new_status: overall_status.clone(),
                    reason: "Health check results".to_string(),
                }
            )).await;
        }

        // Trigger recovery if needed
        if should_trigger_recovery {
            let recovery_action = Self::determine_recovery_action(&overall_status, &config);
            let _ = event_emitter.emit(Event::HealthCheck(
                HealthCheckEvent::RecoveryActionTriggered {
                    instance_id: instance_id.clone(),
                    action: recovery_action.clone(),
                    reason: "Health status degraded".to_string(),
                }
            )).await;

            // Execute recovery action
            Self::execute_recovery_action(&instance_id, &recovery_action, &health_data).await?;
        }

        Ok(())
    }

    /// Determine overall health status from check results
    fn determine_overall_health(results: &[HealthCheckResult]) -> PluginHealthStatus {
        if results.is_empty() {
            return PluginHealthStatus::Unknown;
        }

        let healthy_count = results.iter().filter(|r| r.status == PluginHealthStatus::Healthy).count();
        let degraded_count = results.iter().filter(|r| r.status == PluginHealthStatus::Degraded).count();
        let unhealthy_count = results.iter().filter(|r| r.status == PluginHealthStatus::Unhealthy).count();

        // If any check is unhealthy, overall status is unhealthy
        if unhealthy_count > 0 {
            return PluginHealthStatus::Unhealthy;
        }

        // If any check is degraded, overall status is degraded
        if degraded_count > 0 {
            return PluginHealthStatus::Degraded;
        }

        // If all checks are healthy, overall status is healthy
        if healthy_count == results.len() {
            return PluginHealthStatus::Healthy;
        }

        // Default to unknown
        PluginHealthStatus::Unknown
    }

    /// Check if recovery should be triggered
    fn should_trigger_recovery(
        old_status: &PluginHealthStatus,
        new_status: &PluginHealthStatus,
        recovery_state: &RecoveryState,
        config: &HealthCheckConfiguration,
    ) -> bool {
        // Don't trigger if in cooldown
        if let Some(cooldown_until) = recovery_state.cooldown_until {
            if SystemTime::now() < cooldown_until {
                return false;
            }
        }

        // Check if recovery is enabled
        if !config.recovery.enabled {
            return false;
        }

        // Check if we've exceeded max attempts
        if recovery_state.attempt_count >= config.recovery.max_restart_attempts {
            return false;
        }

        // Trigger recovery based on status change
        match (old_status, new_status) {
            (PluginHealthStatus::Healthy | PluginHealthStatus::Degraded, PluginHealthStatus::Unhealthy) => true,
            (PluginHealthStatus::Unknown, PluginHealthStatus::Unhealthy) => true,
            _ => false,
        }
    }

    /// Determine recovery action based on health status
    fn determine_recovery_action(status: &PluginHealthStatus, config: &HealthCheckConfiguration) -> RecoveryAction {
        match status {
            PluginHealthStatus::Unhealthy => RecoveryAction::Restart,
            PluginHealthStatus::Degraded => RecoveryAction::Notify,
            _ => RecoveryAction::None,
        }
    }

    /// Execute recovery action
    async fn execute_recovery_action(
        instance_id: &str,
        action: &RecoveryAction,
        health_data: &Arc<RwLock<HashMap<String, PluginHealthData>>>,
    ) -> PluginResult<()> {
        match action {
            RecoveryAction::Restart => {
                // Update recovery state
                {
                    let mut data = health_data.write().await;
                    if let Some(plugin_data) = data.get_mut(instance_id) {
                        plugin_data.recovery_state.attempt_count += 1;
                        plugin_data.recovery_state.last_recovery = Some(SystemTime::now());
                        plugin_data.recovery_state.last_action = Some(action.clone());
                    }
                }

                // In a real implementation, this would trigger plugin restart
                tracing::info!("Triggering restart for plugin instance: {}", instance_id);
            }
            RecoveryAction::Stop => {
                tracing::info!("Stopping plugin instance: {}", instance_id);
            }
            RecoveryAction::Disable => {
                tracing::info!("Disabling plugin instance: {}", instance_id);
            }
            RecoveryAction::Notify => {
                tracing::warn!("Plugin instance {} health degraded", instance_id);
            }
            RecoveryAction::CustomScript(script) => {
                tracing::info!("Executing custom recovery script for {}: {}", instance_id, script);
            }
            _ => {}
        }

        Ok(())
    }

    /// Calculate initial jitter to avoid thundering herd
    fn calculate_initial_jitter(&self) -> Duration {
        if self.config.jitter_percentage <= 0.0 {
            return Duration::ZERO;
        }

        let jitter_ms = (self.config.interval.as_millis() as f64 * self.config.jitter_percentage / 100.0) as u64;
        let random_jitter = rand::random::<u64>() % jitter_ms;
        Duration::from_millis(random_jitter)
    }

    /// Calculate next check interval based on recovery state and current status
    fn calculate_next_check_interval(
        recovery_state: &RecoveryState,
        current_status: PluginHealthStatus,
        base_interval: Duration,
    ) -> Duration {
        match current_status {
            PluginHealthStatus::Unhealthy => {
                // Check more frequently when unhealthy
                std::cmp::min(base_interval / 2, Duration::from_secs(5))
            }
            PluginHealthStatus::Degraded => {
                // Check slightly more frequently when degraded
                base_interval * 3 / 4
            }
            _ => {
                // Apply backoff if recovering
                if recovery_state.attempt_count > 0 {
                    let backoff_multiplier = recovery_state.backoff_multiplier.max(1.0);
                    Duration::from_millis((base_interval.as_millis() as f64 * backoff_multiplier) as u64)
                } else {
                    base_interval
                }
            }
        }
    }
}

impl HealthCheckStrategyRegistry {
    fn new() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    fn register_strategy(&mut self, strategy: Box<dyn HealthCheckStrategyImpl>) {
        let strategy_type = strategy.strategy_type();
        self.strategies.insert(format!("{:?}", strategy_type), strategy);
    }

    fn get_strategy(&self, strategy_type: &HealthCheckType) -> Option<&dyn HealthCheckStrategyImpl> {
        self.strategies.get(&format!("{:?}", strategy_type)).map(|s| s.as_ref())
    }
}

impl Clone for HealthCheckStrategyRegistry {
    fn clone(&self) -> Self {
        // This is a simplified clone - in practice, strategies would need to be clonable
        Self {
            strategies: HashMap::new(),
        }
    }
}

// Default implementations for recovery state and health statistics

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            attempt_count: 0,
            last_action: None,
            last_recovery: None,
            backoff_multiplier: 1.0,
            cooldown_until: None,
        }
    }
}

impl Default for HealthStatistics {
    fn default() -> Self {
        Self {
            total_checks: 0,
            successful_checks: 0,
            failed_checks: 0,
            average_check_duration: Duration::ZERO,
            uptime_percentage: 100.0,
            mean_time_between_failures: None,
            mean_time_to_recovery: None,
            last_successful_check: None,
            last_failure: None,
        }
    }
}

/// ============================================================================
/// HEALTH CHECK STRATEGY IMPLEMENTATIONS
/// ============================================================================

#[async_trait::async_trait]
impl HealthCheckStrategyImpl for ProcessHealthCheckStrategy {
    fn strategy_type(&self) -> HealthCheckType {
        HealthCheckType::Process
    }

    async fn execute_check(&self, instance: &PluginInstance) -> PluginResult<HealthCheckResult> {
        let start_time = Instant::now();

        // Check if process is running
        let is_running = if let Some(pid) = instance.pid {
            // In a real implementation, this would check if the process is actually running
            true // Placeholder
        } else {
            false
        };

        let status = if is_running {
            PluginHealthStatus::Healthy
        } else {
            PluginHealthStatus::Unhealthy
        };

        let duration = start_time.elapsed();

        Ok(HealthCheckResult {
            check_type: HealthCheckType::Process,
            status,
            duration,
            timestamp: SystemTime::now(),
            details: HashMap::new(),
            error: None,
            metrics: HealthMetrics {
                response_time_ms: duration.as_millis() as u64,
                resource_usage: None,
                connection_status: ConnectionStatus::Healthy,
                process_status: if is_running { ProcessStatus::Running } else { ProcessStatus::Crashed },
                last_successful_check: Some(SystemTime::now()),
                consecutive_failures: 0,
            },
        })
    }

    fn validate_config(&self, _config: &HashMap<String, serde_json::Value>) -> PluginResult<()> {
        Ok(())
    }

    fn default_config(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait::async_trait]
impl HealthCheckStrategyImpl for IpcHealthCheckStrategy {
    fn strategy_type(&self) -> HealthCheckType {
        HealthCheckType::Ping
    }

    async fn execute_check(&self, instance: &PluginInstance) -> PluginResult<HealthCheckResult> {
        let start_time = Instant::now();

        // Simulate IPC health check
        // In a real implementation, this would send a ping message over IPC
        let is_responsive = true; // Placeholder

        let status = if is_responsive {
            PluginHealthStatus::Healthy
        } else {
            PluginHealthStatus::Unhealthy
        };

        let duration = start_time.elapsed();

        Ok(HealthCheckResult {
            check_type: HealthCheckType::Ping,
            status,
            duration,
            timestamp: SystemTime::now(),
            details: HashMap::new(),
            error: None,
            metrics: HealthMetrics {
                response_time_ms: duration.as_millis() as u64,
                resource_usage: None,
                connection_status: if is_responsive { ConnectionStatus::Healthy } else { ConnectionStatus::Broken },
                process_status: ProcessStatus::Running,
                last_successful_check: Some(SystemTime::now()),
                consecutive_failures: 0,
            },
        })
    }

    fn validate_config(&self, _config: &HashMap<String, serde_json::Value>) -> PluginResult<()> {
        Ok(())
    }

    fn default_config(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait::async_trait]
impl HealthCheckStrategyImpl for ResourceHealthCheckStrategy {
    fn strategy_type(&self) -> HealthCheckType {
        HealthCheckType::Resource
    }

    async fn execute_check(&self, instance: &PluginInstance) -> PluginResult<HealthCheckResult> {
        let start_time = Instant::now();

        // Check resource usage against limits
        let current_usage = &instance.resource_usage;
        let limits = &instance.resource_limits;

        let mut is_healthy = true;
        let mut details = HashMap::new();

        // Check memory usage
        if let Some(max_memory) = limits.max_memory_bytes {
            let memory_usage_percent = (current_usage.memory_bytes as f64 / max_memory as f64) * 100.0;
            details.insert("memory_usage_percent".to_string(), serde_json::Value::Number(memory_usage_percent.into()));
            if memory_usage_percent > 90.0 {
                is_healthy = false;
            }
        }

        // Check CPU usage
        if let Some(max_cpu) = limits.max_cpu_percentage {
            details.insert("cpu_usage_percent".to_string(), serde_json::Value::Number(current_usage.cpu_percentage.into()));
            if current_usage.cpu_percentage > max_cpu {
                is_healthy = false;
            }
        }

        let status = if is_healthy {
            PluginHealthStatus::Healthy
        } else {
            PluginHealthStatus::Degraded
        };

        let duration = start_time.elapsed();

        Ok(HealthCheckResult {
            check_type: HealthCheckType::Resource,
            status,
            duration,
            timestamp: SystemTime::now(),
            details,
            error: None,
            metrics: HealthMetrics {
                response_time_ms: duration.as_millis() as u64,
                resource_usage: Some(current_usage.clone()),
                connection_status: ConnectionStatus::Healthy,
                process_status: ProcessStatus::Running,
                last_successful_check: Some(SystemTime::now()),
                consecutive_failures: 0,
            },
        })
    }

    fn validate_config(&self, _config: &HashMap<String, serde_json::Value>) -> PluginResult<()> {
        Ok(())
    }

    fn default_config(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

#[async_trait::async_trait]
impl HealthCheckStrategyImpl for CustomHealthCheckStrategy {
    fn strategy_type(&self) -> HealthCheckType {
        HealthCheckType::Custom
    }

    async fn execute_check(&self, _instance: &PluginInstance) -> PluginResult<HealthCheckResult> {
        let start_time = Instant::now();

        // Implement custom health check logic
        // This could involve HTTP requests, script execution, etc.

        let duration = start_time.elapsed();

        Ok(HealthCheckResult {
            check_type: HealthCheckType::Custom,
            status: PluginHealthStatus::Healthy,
            duration,
            timestamp: SystemTime::now(),
            details: HashMap::new(),
            error: None,
            metrics: HealthMetrics {
                response_time_ms: duration.as_millis() as u64,
                resource_usage: None,
                connection_status: ConnectionStatus::Healthy,
                process_status: ProcessStatus::Running,
                last_successful_check: Some(SystemTime::now()),
                consecutive_failures: 0,
            },
        })
    }

    fn validate_config(&self, _config: &HashMap<String, serde_json::Value>) -> PluginResult<()> {
        Ok(())
    }

    fn default_config(&self) -> HashMap<String, serde_json::Value> {
        let mut config = HashMap::new();
        config.insert("timeout_ms".to_string(), serde_json::Value::Number(5000.into()));
        config
    }
}

/// ============================================================================
/// HEALTH CHECKING SERVICE TRAIT
/// ============================================================================

/// Health checking service trait
#[async_trait::async_trait]
pub trait HealthCheckingService: Send + Sync {
    /// Start health checking for a plugin
    async fn start_health_checking(&self, instance: PluginInstance) -> PluginResult<()>;

    /// Stop health checking for a plugin
    async fn stop_health_checking(&self, instance_id: &str) -> PluginResult<()>;

    /// Get current health status
    async fn get_health_status(&self, instance_id: &str) -> PluginResult<Option<PluginHealthStatus>>;

    /// Get health history
    async fn get_health_history(&self, instance_id: &str) -> PluginResult<Option<HealthStatusHistory>>;

    /// Get health statistics
    async fn get_health_statistics(&self, instance_id: &str) -> PluginResult<Option<HealthStatistics>>;

    /// Trigger immediate health check
    async fn trigger_health_check(&self, instance_id: &str, check_types: Option<Vec<HealthCheckType>>) -> PluginResult<Vec<HealthCheckResult>>;
}

#[async_trait::async_trait]
impl HealthCheckingService for HealthChecker {
    async fn start_health_checking(&self, instance: PluginInstance) -> PluginResult<()> {
        self.start_health_checking(instance).await
    }

    async fn stop_health_checking(&self, instance_id: &str) -> PluginResult<()> {
        self.stop_health_checking(instance_id).await
    }

    async fn get_health_status(&self, instance_id: &str) -> PluginResult<Option<PluginHealthStatus>> {
        self.get_health_status(instance_id).await
    }

    async fn get_health_history(&self, instance_id: &str) -> PluginResult<Option<HealthStatusHistory>> {
        self.get_health_history(instance_id).await
    }

    async fn get_health_statistics(&self, instance_id: &str) -> PluginResult<Option<HealthStatistics>> {
        self.get_health_statistics(instance_id).await
    }

    async fn trigger_health_check(&self, instance_id: &str, check_types: Option<Vec<HealthCheckType>>) -> PluginResult<Vec<HealthCheckResult>> {
        self.trigger_health_check(instance_id, check_types).await
    }
}

impl Default for HealthCheckConfiguration {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            strategies: vec![
                HealthCheckStrategy {
                    name: "process".to_string(),
                    strategy_type: HealthCheckType::Process,
                    config: HashMap::new(),
                    enabled: true,
                },
                HealthCheckStrategy {
                    name: "ping".to_string(),
                    strategy_type: HealthCheckType::Ping,
                    config: HashMap::new(),
                    enabled: true,
                },
                HealthCheckStrategy {
                    name: "resource".to_string(),
                    strategy_type: HealthCheckType::Resource,
                    config: HashMap::new(),
                    enabled: true,
                },
            ],
            recovery: RecoveryConfig::default(),
            adaptive_checking: true,
            max_parallel_checks: 10,
            jitter_percentage: 10.0,
        }
    }
}