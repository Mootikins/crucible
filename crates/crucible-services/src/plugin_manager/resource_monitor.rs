//! # Plugin Resource Monitor
//!
//! This module provides comprehensive resource monitoring for plugin processes,
//! including CPU usage, memory consumption, I/O operations, and system handles.
//! The monitor is designed for low overhead operation and efficient data collection.

use super::error::{PluginError, PluginResult};
use super::types::{PluginInstance, ResourceUsage, ResourceLimits};
use crate::events::{EventEmitter, Event};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Child;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;
use uuid::Uuid;

/// ============================================================================
/// RESOURCE MONITORING CORE
/// ============================================================================

/// Resource monitoring event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceMonitoringEvent {
    /// Resource usage exceeded threshold
    ThresholdExceeded {
        instance_id: String,
        resource_type: ResourceType,
        current_value: f64,
        threshold: f64,
    },
    /// Resource usage returned to normal
    ThresholdNormalized {
        instance_id: String,
        resource_type: ResourceType,
        current_value: f64,
    },
    /// Resource quota exceeded
    QuotaExceeded {
        instance_id: String,
        resource_type: ResourceType,
        current_value: u64,
        quota: u64,
    },
    /// Monitoring data collected
    DataCollected {
        instance_id: String,
        usage: ResourceUsage,
    },
    /// Monitoring error occurred
    MonitoringError {
        instance_id: String,
        error: String,
    },
}

/// Resource types that can be monitored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// CPU usage percentage
    Cpu,
    /// Memory usage in bytes
    Memory,
    /// Disk I/O bytes
    DiskIo,
    /// Network I/O bytes
    NetworkIo,
    /// Number of file descriptors
    FileDescriptors,
    /// Number of threads
    Threads,
    /// Number of child processes
    ChildProcesses,
    /// Virtual memory in bytes
    VirtualMemory,
    /// Resident set size (RSS) in bytes
    ResidentSetSize,
    /// Peak memory usage
    PeakMemory,
}

/// Resource monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMonitoringConfig {
    /// Enable monitoring
    pub enabled: bool,
    /// Monitoring interval
    pub interval: Duration,
    /// History retention period
    pub history_retention: Duration,
    /// Maximum history entries per plugin
    pub max_history_entries: usize,
    /// Resource thresholds
    pub thresholds: HashMap<ResourceType, ResourceThreshold>,
    /// Enable adaptive monitoring
    pub adaptive_monitoring: bool,
    /// Batch collection size
    pub batch_size: usize,
    /// Collection timeout
    pub collection_timeout: Duration,
}

/// Resource threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceThreshold {
    /// Warning threshold (percentage)
    pub warning_threshold: f64,
    /// Critical threshold (percentage)
    pub critical_threshold: f64,
    /// Grace period before triggering alerts
    pub grace_period: Duration,
    /// Enable automatic throttling
    pub auto_throttle: bool,
}

/// Historical resource usage data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageHistory {
    /// Instance ID
    pub instance_id: String,
    /// Usage history (oldest first)
    pub history: VecDeque<ResourceUsageSnapshot>,
    /// Peak usage values
    pub peak_usage: HashMap<ResourceType, f64>,
    /// Average usage values
    pub average_usage: HashMap<ResourceType, f64>,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Resource usage snapshot at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageSnapshot {
    /// Timestamp of measurement
    pub timestamp: SystemTime,
    /// Resource usage values
    pub usage: HashMap<ResourceType, f64>,
    /// Process metadata
    pub metadata: ProcessMetadata,
}

/// Process metadata collected during monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetadata {
    /// Process ID
    pub pid: u32,
    /// Parent process ID
    pub ppid: u32,
    /// Process start time
    pub start_time: SystemTime,
    /// Command line
    pub command_line: Option<String>,
    /// Working directory
    pub working_directory: Option<String>,
    /// Number of threads
    pub thread_count: u32,
    /// Process status
    pub status: ProcessStatus,
}

