//! # Test Helper Functions
//!
//! Utility functions and helpers for writing tests more efficiently
//! and with better readability.

use super::*;
use crate::plugin_manager::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;

/// ============================================================================
/// ASYNC TESTING HELPERS
/// ============================================================================

/// Run an async test with a timeout
pub async fn run_with_timeout<F, T>(duration: Duration, future: F) -> Result<T, &'static str>
where
    F: std::future::Future<Output = T>,
{
    timeout(duration, future).await.map_err(|_| "Test timed out")
}

/// Assert that an async operation completes within a timeout
pub async fn assert_completes_within<F, T>(duration: Duration, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => panic!("Operation did not complete within {:?}", duration),
    }
}

/// Retry an async operation with backoff
pub async fn retry_async<F, T, E>(
    mut operation: F,
    max_attempts: u32,
    initial_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
    E: std::fmt::Debug,
{
    let mut delay = initial_delay;
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay *= 2; // Exponential backoff
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// ============================================================================
/// PLUGIN MANAGER HELPERS
/// ============================================================================

/// Create a plugin manager with custom components for testing
pub async fn create_plugin_manager_with_components(
    registry: Box<dyn PluginRegistry>,
    resource_manager: Box<dyn ResourceManager>,
    security_manager: Box<dyn SecurityManager>,
    health_monitor: Box<dyn HealthMonitor>,
) -> PluginManagerService {
    let config = default_test_config();

    let mut service = PluginManagerService::new(config);

    // Replace default components with test components
    service.registry = Arc::new(RwLock::new(registry));
    service.resource_manager = Arc::new(RwLock::new(resource_manager));
    service.security_manager = Arc::new(RwLock::new(security_manager));
    service.health_monitor = Arc::new(RwLock::new(health_monitor));

    service
}

/// Start a plugin manager and wait for it to be ready
pub async fn start_plugin_manager(service: &mut PluginManagerService) -> Result<(), Box<dyn std::error::Error>> {
    service.start().await?;

    // Wait a bit for initialization
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify it's running
    let health = service.health_check().await?;
    assert_eq!(health.status, ServiceStatus::Healthy);

    Ok(())
}

/// Stop a plugin manager and wait for graceful shutdown
pub async fn stop_plugin_manager(service: &mut PluginManagerService) -> Result<(), Box<dyn std::error::Error>> {
    service.stop().await?;
    Ok(())
}

/// Register a test plugin with the manager
pub async fn register_test_plugin(
    service: &mut PluginManagerService,
    plugin_id: &str,
    plugin_type: PluginType,
) -> Result<String, Box<dyn std::error::Error>> {
    let manifest = create_test_plugin_manifest(plugin_id, plugin_type);

    // Access the registry directly to register the plugin
    let mut registry = service.registry.write().await;
    let registered_id = registry.register_plugin(manifest).await?;

    Ok(registered_id)
}

/// Create and start a plugin instance
pub async fn create_and_start_instance(
    service: &mut PluginManagerService,
    plugin_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let instance_id = service.create_instance(plugin_id, None).await?;
    service.start_instance(&instance_id).await?;
    Ok(instance_id)
}

/// Wait for an event to be received
pub async fn wait_for_event_type(
    mut receiver: &mut tokio::sync::mpsc::UnboundedReceiver<PluginManagerEvent>,
    expected_type: &str,
    timeout_duration: Duration,
) -> PluginManagerEvent {
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < timeout_duration {
        match receiver.try_recv() {
            Ok(event) => {
                let event_str = format!("{:?}", event);
                if event_str.contains(expected_type) {
                    return event;
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                panic!("Event channel disconnected while waiting for: {}", expected_type);
            }
        }
    }

    panic!("Timeout waiting for event type: {}", expected_type);
}

/// Assert that no events are received within a timeout
pub async fn assert_no_events(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<PluginManagerEvent>,
    timeout_duration: Duration,
) {
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < timeout_duration {
        match receiver.try_recv() {
            Ok(event) => panic!("Unexpected event received: {:?}", event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                return; // Channel disconnected, which is fine for this test
            }
        }
    }
}

/// ============================================================================
/// ASSERTION HELPERS
/// ============================================================================

/// Assert that two plugin manifests are approximately equal (ignoring timestamps)
pub fn assert_manifests_approx_equal(manifest1: &PluginManifest, manifest2: &PluginManifest) {
    assert_eq!(manifest1.id, manifest2.id);
    assert_eq!(manifest1.name, manifest2.name);
    assert_eq!(manifest1.version, manifest2.version);
    assert_eq!(manifest1.plugin_type, manifest2.plugin_type);
    assert_eq!(manifest1.author, manifest2.author);
    assert_eq!(manifest1.entry_point, manifest2.entry_point);
    // Don't compare created_at and modified_at as they change
}

/// Assert that a plugin instance is in a specific state
pub async fn assert_instance_state(
    service: &PluginManagerService,
    instance_id: &str,
    expected_state: PluginInstanceState,
) -> Result<(), Box<dyn std::error::Error>> {
    let instances = service.instances.read().await;
    let instance = instances.get(instance_id)
        .ok_or(format!("Instance {} not found", instance_id))?;

    let actual_state = instance.get_state().await?;
    assert_eq!(actual_state, expected_state);

    Ok(())
}

/// Assert that resource usage is within expected bounds
pub fn assert_resource_usage_within_bounds(
    usage: &ResourceUsage,
    max_memory: Option<u64>,
    max_cpu: Option<f64>,
    max_disk: Option<u64>,
) {
    if let Some(max_mem) = max_memory {
        assert!(
            usage.memory_bytes <= max_mem,
            "Memory usage {} exceeds maximum {}",
            usage.memory_bytes,
            max_mem
        );
    }

    if let Some(max_cpu_percent) = max_cpu {
        assert!(
            usage.cpu_percentage <= max_cpu_percent,
            "CPU usage {}% exceeds maximum {}%",
            usage.cpu_percentage,
            max_cpu_percent
        );
    }

    if let Some(max_disk_bytes) = max_disk {
        assert!(
            usage.disk_bytes <= max_disk_bytes,
            "Disk usage {} exceeds maximum {}",
            usage.disk_bytes,
            max_disk_bytes
        );
    }
}

/// Assert that a plugin has specific capabilities
pub fn assert_plugin_has_capabilities(
    manifest: &PluginManifest,
    expected_capabilities: &[PluginCapability],
) {
    for expected_cap in expected_capabilities {
        assert!(
            manifest.capabilities.contains(expected_cap),
            "Plugin {} missing expected capability: {:?}",
            manifest.id,
            expected_cap
        );
    }
}

/// Assert that a plugin has specific permissions
pub fn assert_plugin_has_permissions(
    manifest: &PluginManifest,
    expected_permissions: &[PluginPermission],
) {
    for expected_perm in expected_permissions {
        assert!(
            manifest.permissions.contains(expected_perm),
            "Plugin {} missing expected permission: {:?}",
            manifest.id,
            expected_perm
        );
    }
}

/// ============================================================================
/// PERFORMANCE TESTING HELPERS
/// ============================================================================

/// Measure execution time of an async operation
pub async fn measure_async<F, T>(operation: F) -> (T, Duration)
where
    F: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    let result = operation.await;
    let duration = start.elapsed();
    (result, duration)
}

/// Assert that an operation completes within a time limit
pub async fn assert_completes_within_time<F, T>(
    max_duration: Duration,
    operation: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    let (result, actual_duration) = measure_async(operation).await;

    assert!(
        actual_duration <= max_duration,
        "Operation took {:?}, which exceeds maximum allowed duration of {:?}",
        actual_duration,
        max_duration
    );

    result
}

/// Benchmark an async operation multiple times
pub async fn benchmark_async<F, T>(
    operation: F,
    iterations: usize,
) -> Vec<Duration>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>> + Send + Sync,
    T: Send + 'static,
{
    let mut durations = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let (_, duration) = measure_async(operation()).await;
        durations.push(duration);
    }

    durations
}

/// Calculate statistics from duration measurements
pub fn calculate_duration_stats(durations: &[Duration]) -> DurationStats {
    if durations.is_empty() {
        return DurationStats::default();
    }

    let mut sorted_durations = durations.to_vec();
    sorted_durations.sort();

    let total: Duration = sorted_durations.iter().sum();
    let mean = total / sorted_durations.len() as u32;

    let min = sorted_durations[0];
    let max = sorted_durations[sorted_durations.len() - 1];

    let median = if sorted_durations.len() % 2 == 0 {
        let mid = sorted_durations.len() / 2;
        (sorted_durations[mid - 1] + sorted_durations[mid]) / 2
    } else {
        sorted_durations[sorted_durations.len() / 2]
    };

    // Calculate 95th percentile
    let percentile_95_idx = (sorted_durations.len() as f64 * 0.95) as usize;
    let percentile_95 = sorted_durations[percentile_95_idx.min(sorted_durations.len() - 1)];

    DurationStats {
        min,
        max,
        mean,
        median,
        percentile_95,
        total,
        count: sorted_durations.len(),
    }
}

/// Duration statistics
#[derive(Debug, Clone)]
pub struct DurationStats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub percentile_95: Duration,
    pub total: Duration,
    pub count: usize,
}

