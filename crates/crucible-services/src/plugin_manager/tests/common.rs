//! # Common Test Utilities
//!
//! Shared utilities and helpers for plugin system integration tests.

use super::super::config::*;
use super::super::error::PluginResult;
use super::super::types::*;
use super::super::manager::{PluginManagerService, PluginManagerEvent};
use super::super::instance::PluginInstance;
use super::super::registry::PluginRegistryEntry;
use super::super::resource_monitor::ResourceMonitor;
use super::super::health_checker::HealthChecker;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tempfile::{TempDir, NamedTempFile};
use tokio::sync::{mpsc, RwLock, Mutex};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// ============================================================================
/// TEST CONFIGURATION BUILDERS
/// ============================================================================

/// Builder for creating test configurations
#[derive(Debug, Clone)]
pub struct TestConfigBuilder {
    config: PluginManagerConfig,
}

impl TestConfigBuilder {
    /// Create a new test configuration builder
    pub fn new() -> Self {
        Self {
            config: PluginManagerConfig::default(),
        }
    }

    /// Set plugin directories for testing
    pub fn with_plugin_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.config.plugin_directories = dirs;
        self
    }

    /// Enable auto-discovery with custom settings
    pub fn with_auto_discovery(mut self, enabled: bool, scan_interval: Duration) -> Self {
        self.config.auto_discovery.enabled = enabled;
        self.config.auto_discovery.scan_interval = scan_interval;
        self
    }

    /// Configure security settings for testing
    pub fn with_security(mut self, security: SecurityConfig) -> Self {
        self.config.security = security;
        self
    }

    /// Configure resource management for testing
    pub fn with_resource_management(mut self, resource_mgmt: ResourceManagementConfig) -> Self {
        self.config.resource_management = resource_mgmt;
        self
    }

    /// Configure health monitoring for testing
    pub fn with_health_monitoring(mut self, health: HealthMonitoringConfig) -> Self {
        self.config.health_monitoring = health;
        self
    }

    /// Enable performance monitoring
    pub fn with_performance_monitoring(mut self) -> Self {
        self.config.resource_management.monitoring.enabled = true;
        self.config.health_monitoring.enabled = true;
        self
    }

    /// Set thread pool size for testing
    pub fn with_thread_pool_size(mut self, size: u32) -> Self {
        self.config.performance.thread_pool_size = size;
        self
    }

    /// Build the configuration
    pub fn build(self) -> PluginManagerConfig {
        self.config
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// MOCK PLUGIN MANIFESTS
/// ============================================================================

/// Create a mock plugin manifest for testing
pub fn create_mock_plugin_manifest(plugin_id: &str, name: &str) -> PluginManifest {
    PluginManifest {
        id: plugin_id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Mock plugin {} for testing", name),
        author: "Test Suite".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        repository: None,
        keywords: vec!["test".to_string(), "mock".to_string()],
        category: PluginCategory::Utility,
        entry_point: PathBuf::from(format!("{}.js", plugin_id)),
        dependencies: vec![],
        optional_dependencies: vec![],
        resource_limits: ResourceLimits::default(),
        capabilities: vec![
            PluginCapability::FileSystem {
                read_paths: vec!["/tmp".to_string()],
                write_paths: vec!["/tmp".to_string()],
            },
            PluginCapability::IpcCommunication,
        ],
        security_level: SecurityLevel::Basic,
        sandbox_config: SandboxConfig::default(),
        metadata: HashMap::from([
            ("test".to_string(), serde_json::Value::Bool(true)),
            ("created_at".to_string(), serde_json::Value::String(
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_string()
            )),
        ]),
    }
}

/// Create a mock plugin manifest with dependencies
pub fn create_mock_plugin_with_dependencies(
    plugin_id: &str,
    name: &str,
    dependencies: Vec<String>,
) -> PluginManifest {
    let mut manifest = create_mock_plugin_manifest(plugin_id, name);
    manifest.dependencies = dependencies;
    manifest
}

/// Create a collection of mock plugins with dependency chains
pub fn create_mock_plugin_suite() -> Vec<PluginManifest> {
    vec![
        // Core plugin (no dependencies)
        create_mock_plugin_manifest("core-plugin", "Core Plugin"),

        // Database plugin (depends on core)
        create_mock_plugin_with_dependencies(
            "database-plugin",
            "Database Plugin",
            vec!["core-plugin".to_string()]
        ),

        // API plugin (depends on database)
        create_mock_plugin_with_dependencies(
            "api-plugin",
            "API Plugin",
            vec!["database-plugin".to_string()]
        ),

        // Web plugin (depends on API)
        create_mock_plugin_with_dependencies(
            "web-plugin",
            "Web Plugin",
            vec!["api-plugin".to_string()]
        ),

        // Logger plugin (depends on core)
        create_mock_plugin_with_dependencies(
            "logger-plugin",
            "Logger Plugin",
            vec!["core-plugin".to_string()]
        ),

        // Monitoring plugin (depends on logger and core)
        create_mock_plugin_with_dependencies(
            "monitoring-plugin",
            "Monitoring Plugin",
            vec!["logger-plugin".to_string(), "core-plugin".to_string()]
        ),
    ]
}

/// ============================================================================
/// MOCK PLUGIN INSTANCES
/// ============================================================================

/// Mock plugin instance for testing
#[derive(Debug, Clone)]
pub struct MockPluginInstance {
    pub instance_id: String,
    pub plugin_id: String,
    pub state: PluginInstanceState,
    pub health_status: PluginHealthStatus,
    pub resource_usage: ResourceUsage,
    pub start_time: Option<SystemTime>,
    pub crash_count: u32,
    pub should_crash: bool,
    pub should_hang: bool,
    pub event_log: Arc<Mutex<Vec<String>>>,
}

impl MockPluginInstance {
    /// Create a new mock plugin instance
    pub fn new(plugin_id: &str, instance_id: Option<String>) -> Self {
        Self {
            instance_id: instance_id.unwrap_or_else(|| format!("{}-{}", plugin_id, Uuid::new_v4())),
            plugin_id: plugin_id.to_string(),
            state: PluginInstanceState::Created,
            health_status: PluginHealthStatus::Unknown,
            resource_usage: ResourceUsage::default(),
            start_time: None,
            crash_count: 0,
            should_crash: false,
            should_hang: false,
            event_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configure the instance to crash on start
    pub fn with_crash(mut self) -> Self {
        self.should_crash = true;
        self
    }

    /// Configure the instance to hang on operations
    pub fn with_hang(mut self) -> Self {
        self.should_hang = true;
        self
    }

    /// Log an event
    pub async fn log_event(&self, event: &str) {
        let mut log = self.event_log.lock().await;
        log.push(format!("{}: {}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(), event));
    }

    /// Get the event log
    pub async fn get_event_log(&self) -> Vec<String> {
        let log = self.event_log.lock().await;
        log.clone()
    }

    /// Simulate plugin crash
    pub async fn crash(&mut self) {
        self.crash_count += 1;
        self.state = PluginInstanceState::Error("Simulated crash".to_string());
        self.health_status = PluginHealthStatus::Unhealthy;
        self.log_event("CRASHED").await;
    }

    /// Simulate resource usage changes
    pub async fn simulate_resource_usage(&mut self, memory_mb: u64, cpu_percent: f64) {
        self.resource_usage.memory_bytes = memory_mb * 1024 * 1024;
        self.resource_usage.cpu_percentage = cpu_percent;
        self.log_event(&format!("Resource usage updated: {}MB, {}% CPU", memory_mb, cpu_percent)).await;
    }
}

/// ============================================================================
/// TEST ENVIRONMENT SETUP
/// ============================================================================

/// Test environment for plugin integration tests
#[derive(Debug)]
pub struct TestEnvironment {
    /// Temporary directory for test files
    pub temp_dir: TempDir,
    /// Plugin manager service
    pub plugin_manager: Arc<RwLock<PluginManagerService>>,
    /// Event receiver for plugin manager events
    pub event_receiver: Arc<Mutex<mpsc::UnboundedReceiver<PluginManagerEvent>>>,
    /// Mock plugin instances
    pub mock_instances: Arc<RwLock<HashMap<String, MockPluginInstance>>>,
    /// Test configuration
    pub config: PluginManagerConfig,
}

impl TestEnvironment {
    /// Create a new test environment
    pub async fn new() -> PluginResult<Self> {
        Self::with_config(TestConfigBuilder::new().build()).await
    }

    /// Create a test environment with custom configuration
    pub async fn with_config(config: PluginManagerConfig) -> PluginResult<Self> {
        let temp_dir = TempDir::new().map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to create temp dir: {}", e))
        })?;

        // Create plugin directory in temp directory
        let plugin_dir = temp_dir.path().join("plugins");
        std::fs::create_dir_all(&plugin_dir).map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to create plugin dir: {}", e))
        })?;

        // Update config with test plugin directory
        let mut test_config = config;
        test_config.plugin_directories = vec![plugin_dir];

        // Create plugin manager
        let mut plugin_manager = PluginManagerService::new(test_config.clone());

        // Subscribe to events before starting
        let event_receiver = plugin_manager.subscribe_events().await;

        // Start the plugin manager
        plugin_manager.start().await.map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to start plugin manager: {}", e))
        })?;

        Ok(Self {
            temp_dir,
            plugin_manager: Arc::new(RwLock::new(plugin_manager)),
            event_receiver: Arc::new(Mutex::new(event_receiver)),
            mock_instances: Arc::new(RwLock::new(HashMap::new())),
            config: test_config,
        })
    }

    /// Create a test environment with performance monitoring enabled
    pub async fn with_monitoring() -> PluginResult<Self> {
        let config = TestConfigBuilder::new()
            .with_performance_monitoring()
            .with_auto_discovery(false, Duration::from_secs(60))
            .build();

        Self::with_config(config).await
    }

    /// Register a mock plugin
    pub async fn register_mock_plugin(&self, manifest: PluginManifest) -> PluginResult<String> {
        let plugin_id = manifest.id.clone();

        // Write manifest file
        let manifest_path = self.temp_dir.path().join("plugins").join(format!("{}.json", plugin_id));
        let manifest_content = serde_json::to_string_pretty(&manifest).map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to serialize manifest: {}", e))
        })?;

        std::fs::write(&manifest_path, manifest_content).map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to write manifest: {}", e))
        })?;

        // Create mock entry point file
        let entry_point_path = self.temp_dir.path().join("plugins").join(&manifest.entry_point);
        std::fs::write(&entry_point_path, "// Mock plugin entry point").map_err(|e| {
            super::super::error::PluginError::generic(format!("Failed to write entry point: {}", e))
        })?;

        info!("Registered mock plugin: {}", plugin_id);
        Ok(plugin_id)
    }

    /// Create a mock plugin instance
    pub async fn create_mock_instance(&self, plugin_id: &str) -> String {
        let instance_id = format!("{}-{}", plugin_id, Uuid::new_v4());
        let instance = MockPluginInstance::new(plugin_id, Some(instance_id.clone()));

        let mut instances = self.mock_instances.write().await;
        instances.insert(instance_id.clone(), instance);

        instance_id
    }

    /// Get a mock plugin instance
    pub async fn get_mock_instance(&self, instance_id: &str) -> Option<MockPluginInstance> {
        let instances = self.mock_instances.read().await;
        instances.get(instance_id).cloned()
    }

    /// Update a mock plugin instance
    pub async fn update_mock_instance<F, R>(&self, instance_id: &str, updater: F) -> Option<R>
    where
        F: FnOnce(&mut MockPluginInstance) -> R,
    {
        let mut instances = self.mock_instances.write().await;
        instances.get_mut(instance_id).map(updater)
    }

    /// Wait for a specific event
    pub async fn wait_for_event(&self, timeout: Duration) -> Option<PluginManagerEvent> {
        let mut receiver = self.event_receiver.lock().await;
        tokio::select! {
            event = receiver.recv() => event,
            _ = tokio::time::sleep(timeout) => None,
        }
    }

    /// Wait for multiple events
    pub async fn wait_for_events(&self, count: usize, timeout: Duration) -> Vec<PluginManagerEvent> {
        let mut events = Vec::new();
        let mut receiver = self.event_receiver.lock().await;

        for _ in 0..count {
            match tokio::time::timeout(timeout, receiver.recv()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) => break, // Channel closed
                Err(_) => break, // Timeout
            }
        }

        events
    }

    /// Get plugin manager metrics
    pub async fn get_metrics(&self) -> super::super::manager::MonitoringStatistics {
        let manager = self.plugin_manager.read().await;
        manager.get_monitoring_statistics().await.unwrap_or_default()
    }

    /// Get system health
    pub async fn get_system_health(&self) -> PluginResult<SystemHealthSummary> {
        let manager = self.plugin_manager.read().await;
        manager.get_system_health().await
    }

    /// List registered plugins
    pub async fn list_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        let manager = self.plugin_manager.read().await;
        manager.list_plugins().await
    }

    /// List active instances
    pub async fn list_instances(&self) -> PluginResult<Vec<PluginInstance>> {
        let manager = self.plugin_manager.read().await;
        manager.list_instances().await
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // Cleanup is handled automatically by TempDir
        info!("Test environment cleaned up");
    }
}