/// Process status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Running
    Running,
    /// Sleeping
    Sleeping,
    /// Waiting
    Waiting,
    /// Zombie
    Zombie,
    /// Stopped
    Stopped,
    /// Tracing stop
    TracingStop,
    /// Dead
    Dead,
    /// Unknown
    Unknown,
}

/// Resource quota enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuotaConfig {
    /// Enable quota enforcement
    pub enabled: bool,
    /// Quota limits per resource type
    pub quotas: HashMap<ResourceType, u64>,
    /// Enforcement strategy
    pub strategy: QuotaEnforcementStrategy,
    /// Grace period before enforcement
    pub grace_period: Duration,
    /// Action when quota exceeded
    pub exceeded_action: QuotaExceededAction,
}

/// Quota enforcement strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuotaEnforcementStrategy {
    /// Hard limit (terminate when exceeded)
    Hard,
    /// Soft limit (throttle when exceeded)
    Soft,
    /// Adaptive limit (adjust based on usage patterns)
    Adaptive,
}

/// Action when resource quota is exceeded
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuotaExceededAction {
    /// Terminate the plugin
    Terminate,
    /// Suspend the plugin
    Suspend,
    /// Throttle the plugin
    Throttle,
    /// Send warning
    Warn,
    /// Reduce quota temporarily
    ReduceQuota,
}

/// ============================================================================
/// RESOURCE MONITOR IMPLEMENTATION
/// ============================================================================

/// Resource monitor for plugin processes
pub struct ResourceMonitor {
    /// Monitoring configuration
    config: ResourceMonitoringConfig,
    /// Active plugin monitoring data
    monitored_plugins: RwLock<HashMap<String, PluginMonitoringData>>,
    /// Historical data storage
    history_storage: RwLock<HashMap<String, ResourceUsageHistory>>,
    /// Event emitter for monitoring events
    event_emitter: EventEmitter,
    /// Shutdown channel receiver
    shutdown_rx: Option<mpsc::Receiver<()>>,
    /// Shutdown channel sender
    shutdown_tx: mpsc::Sender<()>,
    /// System resource collector
    system_collector: SystemResourceCollector,
}

/// Monitoring data for a specific plugin
#[derive(Debug)]
struct PluginMonitoringData {
    /// Plugin instance reference
    instance: PluginInstance,
    /// Process handle (if available)
    process: Option<Child>,
    /// Last monitoring timestamp
    last_monitored: Option<Instant>,
    /// Current thresholds status
    threshold_status: HashMap<ResourceType, ThresholdStatus>,
    /// Monitoring statistics
    stats: MonitoringStatistics,
}

/// Threshold status for a resource type
#[derive(Debug, Clone)]
struct ThresholdStatus {
    /// Current status
    status: ThresholdState,
    /// Status changed timestamp
    status_changed_at: Instant,
    /// Consecutive violations
    consecutive_violations: u32,
    /// Total violations
    total_violations: u32,
}

/// Threshold state
#[derive(Debug, Clone, PartialEq, Eq)]
enum ThresholdState {
    /// Within normal limits
    Normal,
    /// Above warning threshold
    Warning,
    /// Above critical threshold
    Critical,
}

/// Monitoring statistics
#[derive(Debug, Clone, Default)]
struct MonitoringStatistics {
    /// Total monitoring cycles
    total_cycles: u64,
    /// Successful cycles
    successful_cycles: u64,
    /// Failed cycles
    failed_cycles: u64,
    /// Average cycle duration
    average_cycle_duration: Duration,
    /// Last successful collection
    last_successful_collection: Option<Instant>,
}

/// System resource collector interface
trait SystemResourceCollector: Send + Sync {
    /// Collect current resource usage for a process
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage>;

    /// Collect process metadata
    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata>;

