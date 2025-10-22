//! # Resource Manager
//!
//! This module implements the ResourceManager which monitors plugin resource usage,
//! enforces limits, and manages resource allocation across the plugin system.

use super::config::{ResourceManagementConfig, ResourceEnforcementConfig, EnforcementStrategy, LimitExceededAction};
use super::error::{PluginError, PluginResult, ErrorContext, ErrorMetrics};
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
/// RESOURCE MANAGER TRAIT
/// ============================================================================

#[async_trait]
pub trait ResourceManager: Send + Sync {
    /// Start resource monitoring
    async fn start(&mut self) -> PluginResult<()>;

    /// Stop resource monitoring
    async fn stop(&mut self) -> PluginResult<()>;

    /// Register a plugin instance for monitoring
    async fn register_instance(&mut self, instance_id: String, limits: ResourceLimits) -> PluginResult<()>;

    /// Unregister a plugin instance
    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()>;

    /// Get current resource usage for an instance
    async fn get_instance_usage(&self, instance_id: &str) -> PluginResult<ResourceUsage>;

    /// Get current resource usage for all instances
    async fn get_all_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>>;

    /// Get global resource usage
    async fn get_global_usage(&self) -> PluginResult<ResourceUsage>;

    /// Update resource limits for an instance
    async fn update_instance_limits(&mut self, instance_id: &str, limits: ResourceLimits) -> PluginResult<()>;

    /// Check if instance is within limits
    async fn check_instance_limits(&self, instance_id: &str) -> PluginResult<bool>;

    /// Get resource metrics
    async fn get_metrics(&self) -> PluginResult<ResourceMetrics>;

    /// Subscribe to resource events
    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<ResourceEvent>;

    /// Force cleanup of resources for an instance
    async fn cleanup_instance_resources(&mut self, instance_id: &str) -> PluginResult<()>;
}

/// ============================================================================
/// RESOURCE EVENTS
/// ============================================================================

/// Resource event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceEvent {
    /// Resource limit exceeded
    LimitExceeded {
        instance_id: String,
        resource_type: ResourceType,
        current_value: f64,
        limit: f64,
        action_taken: LimitExceededAction,
    },
    /// Resource usage anomaly detected
    AnomalyDetected {
        instance_id: String,
        resource_type: ResourceType,
        anomaly_type: AnomalyType,
        severity: AnomalySeverity,
        description: String,
    },
    /// Resource pressure warning
    PressureWarning {
        resource_type: ResourceType,
        usage_percentage: f64,
        available_capacity: f64,
    },
    /// Resource allocation changed
    AllocationChanged {
        instance_id: String,
        old_limits: ResourceLimits,
        new_limits: ResourceLimits,
    },
    /// Instance registered
    InstanceRegistered {
        instance_id: String,
        limits: ResourceLimits,
    },
    /// Instance unregistered
    InstanceUnregistered {
        instance_id: String,
    },
    /// Global limit exceeded
    GlobalLimitExceeded {
        resource_type: ResourceType,
        current_value: f64,
        limit: f64,
    },
    /// Resource cleanup completed
    CleanupCompleted {
        instance_id: String,
        resources_freed: Vec<ResourceType>,
        bytes_freed: u64,
    },
}

/// Resource type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// CPU usage
    Cpu,
    /// Memory usage
    Memory,
    /// Disk usage
    Disk,
    /// Network usage
    Network,
    /// File descriptors
    FileDescriptors,
    /// Processes
    Processes,
    /// Threads
    Threads,
    /// Custom resource
    Custom(String),
}

/// Anomaly type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnomalyType {
    /// Sudden spike in usage
    Spike,
    /// Gradual increase
    GradualIncrease,
    /// Unusual pattern
    UnusualPattern,
    /// Resource leak
    ResourceLeak,
    /// Deadlock detected
    Deadlock,
}