/// ============================================================================
/// TEST HELPERS AND ASSERTIONS
/// ============================================================================

/// Assert that a condition becomes true within timeout
pub async fn assert_eventually<F>(condition: F, timeout: Duration, message: &str)
where
    F: Fn() -> bool,
{
    let start = SystemTime::now();

    while SystemTime::now().duration_since(start).unwrap() < timeout {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("Condition not met within timeout: {}", message);
}

/// Assert that a plugin becomes healthy
pub async fn assert_plugin_healthy(
    env: &TestEnvironment,
    instance_id: &str,
    timeout: Duration,
) -> PluginResult<()> {
    let start = SystemTime::now();

    while SystemTime::now().duration_since(start).unwrap() < timeout {
        let manager = env.plugin_manager.read().await;
        if let Ok(health) = manager.get_instance_health(instance_id).await {
            if matches!(health, PluginHealthStatus::Healthy) {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(super::super::error::PluginError::generic(
        format!("Plugin {} did not become healthy within timeout", instance_id)
    ))
}

/// Assert that a plugin reaches a specific state
pub async fn assert_plugin_state(
    env: &TestEnvironment,
    instance_id: &str,
    expected_state: PluginInstanceState,
    timeout: Duration,
) -> PluginResult<()> {
    let start = SystemTime::now();

    while SystemTime::now().duration_since(start).unwrap() < timeout {
        let manager = env.plugin_manager.read().await;
        if let Ok(states) = manager.get_all_instance_states().await {
            if let Some(current_state) = states.get(instance_id) {
                if matches!(current_state, &expected_state) {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(super::super::error::PluginError::generic(
        format!("Plugin {} did not reach state {:?} within timeout", instance_id, expected_state)
    ))
}

/// Measure execution time of an async operation
pub async fn measure_time<F, T>(operation: F) -> (T, Duration)
where
    F: std::future::Future<Output = T>,
{
    let start = SystemTime::now();
    let result = operation.await;
    let duration = SystemTime::now().duration_since(start).unwrap();
    (result, duration)
}

/// Generate load for performance testing
pub async fn generate_load<F>(
    concurrent_operations: usize,
    operations_per_worker: usize,
    operation: F,
) where
    F: Fn(usize) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync + 'static,
{
    let operation = Arc::new(operation);
    let mut handles = Vec::new();

    for worker_id in 0..concurrent_operations {
        let op = operation.clone();
        let handle = tokio::spawn(async move {
            for i in 0..operations_per_worker {
                op(worker_id * operations_per_worker + i).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all workers to complete
    for handle in handles {
        let _ = handle.await;
    }
}

/// ============================================================================
/// RESOURCE MONITORING HELPERS
/// ============================================================================

/// Monitor resource usage for a plugin instance
pub struct ResourceMonitor {
    instance_id: String,
    interval: Duration,
    running: Arc<RwLock<bool>>,
    measurements: Arc<Mutex<Vec<(SystemTime, ResourceUsage)>>>,
}

impl ResourceMonitor {
    /// Create a new resource monitor
    pub fn new(instance_id: String, interval: Duration) -> Self {
        Self {
            instance_id,
            interval,
            running: Arc::new(RwLock::new(false)),
            measurements: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start monitoring
    pub async fn start(&self, plugin_manager: Arc<RwLock<PluginManagerService>>) {
        let mut running = self.running.write().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let instance_id = self.instance_id.clone();
        let interval = self.interval;
        let measurements = self.measurements.clone();
        let running_flag = self.running.clone();
        let manager = plugin_manager;

        tokio::spawn(async move {
            while *running_flag.read().await {
                if let Ok(usage) = manager.read().await.get_resource_usage(Some(&instance_id)).await {
                    let mut measurements_guard = measurements.lock().await;
                    measurements_guard.push((SystemTime::now(), usage));

                    // Keep only last 100 measurements
                    if measurements_guard.len() > 100 {
                        measurements_guard.remove(0);
                    }
                }
                tokio::time::sleep(interval).await;
            }
        });
    }

    /// Stop monitoring
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Get measurements
    pub async fn get_measurements(&self) -> Vec<(SystemTime, ResourceUsage)> {
        let measurements = self.measurements.lock().await;
        measurements.clone()
    }

    /// Get peak memory usage
    pub async fn get_peak_memory(&self) -> Option<u64> {
        let measurements = self.measurements.lock().await;
        measurements.iter()
            .map(|(_, usage)| usage.memory_bytes)
            .max()
    }

    /// Get average CPU usage
    pub async fn get_average_cpu(&self) -> Option<f64> {
        let measurements = self.measurements.lock().await;
        if measurements.is_empty() {
            return None;
        }

        let total: f64 = measurements.iter()
            .map(|(_, usage)| usage.cpu_percentage)
            .sum();

        Some(total / measurements.len() as f64)
    }
}

/// ============================================================================
/// EVENT COLLECTOR
/// ============================================================================

/// Collect events during test execution
pub struct EventCollector {
    events: Arc<Mutex<Vec<PluginManagerEvent>>>,
    receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<PluginManagerEvent>>>>,
}

impl EventCollector {
    /// Create a new event collector
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            receiver: Arc::new(Mutex::new(None)),
        }
    }

    /// Start collecting events from a receiver
    pub async fn start_collecting(&self, mut receiver: mpsc::UnboundedReceiver<PluginManagerEvent>) {
        *self.receiver.lock().await = Some(receiver);

        let events = self.events.clone();
        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                let mut events_guard = events.lock().await;
                events_guard.push(event);
            }
        });
    }

    /// Get collected events
    pub async fn get_events(&self) -> Vec<PluginManagerEvent> {
        let events = self.events.lock().await;
        events.clone()
    }

    /// Get events of a specific type
    pub async fn get_events_by_type<F>(&self, predicate: F) -> Vec<PluginManagerEvent>
    where
        F: Fn(&PluginManagerEvent) -> bool,
    {
        let events = self.events.lock().await;
        events.iter().filter(|e| predicate(e)).cloned().collect()
    }

    /// Count events
    pub async fn count_events(&self) -> usize {
        let events = self.events.lock().await;
        events.len()
    }

    /// Clear collected events
    pub async fn clear(&self) {
        let mut events = self.events.lock().await;
        events.clear();
    }

    /// Wait for a specific event type
    pub async fn wait_for_event<F>(&self, predicate: F, timeout: Duration) -> Option<PluginManagerEvent>
    where
        F: Fn(&PluginManagerEvent) -> bool,
    {
        let start = SystemTime::now();

        while SystemTime::now().duration_since(start).unwrap() < timeout {
            let events = self.events.lock().await;
            for event in events.iter() {
                if predicate(event) {
                    return Some(event.clone());
                }
            }
            drop(events);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        None
    }
}

impl Default for EventCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// COMMON TEST SCENARIOS
/// ============================================================================

/// Create a common test scenario with multiple plugins
pub async fn setup_multi_plugin_scenario() -> PluginResult<TestEnvironment> {
    let env = TestEnvironment::with_monitoring().await?;

    // Register mock plugins
    let manifests = create_mock_plugin_suite();
    for manifest in manifests {
        env.register_mock_plugin(manifest).await?;
    }

    // Allow some time for plugin discovery
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(env)
}

/// Create a stress test scenario
pub async fn setup_stress_test_scenario(plugin_count: usize) -> PluginResult<TestEnvironment> {
    let env = TestEnvironment::with_monitoring().await?;

    // Create many mock plugins
    for i in 0..plugin_count {
        let manifest = create_mock_plugin_manifest(
            &format!("stress-plugin-{}", i),
            &format!("Stress Plugin {}", i),
        );
        env.register_mock_plugin(manifest).await?;
    }

    Ok(env)
}

/// Create a failure scenario with crashing plugins
pub async fn setup_failure_scenario() -> PluginResult<TestEnvironment> {
    let env = TestEnvironment::with_monitoring().await?;

    // Register some normal plugins
    for i in 0..3 {
        let manifest = create_mock_plugin_manifest(
            &format!("normal-plugin-{}", i),
            &format!("Normal Plugin {}", i),
        );
        env.register_mock_plugin(manifest).await?;
    }

    // Register some crashing plugins
    for i in 0..2 {
        let mut manifest = create_mock_plugin_manifest(
            &format!("crashing-plugin-{}", i),
            &format!("Crashing Plugin {}", i),
        );
        manifest.metadata.insert("should_crash".to_string(), serde_json::Value::Bool(true));
        env.register_mock_plugin(manifest).await?;
    }

    Ok(env)
}