    /// Check if process is still running
    fn is_process_running(&self, pid: u32) -> bool;

    /// Get system-wide resource information
    fn get_system_info(&self) -> PluginResult<SystemResourceInfo>;
}

/// System resource information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourceInfo {
    /// Total system memory in bytes
    pub total_memory: u64,
    /// Available memory in bytes
    pub available_memory: u64,
    /// Total CPU cores
    pub total_cpu_cores: u32,
    /// CPU usage percentage
    pub cpu_usage_percentage: f64,
    /// System load averages
    pub load_averages: (f64, f64, f64), // 1min, 5min, 15min
}

/// Default system resource collector implementation
struct DefaultSystemResourceCollector {
    /// Platform-specific collector
    platform_collector: Box<dyn PlatformResourceCollector>,
}

/// Platform-specific resource collector interface
trait PlatformResourceCollector: Send + Sync {
    /// Collect resource usage on this platform
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage>;

    /// Collect process metadata on this platform
    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata>;

    /// Check if process is running on this platform
    fn is_process_running(&self, pid: u32) -> bool;

    /// Get system information on this platform
    fn get_system_info(&self) -> PluginResult<SystemResourceInfo>;
}

/// Linux-specific resource collector
#[cfg(target_os = "linux")]
struct LinuxResourceCollector;

/// macOS-specific resource collector
#[cfg(target_os = "macos")]
struct MacOsResourceCollector;

/// Windows-specific resource collector
#[cfg(target_os = "windows")]
struct WindowsResourceCollector;