impl Default for DurationStats {
    fn default() -> Self {
        Self {
            min: Duration::ZERO,
            max: Duration::ZERO,
            mean: Duration::ZERO,
            median: Duration::ZERO,
            percentile_95: Duration::ZERO,
            total: Duration::ZERO,
            count: 0,
        }
    }
}

/// ============================================================================
/// RESOURCE TESTING HELPERS
/// ============================================================================

/// Simulate resource usage growth over time
pub async fn simulate_resource_growth(
    instance_id: &str,
    resource_manager: &dyn ResourceManager,
    steps: usize,
    step_duration: Duration,
) {
    for step in 1..=steps {
        let usage = ResourceUsage {
            memory_bytes: (step * 50 * 1024 * 1024) as u64, // 50MB per step
            cpu_percentage: (step * 10) as f64, // 10% per step
            disk_bytes: (step * 10 * 1024 * 1024) as u64, // 10MB per step
            network_bytes: (step * 5 * 1024 * 1024) as u64, // 5MB per step
            open_files: step as u32 * 2,
            active_threads: step as u32,
            child_processes: if step > 5 { 1 } else { 0 },
            measured_at: SystemTime::now(),
        };

        // Update the mock resource manager's usage tracking
        if let Some(mock_manager) = resource_manager.as_any().downcast_ref::<MockResourceManager>() {
            mock_manager.set_instance_usage(instance_id, usage).await;
        }

        tokio::time::sleep(step_duration).await;
    }
}