/// Anomaly severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnomalySeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// Total resource usage
    pub total_usage: ResourceUsage,
    /// Usage by instance
    pub usage_by_instance: HashMap<String, ResourceUsage>,
    /// Usage by resource type
    pub usage_by_type: HashMap<ResourceType, f64>,
    /// Peak usage in last hour
    pub peak_usage: HashMap<ResourceType, f64>,
    /// Average usage in last hour
    pub average_usage: HashMap<ResourceType, f64>,
    /// Resource pressure indicators
    pub pressure_indicators: HashMap<ResourceType, ResourcePressure>,
    /// Anomaly count
    pub anomaly_count: u64,
    /// Limit violation count
    pub limit_violation_count: u64,
    /// Metrics collection timestamp
    pub timestamp: SystemTime,
}

/// Resource pressure indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePressure {
    /// Current pressure level (0.0-1.0)
    pub pressure_level: f64,
    /// Pressure trend
    pub trend: PressureTrend,
    /// Time until limit breach (estimated)
    pub time_until_breach: Option<Duration>,
    /// Recommended actions
    pub recommended_actions: Vec<String>,
}

/// Pressure trend
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PressureTrend {
    /// Increasing
    Increasing,
    /// Decreasing
    Decreasing,
    /// Stable
    Stable,
}

/// ============================================================================
/// DEFAULT RESOURCE MANAGER
/// ============================================================================