/// Cross-platform fallback collector
struct CrossPlatformCollector;

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(config: ResourceMonitoringConfig, event_emitter: EventEmitter) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Self {
            config,
            monitored_plugins: RwLock::new(HashMap::new()),
            history_storage: RwLock::new(HashMap::new()),
            event_emitter,
            shutdown_rx: Some(shutdown_rx),
            shutdown_tx,
            system_collector: DefaultSystemResourceCollector::new(),
        }
    }

    /// Start monitoring a plugin instance
    pub async fn start_monitoring(&self, instance: PluginInstance, process: Option<Child>) -> PluginResult<()> {
        let instance_id = instance.instance_id.clone();

        // Initialize monitoring data
        let monitoring_data = PluginMonitoringData {
            instance,
            process,
            last_monitored: None,
            threshold_status: HashMap::new(),
            stats: MonitoringStatistics::default(),
        };

        // Add to monitored plugins
        {
            let mut monitored = self.monitored_plugins.write().await;
            monitored.insert(instance_id.clone(), monitoring_data);
        }

        // Initialize history storage
        {
            let mut history = self.history_storage.write().await;
            history.insert(instance_id.clone(), ResourceUsageHistory {
                instance_id: instance_id.clone(),
                history: VecDeque::new(),
                peak_usage: HashMap::new(),
                average_usage: HashMap::new(),
                last_updated: SystemTime::now(),
            });
        }

        // Emit monitoring started event
        self.event_emitter.emit(Event::ResourceMonitoring(
            ResourceMonitoringEvent::DataCollected {
                instance_id: instance_id.clone(),
                usage: ResourceUsage::default(),
            }
        )).await?;

        tracing::info!("Started resource monitoring for plugin instance: {}", instance_id);
        Ok(())
    }

    /// Stop monitoring a plugin instance
    pub async fn stop_monitoring(&self, instance_id: &str) -> PluginResult<()> {
        // Remove from active monitoring
        let monitoring_data = {
            let mut monitored = self.monitored_plugins.write().await;
            monitored.remove(instance_id)
        };

        // Clean up process if still running
        if let Some(mut data) = monitoring_data {
            if let Some(mut process) = data.process.take() {
                let _ = process.kill();
                let _ = process.wait();
            }
        }

        tracing::info!("Stopped resource monitoring for plugin instance: {}", instance_id);
        Ok(())
    }

    /// Get current resource usage for a plugin
    pub async fn get_current_usage(&self, instance_id: &str) -> PluginResult<Option<ResourceUsage>> {
        let monitored = self.monitored_plugins.read().await;
        if let Some(data) = monitored.get(instance_id) {
            if let Some(pid) = data.instance.pid {
                Ok(Some(self.system_collector.collect_resource_usage(pid)?))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get historical resource usage for a plugin
    pub async fn get_usage_history(&self, instance_id: &str) -> PluginResult<Option<ResourceUsageHistory>> {
        let history = self.history_storage.read().await;
        Ok(history.get(instance_id).cloned())
    }

    /// Get monitoring statistics for a plugin
    pub async fn get_monitoring_stats(&self, instance_id: &str) -> PluginResult<Option<MonitoringStatistics>> {
        let monitored = self.monitored_plugins.read().await;
        Ok(monitored.get(instance_id).map(|data| data.stats.clone()))
    }

    /// Update resource thresholds
    pub async fn update_thresholds(&self, instance_id: &str, thresholds: HashMap<ResourceType, ResourceThreshold>) -> PluginResult<()> {
        let mut monitored = self.monitored_plugins.write().await;
        if let Some(data) = monitored.get_mut(instance_id) {
            // Update threshold status for new thresholds
            for resource_type in thresholds.keys() {
                data.threshold_status.insert(resource_type.clone(), ThresholdStatus {
                    status: ThresholdState::Normal,
                    status_changed_at: Instant::now(),
                    consecutive_violations: 0,
                    total_violations: 0,
                });
            }
        }
        Ok(())
    }

    /// Start the monitoring loop
    pub async fn start_monitoring_loop(&mut self) -> PluginResult<()> {
        if !self.config.enabled {
            tracing::info!("Resource monitoring is disabled");
            return Ok(());
        }

        let mut shutdown_rx = self.shutdown_rx.take()
            .ok_or_else(|| PluginError::internal("Monitoring loop already started"))?;

        let interval = self.config.interval;
        let event_emitter = self.event_emitter.clone();

        tracing::info!("Starting resource monitoring loop with interval: {:?}", interval);

        let monitor_task = async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        // Perform monitoring cycle
                        if let Err(e) = Self::perform_monitoring_cycle().await {
                            tracing::error!("Error in monitoring cycle: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Resource monitoring loop shutting down");
                        break;
                    }
                }
            }
        };

        tokio::spawn(monitor_task);
        Ok(())
    }

    /// Stop the monitoring loop
    pub async fn stop_monitoring_loop(&self) -> PluginResult<()> {
        let _ = self.shutdown_tx.send(()).await;
        Ok(())
    }

    /// Perform a single monitoring cycle for all plugins
    async fn perform_monitoring_cycle() -> PluginResult<()> {
        // This would be implemented with the actual monitoring logic
        // For now, just sleep to simulate work
        sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    /// Collect resource usage for a specific plugin
    async fn collect_plugin_usage(&self, instance_id: &str) -> PluginResult<Option<ResourceUsage>> {
        let monitored = self.monitored_plugins.read().await;
        if let Some(data) = monitored.get(instance_id) {
            if let Some(pid) = data.instance.pid {
                let usage = self.system_collector.collect_resource_usage(pid)?;
                Ok(Some(usage))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check resource thresholds and emit alerts if necessary
    async fn check_thresholds(&self, instance_id: &str, usage: &ResourceUsage) -> PluginResult<()> {
        // Implementation for threshold checking
        // This would compare current usage against configured thresholds
        // and emit appropriate events
        Ok(())
    }

    /// Update historical data with new usage information
    async fn update_history(&self, instance_id: &str, usage: &ResourceUsage) -> PluginResult<()> {
        let mut history_storage = self.history_storage.write().await;
        if let Some(history) = history_storage.get_mut(instance_id) {
            // Create snapshot
            let snapshot = ResourceUsageSnapshot {
                timestamp: SystemTime::now(),
                usage: HashMap::new(),
                metadata: ProcessMetadata {
                    pid: 0,
                    ppid: 0,
                    start_time: SystemTime::now(),
                    command_line: None,
                    working_directory: None,
                    thread_count: 0,
                    status: ProcessStatus::Unknown,
                },
            };

            // Add to history
            history.history.push_back(snapshot);

            // Trim history if needed
            if history.history.len() > self.config.max_history_entries {
                history.history.pop_front();
            }

            // Update statistics
            self.update_usage_statistics(&mut history);

            history.last_updated = SystemTime::now();
        }
        Ok(())
    }

    /// Update usage statistics (peak and average values)
    fn update_usage_statistics(&self, history: &mut ResourceUsageHistory) {
        // Reset averages
        history.average_usage.clear();

        // Calculate averages from history
        for snapshot in &history.history {
            for (resource_type, value) in &snapshot.usage {
                let average = history.average_usage.entry(resource_type.clone()).or_insert(0.0);
                *average += value;
            }
        }

        // Finalize averages
        let history_count = history.history.len() as f64;
        if history_count > 0.0 {
            for average in history.average_usage.values_mut() {
                *average /= history_count;
            }
        }

        // Update peak values
        for snapshot in &history.history {
            for (resource_type, value) in &snapshot.usage {
                let peak = history.peak_usage.entry(resource_type.clone()).or_insert(0.0);
                if *value > *peak {
                    *peak = *value;
                }
            }
        }
    }

    /// Get aggregated resource usage across all plugins
    pub async fn get_aggregated_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>> {
        let monitored = self.monitored_plugins.read().await;
        let mut aggregated = HashMap::new();

        for (instance_id, data) in monitored.iter() {
            if let Some(pid) = data.instance.pid {
                if let Ok(usage) = self.system_collector.collect_resource_usage(pid) {
                    aggregated.insert(instance_id.clone(), usage);
                }
            }
        }

        Ok(aggregated)
    }

    /// Get system-wide resource information
    pub async fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        self.system_collector.get_system_info()
    }

    /// Check if resource quotas are being exceeded
    pub async fn check_quota_violations(&self, instance_id: &str) -> PluginResult<Vec<ResourceType>> {
        let monitored = self.monitored_plugins.read().await;
        let mut violations = Vec::new();

        if let Some(data) = monitored.get(instance_id) {
            if let Some(pid) = data.instance.pid {
                if let Ok(usage) = self.system_collector.collect_resource_usage(pid) {
                    // Check against configured quotas
                    // Implementation would compare usage against quotas
                    // and return list of violated resource types
                }
            }
        }

        Ok(violations)
    }
}

impl DefaultSystemResourceCollector {
    fn new() -> Self {
        #[cfg(target_os = "linux")]
        let platform_collector: Box<dyn PlatformResourceCollector> = Box::new(LinuxResourceCollector);

        #[cfg(target_os = "macos")]
        let platform_collector: Box<dyn PlatformResourceCollector> = Box::new(MacOsResourceCollector);

        #[cfg(target_os = "windows")]
        let platform_collector: Box<dyn PlatformResourceCollector> = Box::new(WindowsResourceCollector);

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let platform_collector: Box<dyn PlatformResourceCollector> = Box::new(CrossPlatformCollector);

        Self {
            platform_collector,
        }
    }
}

impl SystemResourceCollector for DefaultSystemResourceCollector {
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage> {
        self.platform_collector.collect_resource_usage(pid)
    }

    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata> {
        self.platform_collector.collect_process_metadata(pid)
    }

    fn is_process_running(&self, pid: u32) -> bool {
        self.platform_collector.is_process_running(pid)
    }

    fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        self.platform_collector.get_system_info()
    }
}

// Platform-specific implementations would go here
// For now, we'll provide a basic cross-platform fallback

impl PlatformResourceCollector for CrossPlatformCollector {
    fn collect_resource_usage(&self, _pid: u32) -> PluginResult<ResourceUsage> {
        // Basic cross-platform implementation
        // In a real implementation, this would use platform-specific APIs
        Ok(ResourceUsage {
            memory_bytes: 0,
            cpu_percentage: 0.0,
            disk_bytes: 0,
            network_bytes: 0,
            open_files: 0,
            active_threads: 0,
            child_processes: 0,
            measured_at: SystemTime::now(),
        })
    }

    fn collect_process_metadata(&self, _pid: u32) -> PluginResult<ProcessMetadata> {
        // Basic cross-platform implementation
        Ok(ProcessMetadata {
            pid: 0,
            ppid: 0,
            start_time: SystemTime::now(),
            command_line: None,
            working_directory: None,
            thread_count: 0,
            status: ProcessStatus::Unknown,
        })
    }

    fn is_process_running(&self, _pid: u32) -> bool {
        // Basic implementation - would use platform-specific checks
        false
    }

    fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        // Basic cross-platform implementation
        Ok(SystemResourceInfo {
            total_memory: 0,
            available_memory: 0,
            total_cpu_cores: 1,
            cpu_usage_percentage: 0.0,
            load_averages: (0.0, 0.0, 0.0),
        })
    }
}