/// Check if resource limits are being enforced
pub async fn check_resource_enforcement(
    instance_id: &str,
    resource_manager: &dyn ResourceManager,
) -> bool {
    match resource_manager.enforce_limits(instance_id).await {
        Ok(has_violation) => has_violation,
        Err(_) => false,
    }
}

/// ============================================================================
/// SECURITY TESTING HELPERS
/// ============================================================================

/// Test various security scenarios
pub async fn test_security_scenario(
    security_manager: &dyn SecurityManager,
    plugin_id: &str,
    operation: &str,
    permission: &PluginPermission,
) -> SecurityTestResult {
    let permission_result = security_manager.check_permission(plugin_id, permission).await;
    let policy_result = security_manager.enforce_security_policy(plugin_id, operation).await;

    SecurityTestResult {
        plugin_id: plugin_id.to_string(),
        operation: operation.to_string(),
        permission: permission.clone(),
        permission_allowed: permission_result.unwrap_or(false),
        policy_allowed: policy_result.unwrap_or(false),
        timestamp: SystemTime::now(),
    }
}

/// Result of a security test
#[derive(Debug, Clone)]
pub struct SecurityTestResult {
    pub plugin_id: String,
    pub operation: String,
    pub permission: PluginPermission,
    pub permission_allowed: bool,
    pub policy_allowed: bool,
    pub timestamp: SystemTime,
}

/// ============================================================================
/// CLEANUP HELPERS
/// ============================================================================

/// Clean up test resources
pub async fn cleanup_test_resources(service: &mut PluginManagerService) {
    // Stop all instances
    let instance_ids: Vec<String> = {
        let instances = service.instances.read().await;
        instances.keys().cloned().collect()
    };

    for instance_id in instance_ids {
        let _ = service.stop_instance(&instance_id).await;
    }

    // Stop the service
    let _ = service.stop().await;
}

/// Clean up temporary files and directories
pub async fn cleanup_temp_files(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        tokio::fs::remove_dir_all(path).await?;
    }
    Ok(())
}

/// Create a temporary directory for testing
pub async fn create_temp_dir(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let temp_dir = std::env::temp_dir().join(format!("crucible-test-{}", name));
    tokio::fs::create_dir_all(&temp_dir).await?;
    Ok(temp_dir)
}