/// Default implementation of ResourceManager
#[derive(Debug)]
pub struct DefaultResourceManager {
    /// Configuration
    config: Arc<ResourceManagementConfig>,
    /// Monitored instances
    instances: Arc<RwLock<HashMap<String, MonitoredInstance>>>,
    /// Global usage tracking
    global_usage: Arc<RwLock<ResourceUsage>>,
    /// Historical data
    historical_data: Arc<RwLock<HashMap<String, Vec<ResourceUsage>>>>,
    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<ResourceEvent>>>>,
    /// Metrics collector
    metrics: Arc<RwLock<ResourceMetrics>>,
    /// Anomaly detector
    anomaly_detector: Arc<AnomalyDetector>,
    /// Enforcement engine
    enforcement_engine: Arc<EnforcementEngine>,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Metrics collection handle
    metrics_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Information about a monitored instance
#[derive(Debug, Clone)]
struct MonitoredInstance {
    /// Instance ID
    instance_id: String,
    /// Resource limits
    limits: ResourceLimits,
    /// Current usage
    current_usage: ResourceUsage,
    /// Usage history
    usage_history: Vec<ResourceUsage>,
    /// Violation count
    violation_count: u64,
    /// Last violation time
    last_violation: Option<SystemTime>,
    /// Enforcement actions taken
    actions_taken: Vec<EnforcementAction>,
}

/// Enforcement action record
#[derive(Debug, Clone)]
struct EnforcementAction {
    /// Action taken
    action: LimitExceededAction,
    /// Resource type
    resource_type: ResourceType,
    /// Timestamp
    timestamp: SystemTime,
    /// Action result
    result: EnforcementResult,
}

/// Enforcement result
#[derive(Debug, Clone, PartialEq, Eq)]
enum EnforcementResult {
    /// Action succeeded
    Success,
    /// Action failed
    Failed(String),
    /// Action pending
    Pending,
}

impl DefaultResourceManager {
    /// Create a new resource manager
    pub fn new(config: ResourceManagementConfig) -> Self {
        Self {
            config: Arc::new(config),
            instances: Arc::new(RwLock::new(HashMap::new())),
            global_usage: Arc::new(RwLock::new(ResourceUsage::default())),
            historical_data: Arc::new(RwLock::new(HashMap::new())),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(ResourceMetrics {
                total_usage: ResourceUsage::default(),
                usage_by_instance: HashMap::new(),
                usage_by_type: HashMap::new(),
                peak_usage: HashMap::new(),
                average_usage: HashMap::new(),
                pressure_indicators: HashMap::new(),
                anomaly_count: 0,
                limit_violation_count: 0,
                timestamp: SystemTime::now(),
            })),
            anomaly_detector: Arc::new(AnomalyDetector::new()),
            enforcement_engine: Arc::new(EnforcementEngine::new()),
            running: Arc::new(RwLock::new(false)),
            metrics_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: ResourceEvent) {
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

    /// Collect resource metrics
    async fn collect_metrics(&self) {
        let interval = self.config.monitoring.interval;

        loop {
            tokio::time::sleep(interval).await;

            // Check if still running
            if !*self.running.read().await {
                break;
            }

            if let Err(e) = self.update_metrics().await {
                error!("Failed to update resource metrics: {}", e);
            }
        }
    }

    /// Update resource metrics
    async fn update_metrics(&self) -> PluginResult<()> {
        let instances = self.instances.read().await;
        let mut usage_by_instance = HashMap::new();
        let mut usage_by_type = HashMap::new();
        let mut total_usage = ResourceUsage::default();

        // Collect usage from all instances
        for (instance_id, instance) in instances.iter() {
            let usage = &instance.current_usage;
            usage_by_instance.insert(instance_id.clone(), usage.clone());

            // Aggregate by type
            usage_by_type.insert(ResourceType::Cpu, usage.cpu_percentage);
            usage_by_type.insert(ResourceType::Memory, usage.memory_bytes as f64);
            usage_by_type.insert(ResourceType::Disk, usage.disk_bytes as f64);
            usage_by_type.insert(ResourceType::Network, usage.network_bytes as f64);
            usage_by_type.insert(ResourceType::FileDescriptors, usage.open_files as f64);
            usage_by_type.insert(ResourceType::Threads, usage.active_threads as f64);

            // Aggregate totals
            total_usage.memory_bytes += usage.memory_bytes;
            total_usage.cpu_percentage = total_usage.cpu_percentage.max(usage.cpu_percentage);
            total_usage.disk_bytes += usage.disk_bytes;
            total_usage.network_bytes += usage.network_bytes;
            total_usage.open_files += usage.open_files;
            total_usage.active_threads += usage.active_threads;
            total_usage.child_processes += usage.child_processes;
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_usage = total_usage;
            metrics.usage_by_instance = usage_by_instance;
            metrics.usage_by_type = usage_by_type;
            metrics.timestamp = SystemTime::now();
        }

        // Update global usage
        {
            let mut global_usage = self.global_usage.write().await;
            *global_usage = total_usage;
        }

        // Detect anomalies
        self.anomaly_detector.detect_anomalies(&instances).await?;

        // Enforce limits
        if self.config.enforcement.enabled {
            self.enforcement_engine.enforce_limits(&instances, &self.config.enforcement).await?;
        }

        Ok(())
    }

    /// Check limits for a single instance
    async fn check_instance_limits_internal(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<bool> {
        let usage = &instance.current_usage;
        let limits = &instance.limits;

        let mut within_limits = true;

        // Check memory limit
        if let Some(max_memory) = limits.max_memory_bytes {
            if usage.memory_bytes > max_memory {
                within_limits = false;
                self.handle_limit_violation(instance_id, ResourceType::Memory, usage.memory_bytes as f64, max_memory as f64).await?;
            }
        }

        // Check CPU limit
        if let Some(max_cpu) = limits.max_cpu_percentage {
            if usage.cpu_percentage > max_cpu {
                within_limits = false;
                self.handle_limit_violation(instance_id, ResourceType::Cpu, usage.cpu_percentage, max_cpu).await?;
            }
        }

        // Check disk limit
        if let Some(max_disk) = limits.max_disk_bytes {
            if usage.disk_bytes > max_disk {
                within_limits = false;
                self.handle_limit_violation(instance_id, ResourceType::Disk, usage.disk_bytes as f64, max_disk as f64).await?;
            }
        }

        // Check open files limit
        if let Some(max_files) = limits.max_open_files {
            if usage.open_files > max_files {
                within_limits = false;
                self.handle_limit_violation(instance_id, ResourceType::FileDescriptors, usage.open_files as f64, max_files as f64).await?;
            }
        }

        // Check child processes limit
        if let Some(max_processes) = limits.max_child_processes {
            if usage.child_processes > max_processes {
                within_limits = false;
                self.handle_limit_violation(instance_id, ResourceType::Processes, usage.child_processes as f64, max_processes as f64).await?;
            }
        }

        Ok(within_limits)
    }

    /// Handle limit violation
    async fn handle_limit_violation(&self, instance_id: &str, resource_type: ResourceType, current_value: f64, limit: f64) -> PluginResult<()> {
        warn!("Resource limit exceeded for instance {}: {} = {:.2} > {:.2}",
              instance_id, format!("{:?}", resource_type), current_value, limit);

        // Take enforcement action
        let action = if self.config.enforcement.enabled {
            self.config.enforcement.limit_exceeded_action.clone()
        } else {
            LimitExceededAction::Warn
        };

        // Publish event
        self.publish_event(ResourceEvent::LimitExceeded {
            instance_id: instance_id.to_string(),
            resource_type: resource_type.clone(),
            current_value,
            limit,
            action_taken: action.clone(),
        }).await;

        // Execute enforcement action
        match action {
            LimitExceededAction::Terminate => {
                // Request instance termination (handled by PluginManager)
                info!("Requesting termination of instance {} due to resource limit violation", instance_id);
            }
            LimitExceededAction::Suspend => {
                info!("Requesting suspension of instance {} due to resource limit violation", instance_id);
            }
            LimitExceededAction::Throttle => {
                info!("Throttling instance {} due to resource limit violation", instance_id);
            }
            LimitExceededAction::Warn => {
                // Just log the warning
            }
            LimitExceededAction::Restart => {
                info!("Requesting restart of instance {} due to resource limit violation", instance_id);
            }
        }

        Ok(())
    }

    /// Update instance usage
    async fn update_instance_usage(&self, instance_id: &str, usage: ResourceUsage) -> PluginResult<()> {
        let mut instances = self.instances.write().await;

        if let Some(instance) = instances.get_mut(instance_id) {
            // Add to history (keep last 100 entries)
            instance.usage_history.push(usage.clone());
            if instance.usage_history.len() > 100 {
                instance.usage_history.remove(0);
            }

            // Update current usage
            instance.current_usage = usage.clone();

            // Store in historical data
            let mut historical = self.historical_data.write().await;
            let history = historical.entry(instance_id.to_string()).or_insert_with(Vec::new);
            history.push(usage.clone());

            // Keep history limited (last 1000 entries)
            if history.len() > 1000 {
                history.remove(0);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ResourceManager for DefaultResourceManager {
    async fn start(&mut self) -> PluginResult<()> {
        info!("Starting resource manager");

        {
            let mut running = self.running.write().await;
            if *running {
                return Err(PluginError::resource("Resource manager is already running".to_string()));
            }
            *running = true;
        }

        // Start metrics collection
        let config = self.config.clone();
        let instances = self.instances.clone();
        let metrics = self.metrics.clone();
        let anomaly_detector = self.anomaly_detector.clone();
        let enforcement_engine = self.enforcement_engine.clone();
        let running = self.running.clone();

        let handle = tokio::spawn(async move {
            let interval = config.monitoring.interval;

            loop {
                tokio::time::sleep(interval).await;

                // Check if still running
                if !*running.read().await {
                    break;
                }

                // Update metrics
                if let Err(e) = Self::update_metrics_internal(
                    &instances, &metrics, &anomaly_detector, &enforcement_engine, &config
                ).await {
                    error!("Failed to update resource metrics: {}", e);
                }
            }
        });

        {
            let mut metrics_handle = self.metrics_handle.write().await;
            *metrics_handle = Some(handle);
        }

        info!("Resource manager started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        info!("Stopping resource manager");

        {
            let mut running = self.running.write().await;
            *running = false;
        }

        // Stop metrics collection
        {
            let mut metrics_handle = self.metrics_handle.write().await;
            if let Some(handle) = metrics_handle.take() {
                handle.abort();
            }
        }

        info!("Resource manager stopped");
        Ok(())
    }

    async fn register_instance(&mut self, instance_id: String, limits: ResourceLimits) -> PluginResult<()> {
        debug!("Registering instance {} for resource monitoring", instance_id);

        let instance = MonitoredInstance {
            instance_id: instance_id.clone(),
            limits,
            current_usage: ResourceUsage::default(),
            usage_history: Vec::new(),
            violation_count: 0,
            last_violation: None,
            actions_taken: Vec::new(),
        };

        {
            let mut instances = self.instances.write().await;
            instances.insert(instance_id.clone(), instance);
        }

        self.publish_event(ResourceEvent::InstanceRegistered {
            instance_id,
            limits: instance.limits,
        }).await;

        Ok(())
    }

    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        debug!("Unregistering instance {} from resource monitoring", instance_id);

        {
            let mut instances = self.instances.write().await;
            instances.remove(instance_id);
        }

        {
            let mut historical = self.historical_data.write().await;
            historical.remove(instance_id);
        }

        self.publish_event(ResourceEvent::InstanceUnregistered {
            instance_id: instance_id.to_string(),
        }).await;

        Ok(())
    }

    async fn get_instance_usage(&self, instance_id: &str) -> PluginResult<ResourceUsage> {
        let instances = self.instances.read().await;
        instances.get(instance_id)
            .map(|instance| instance.current_usage.clone())
            .ok_or_else(|| PluginError::resource(format!("Instance {} not found", instance_id)))
    }

    async fn get_all_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>> {
        let instances = self.instances.read().await;
        let mut usage_map = HashMap::new();

        for (instance_id, instance) in instances.iter() {
            usage_map.insert(instance_id.clone(), instance.current_usage.clone());
        }

        Ok(usage_map)
    }

    async fn get_global_usage(&self) -> PluginResult<ResourceUsage> {
        let global_usage = self.global_usage.read().await;
        Ok(global_usage.clone())
    }

    async fn update_instance_limits(&mut self, instance_id: &str, limits: ResourceLimits) -> PluginResult<()> {
        debug!("Updating resource limits for instance {}", instance_id);

        let old_limits = {
            let mut instances = self.instances.write().await;
            let instance = instances.get_mut(instance_id)
                .ok_or_else(|| PluginError::resource(format!("Instance {} not found", instance_id)))?;

            let old_limits = instance.limits.clone();
            instance.limits = limits.clone();
            old_limits
        };

        self.publish_event(ResourceEvent::AllocationChanged {
            instance_id: instance_id.to_string(),
            old_limits,
            new_limits: limits,
        }).await;

        Ok(())
    }

    async fn check_instance_limits(&self, instance_id: &str) -> PluginResult<bool> {
        let instances = self.instances.read().await;
        let instance = instances.get(instance_id)
            .ok_or_else(|| PluginError::resource(format!("Instance {} not found", instance_id)))?;

        self.check_instance_limits_internal(instance_id, instance).await
    }

    async fn get_metrics(&self) -> PluginResult<ResourceMetrics> {
        let metrics = self.metrics.read().await;
        Ok(metrics.clone())
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<ResourceEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.push(tx);
        rx
    }

    async fn cleanup_instance_resources(&mut self, instance_id: &str) -> PluginResult<()> {
        info!("Cleaning up resources for instance {}", instance_id);

        let mut resources_freed = Vec::new();
        let mut bytes_freed = 0u64;

        // Remove from monitoring
        {
            let instances = self.instances.read().await;
            if let Some(instance) = instances.get(instance_id) {
                let usage = &instance.current_usage;

                if usage.memory_bytes > 0 {
                    resources_freed.push(ResourceType::Memory);
                    bytes_freed += usage.memory_bytes;
                }
                if usage.disk_bytes > 0 {
                    resources_freed.push(ResourceType::Disk);
                    bytes_freed += usage.disk_bytes;
                }
                if usage.open_files > 0 {
                    resources_freed.push(ResourceType::FileDescriptors);
                }
            }
        }

        // Remove from data structures
        self.unregister_instance(instance_id).await?;

        self.publish_event(ResourceEvent::CleanupCompleted {
            instance_id: instance_id.to_string(),
            resources_freed,
            bytes_freed,
        }).await;

        info!("Resource cleanup completed for instance {}", instance_id);
        Ok(())
    }
}

impl DefaultResourceManager {
    /// Internal metrics update method
    async fn update_metrics_internal(
        instances: &Arc<RwLock<HashMap<String, MonitoredInstance>>>,
        metrics: &Arc<RwLock<ResourceMetrics>>,
        anomaly_detector: &Arc<AnomalyDetector>,
        enforcement_engine: &Arc<EnforcementEngine>,
        config: &Arc<ResourceManagementConfig>,
    ) -> PluginResult<()> {
        let instances_guard = instances.read().await;
        let mut usage_by_instance = HashMap::new();
        let mut usage_by_type = HashMap::new();
        let mut total_usage = ResourceUsage::default();

        // Collect usage from all instances
        for (instance_id, instance) in instances_guard.iter() {
            let usage = &instance.current_usage;
            usage_by_instance.insert(instance_id.clone(), usage.clone());

            // Aggregate by type
            usage_by_type.insert(ResourceType::Cpu, usage.cpu_percentage);
            usage_by_type.insert(ResourceType::Memory, usage.memory_bytes as f64);
            usage_by_type.insert(ResourceType::Disk, usage.disk_bytes as f64);
            usage_by_type.insert(ResourceType::Network, usage.network_bytes as f64);
            usage_by_type.insert(ResourceType::FileDescriptors, usage.open_files as f64);
            usage_by_type.insert(ResourceType::Threads, usage.active_threads as f64);

            // Aggregate totals
            total_usage.memory_bytes += usage.memory_bytes;
            total_usage.cpu_percentage = total_usage.cpu_percentage.max(usage.cpu_percentage);
            total_usage.disk_bytes += usage.disk_bytes;
            total_usage.network_bytes += usage.network_bytes;
            total_usage.open_files += usage.open_files;
            total_usage.active_threads += usage.active_threads;
            total_usage.child_processes += usage.child_processes;
        }

        // Update metrics
        {
            let mut metrics_guard = metrics.write().await;
            metrics_guard.total_usage = total_usage;
            metrics_guard.usage_by_instance = usage_by_instance;
            metrics_guard.usage_by_type = usage_by_type;
            metrics_guard.timestamp = SystemTime::now();
        }

        // Detect anomalies
        if let Err(e) = anomaly_detector.detect_anomalies(&instances_guard).await {
            error!("Anomaly detection failed: {}", e);
        }

        // Enforce limits
        if config.enforcement.enabled {
            if let Err(e) = enforcement_engine.enforce_limits(&instances_guard, &config.enforcement).await {
                error!("Limit enforcement failed: {}", e);
            }
        }

        Ok(())
    }
}

/// ============================================================================
/// ANOMALY DETECTOR
/// ============================================================================

/// Anomaly detection system
#[derive(Debug)]
pub struct AnomalyDetector {
    /// Configuration
    config: AnomalyDetectorConfig,
}

/// Anomaly detector configuration
#[derive(Debug, Clone)]
struct AnomalyDetectorConfig {
    /// Enable anomaly detection
    enabled: bool,
    /// Threshold for spike detection (multiplier of baseline)
    spike_threshold: f64,
    /// Threshold for gradual increase detection
    gradual_threshold: f64,
    /// Window size for baseline calculation
    baseline_window: usize,
}

impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new() -> Self {
        Self {
            config: AnomalyDetectorConfig {
                enabled: true,
                spike_threshold: 3.0,
                gradual_threshold: 2.0,
                baseline_window: 10,
            },
        }
    }

    /// Detect anomalies in instance usage
    pub async fn detect_anomalies(&self, instances: &HashMap<String, MonitoredInstance>) -> PluginResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        for (instance_id, instance) in instances.iter() {
            if instance.usage_history.len() < self.config.baseline_window {
                continue; // Not enough data
            }

            self.detect_cpu_anomalies(instance_id, instance).await?;
            self.detect_memory_anomalies(instance_id, instance).await?;
            self.detect_resource_leaks(instance_id, instance).await?;
        }

        Ok(())
    }

    /// Detect CPU usage anomalies
    async fn detect_cpu_anomalies(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        let recent_usage = instance.usage_history.iter()
            .rev()
            .take(self.config.baseline_window)
            .map(|u| u.cpu_percentage)
            .collect::<Vec<_>>();

        if recent_usage.len() < self.config.baseline_window {
            return Ok(());
        }

        let baseline = recent_usage.iter().sum::<f64>() / recent_usage.len() as f64;
        let current = recent_usage[0];

        // Spike detection
        if current > baseline * self.config.spike_threshold {
            warn!("CPU spike detected for instance {}: current {:.1}% vs baseline {:.1}%",
                  instance_id, current, baseline);
        }

        Ok(())
    }

    /// Detect memory usage anomalies
    async fn detect_memory_anomalies(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        let recent_usage = instance.usage_history.iter()
            .rev()
            .take(self.config.baseline_window)
            .map(|u| u.memory_bytes)
            .collect::<Vec<_>>();

        if recent_usage.len() < self.config.baseline_window {
            return Ok(());
        }

        let baseline = recent_usage.iter().sum::<u64>() / recent_usage.len() as u64;
        let current = recent_usage[0];

        // Spike detection
        if current > baseline * self.config.spike_threshold as u64 {
            warn!("Memory spike detected for instance {}: current {} MB vs baseline {} MB",
                  instance_id, current / 1024 / 1024, baseline / 1024 / 1024);
        }

        // Gradual increase detection
        let is_increasing = recent_usage.windows(2).all(|w| w[0] <= w[1]);
        if is_increasing && current > baseline * self.config.gradual_threshold as u64 {
            warn!("Gradual memory increase detected for instance {}", instance_id);
        }

        Ok(())
    }

    /// Detect resource leaks
    async fn detect_resource_leaks(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        let recent_usage = instance.usage_history.iter()
            .rev()
            .take(self.config.baseline_window)
            .collect::<Vec<_>>();

        if recent_usage.len() < self.config.baseline_window {
            return Ok(());
        }

        // Check for consistently increasing open files
        let open_files_trend = recent_usage.iter().map(|u| u.open_files).collect::<Vec<_>>();
        let is_files_increasing = open_files_trend.windows(2).all(|w| w[0] <= w[1]);

        if is_files_increasing && open_files_trend.last() > Some(&100) {
            warn!("Potential file descriptor leak detected for instance {} ({} open files)",
                  instance_id, open_files_trend.last().unwrap_or(&0));
        }

        Ok(())
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// ENFORCEMENT ENGINE
/// ============================================================================

/// Enforcement engine for resource limits
#[derive(Debug)]
pub struct EnforcementEngine {
    /// Configuration
    config: EnforcementEngineConfig,
}

/// Enforcement engine configuration
#[derive(Debug, Clone)]
struct EnforcementEngineConfig {
    /// Enable enforcement
    enabled: bool,
    /// Grace period before first enforcement
    grace_period: Duration,
    /// Maximum actions per minute
    max_actions_per_minute: u32,
    /// Action history
    action_history: Arc<RwLock<Vec<EnforcementRecord>>>,
}

/// Enforcement action record
#[derive(Debug, Clone)]
struct EnforcementRecord {
    /// Instance ID
    instance_id: String,
    /// Action taken
    action: LimitExceededAction,
    /// Timestamp
    timestamp: SystemTime,
}

impl EnforcementEngine {
    /// Create a new enforcement engine
    pub fn new() -> Self {
        Self {
            config: EnforcementEngineConfig {
                enabled: true,
                grace_period: Duration::from_secs(30),
                max_actions_per_minute: 10,
                action_history: Arc::new(RwLock::new(Vec::new())),
            },
        }
    }

    /// Enforce resource limits
    pub async fn enforce_limits(
        &self,
        instances: &HashMap<String, MonitoredInstance>,
        config: &ResourceEnforcementConfig,
    ) -> PluginResult<()> {
        if !self.config.enabled || !config.enabled {
            return Ok(());
        }

        for (instance_id, instance) in instances.iter() {
            // Check if we're within rate limits
            if !self.check_rate_limits(instance_id).await? {
                continue;
            }

            // Check if grace period has passed
            if let Some(start_time) = instance.usage_history.first() {
                if SystemTime::now().duration_since(start_time.measured_at).unwrap_or(Duration::MAX) < self.config.grace_period {
                    continue;
                }
            }

            // Enforce limits based on strategy
            match config.strategy {
                EnforcementStrategy::Hard => {
                    self.enforce_hard_limits(instance_id, instance).await?;
                }
                EnforcementStrategy::Soft => {
                    self.enforce_soft_limits(instance_id, instance).await?;
                }
                EnforcementStrategy::Adaptive => {
                    self.enforce_adaptive_limits(instance_id, instance).await?;
                }
            }
        }

        Ok(())
    }

    /// Check rate limits for enforcement actions
    async fn check_rate_limits(&self, instance_id: &str) -> PluginResult<bool> {
        let mut history = self.config.action_history.write().await;

        // Remove old records (older than 1 minute)
        let one_minute_ago = SystemTime::now() - Duration::from_secs(60);
        history.retain(|record| record.timestamp > one_minute_ago);

        // Count actions for this instance in the last minute
        let recent_actions = history.iter()
            .filter(|record| record.instance_id == instance_id)
            .count();

        Ok(recent_actions < self.config.max_actions_per_minute as usize)
    }

    /// Enforce hard limits
    async fn enforce_hard_limits(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        let usage = &instance.current_usage;
        let limits = &instance.limits;

        // Hard enforcement - terminate immediately on any violation
        if let Some(max_memory) = limits.max_memory_bytes {
            if usage.memory_bytes > max_memory {
                self.record_action(instance_id, LimitExceededAction::Terminate).await;
                return self.request_termination(instance_id).await;
            }
        }

        if let Some(max_cpu) = limits.max_cpu_percentage {
            if usage.cpu_percentage > max_cpu {
                self.record_action(instance_id, LimitExceededAction::Terminate).await;
                return self.request_termination(instance_id).await;
            }
        }

        Ok(())
    }

    /// Enforce soft limits
    async fn enforce_soft_limits(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        let usage = &instance.current_usage;
        let limits = &instance.limits;

        // Soft enforcement - throttle or warn
        let mut action_needed = false;

        if let Some(max_memory) = limits.max_memory_bytes {
            if usage.memory_bytes > max_memory {
                action_needed = true;
            }
        }

        if let Some(max_cpu) = limits.max_cpu_percentage {
            if usage.cpu_percentage > max_cpu {
                action_needed = true;
            }
        }

        if action_needed {
            self.record_action(instance_id, LimitExceededAction::Throttle).await;
            return self.request_throttling(instance_id).await;
        }

        Ok(())
    }

    /// Enforce adaptive limits
    async fn enforce_adaptive_limits(&self, instance_id: &str, instance: &MonitoredInstance) -> PluginResult<()> {
        // Adaptive enforcement - adjust based on historical behavior
        let violation_rate = self.calculate_violation_rate(instance).await;

        if violation_rate > 0.8 {
            // High violation rate - enforce strictly
            self.enforce_hard_limits(instance_id, instance).await
        } else if violation_rate > 0.4 {
            // Medium violation rate - enforce softly
            self.enforce_soft_limits(instance_id, instance).await
        } else {
            // Low violation rate - just warn
            self.record_action(instance_id, LimitExceededAction::Warn).await;
            Ok(())
        }
    }

    /// Calculate violation rate for an instance
    async fn calculate_violation_rate(&self, instance: &MonitoredInstance) -> f64 {
        if instance.usage_history.is_empty() {
            return 0.0;
        }

        let violations = instance.usage_history.iter()
            .filter(|usage| self.is_violation(usage, &instance.limits))
            .count();

        violations as f64 / instance.usage_history.len() as f64
    }

    /// Check if usage violates limits
    fn is_violation(&self, usage: &ResourceUsage, limits: &ResourceLimits) -> bool {
        if let Some(max_memory) = limits.max_memory_bytes {
            if usage.memory_bytes > max_memory {
                return true;
            }
        }

        if let Some(max_cpu) = limits.max_cpu_percentage {
            if usage.cpu_percentage > max_cpu {
                return true;
            }
        }

        false
    }

    /// Record an enforcement action
    async fn record_action(&self, instance_id: &str, action: LimitExceededAction) {
        let record = EnforcementRecord {
            instance_id: instance_id.to_string(),
            action,
            timestamp: SystemTime::now(),
        };

        let mut history = self.config.action_history.write().await;
        history.push(record);
    }

    /// Request instance termination
    async fn request_termination(&self, instance_id: &str) -> PluginResult<()> {
        // This would be handled by the PluginManager
        info!("Requesting termination of instance {} due to resource limit violation", instance_id);
        Ok(())
    }

    /// Request instance throttling
    async fn request_throttling(&self, instance_id: &str) -> PluginResult<()> {
        // This would be handled by the PluginManager
        info!("Requesting throttling of instance {} due to resource limit violation", instance_id);
        Ok(())
    }
}

impl Default for EnforcementEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Create a default resource manager
pub fn create_resource_manager(config: ResourceManagementConfig) -> Box<dyn ResourceManager> {
    Box::new(DefaultResourceManager::new(config))
}