// Placeholder implementations for platform-specific collectors
// These would be implemented with actual system calls and APIs

#[cfg(target_os = "linux")]
impl PlatformResourceCollector for LinuxResourceCollector {
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage> {
        // Linux-specific implementation using /proc filesystem
        CrossPlatformCollector.collect_resource_usage(pid)
    }

    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata> {
        // Linux-specific implementation
        CrossPlatformCollector.collect_process_metadata(pid)
    }

    fn is_process_running(&self, pid: u32) -> bool {
        // Linux-specific implementation using kill(pid, 0)
        CrossPlatformCollector.is_process_running(pid)
    }

    fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        // Linux-specific implementation using /proc/meminfo, /proc/loadavg
        CrossPlatformCollector.get_system_info()
    }
}

#[cfg(target_os = "macos")]
impl PlatformResourceCollector for MacOsResourceCollector {
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage> {
        // macOS-specific implementation using libproc or sysctl
        CrossPlatformCollector.collect_resource_usage(pid)
    }

    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata> {
        // macOS-specific implementation
        CrossPlatformCollector.collect_process_metadata(pid)
    }

    fn is_process_running(&self, pid: u32) -> bool {
        // macOS-specific implementation
        CrossPlatformCollector.is_process_running(pid)
    }

    fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        // macOS-specific implementation using sysctl
        CrossPlatformCollector.get_system_info()
    }
}

#[cfg(target_os = "windows")]
impl PlatformResourceCollector for WindowsResourceCollector {
    fn collect_resource_usage(&self, pid: u32) -> PluginResult<ResourceUsage> {
        // Windows-specific implementation using Windows API
        CrossPlatformCollector.collect_resource_usage(pid)
    }

    fn collect_process_metadata(&self, pid: u32) -> PluginResult<ProcessMetadata> {
        // Windows-specific implementation
        CrossPlatformCollector.collect_process_metadata(pid)
    }

    fn is_process_running(&self, pid: u32) -> bool {
        // Windows-specific implementation
        CrossPlatformCollector.is_process_running(pid)
    }

    fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        // Windows-specific implementation
        CrossPlatformCollector.get_system_info()
    }
}

/// ============================================================================
/// DEFAULT CONFIGURATIONS AND FACTORIES
/// ============================================================================

impl Default for ResourceMonitoringConfig {
    fn default() -> Self {
        let mut thresholds = HashMap::new();

        // CPU thresholds
        thresholds.insert(ResourceType::Cpu, ResourceThreshold {
            warning_threshold: 70.0,
            critical_threshold: 90.0,
            grace_period: Duration::from_secs(30),
            auto_throttle: true,
        });

        // Memory thresholds
        thresholds.insert(ResourceType::Memory, ResourceThreshold {
            warning_threshold: 80.0,
            critical_threshold: 95.0,
            grace_period: Duration::from_secs(60),
            auto_throttle: true,
        });

        // File descriptor thresholds
        thresholds.insert(ResourceType::FileDescriptors, ResourceThreshold {
            warning_threshold: 80.0,
            critical_threshold: 90.0,
            grace_period: Duration::from_secs(30),
            auto_throttle: false,
        });

        Self {
            enabled: true,
            interval: Duration::from_secs(5),
            history_retention: Duration::from_secs(60 * 60 * 24), // 24 hours
            max_history_entries: 1000,
            thresholds,
            adaptive_monitoring: true,
            batch_size: 10,
            collection_timeout: Duration::from_secs(10),
        }
    }
}

impl Default for ResourceQuotaConfig {
    fn default() -> Self {
        let mut quotas = HashMap::new();

        // Default quotas
        quotas.insert(ResourceType::Memory, 512 * 1024 * 1024); // 512MB
        quotas.insert(ResourceType::Cpu, 80.0 as u64); // 80%
        quotas.insert(ResourceType::FileDescriptors, 1000);
        quotas.insert(ResourceType::Threads, 50);

        Self {
            enabled: true,
            quotas,
            strategy: QuotaEnforcementStrategy::Soft,
            grace_period: Duration::from_secs(60),
            exceeded_action: QuotaExceededAction::Throttle,
        }
    }
}

/// ============================================================================
/// MONITORING SERVICE TRAIT
/// ============================================================================

/// Resource monitoring service trait
#[async_trait::async_trait]
pub trait ResourceMonitoringService: Send + Sync {
    /// Start monitoring a plugin
    async fn start_monitoring(&self, instance: PluginInstance, process: Option<Child>) -> PluginResult<()>;

    /// Stop monitoring a plugin
    async fn stop_monitoring(&self, instance_id: &str) -> PluginResult<()>;

    /// Get current resource usage
    async fn get_current_usage(&self, instance_id: &str) -> PluginResult<Option<ResourceUsage>>;

    /// Get usage history
    async fn get_usage_history(&self, instance_id: &str) -> PluginResult<Option<ResourceUsageHistory>>;

    /// Update thresholds
    async fn update_thresholds(&self, instance_id: &str, thresholds: HashMap<ResourceType, ResourceThreshold>) -> PluginResult<()>;

    /// Get aggregated usage
    async fn get_aggregated_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>>;

    /// Get system information
    async fn get_system_info(&self) -> PluginResult<SystemResourceInfo>;
}

#[async_trait::async_trait]
impl ResourceMonitoringService for ResourceMonitor {
    async fn start_monitoring(&self, instance: PluginInstance, process: Option<Child>) -> PluginResult<()> {
        self.start_monitoring(instance, process).await
    }

    async fn stop_monitoring(&self, instance_id: &str) -> PluginResult<()> {
        self.stop_monitoring(instance_id).await
    }

    async fn get_current_usage(&self, instance_id: &str) -> PluginResult<Option<ResourceUsage>> {
        self.get_current_usage(instance_id).await
    }

    async fn get_usage_history(&self, instance_id: &str) -> PluginResult<Option<ResourceUsageHistory>> {
        self.get_usage_history(instance_id).await
    }

    async fn update_thresholds(&self, instance_id: &str, thresholds: HashMap<ResourceType, ResourceThreshold>) -> PluginResult<()> {
        self.update_thresholds(instance_id, thresholds).await
    }

    async fn get_aggregated_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>> {
        self.get_aggregated_usage().await
    }

    async fn get_system_info(&self) -> PluginResult<SystemResourceInfo> {
        self.get_system_info().await
    }
}