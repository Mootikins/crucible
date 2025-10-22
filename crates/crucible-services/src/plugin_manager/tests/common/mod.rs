//! # Common Test Utilities
//!
//! Shared test utilities, mock implementations, and helper functions
//! used across the PluginManager test suite.

pub mod mocks;
pub mod fixtures;
pub mod helpers;
pub mod mock_traits;

// Re-export common utilities
pub use mocks::*;
pub use fixtures::*;
pub use helpers::*;
pub use mock_traits::*;

use super::*;
use crate::plugin_manager::*;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Test timeout for async operations
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Short timeout for quick operations
pub const SHORT_TIMEOUT: Duration = Duration::from_millis(500);

/// Default test configuration
pub fn default_test_config() -> PluginManagerConfig {
    PluginManagerConfig {
        plugin_directories: vec![
            std::path::PathBuf::from("/tmp/test-plugins"),
        ],
        auto_discovery: AutoDiscoveryConfig {
            enabled: false, // Disable auto-discovery for tests
            scan_interval: Duration::from_secs(60),
            file_patterns: vec!["*.json".to_string()],
            watch_filesystem: false,
            auto_install: false,
            validation: DiscoveryValidationConfig {
                validate_manifests: true,
                validate_signatures: false,
                security_scan: false, // Disable for faster tests
                validate_dependencies: false,
                strict: false,
            },
        },
        security: SecurityConfig {
            default_sandbox: SandboxConfig {
                enabled: false, // Disable sandboxing for tests
                ..Default::default()
            },
            ..Default::default()
        },
        resource_management: ResourceManagementConfig {
            global_limits: ResourceLimits {
                max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
                max_cpu_percentage: Some(80.0),
                max_concurrent_operations: Some(100),
                ..Default::default()
            },
            per_plugin_limits: ResourceLimits {
                max_memory_bytes: Some(256 * 1024 * 1024), // 256MB
                max_cpu_percentage: Some(25.0),
                max_concurrent_operations: Some(10),
                operation_timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            },
            monitoring: ResourceMonitoringConfig {
                enabled: true,
                interval: Duration::from_millis(100), // Fast for tests
                metrics: vec![
                    ResourceMetric::CpuUsage,
                    ResourceMetric::MemoryUsage,
                ],
                retention_period: Duration::from_secs(60),
            },
            enforcement: ResourceEnforcementConfig {
                enabled: true,
                strategy: EnforcementStrategy::Soft,
                grace_period: Duration::from_millis(500),
                limit_exceeded_action: LimitExceededAction::Warn,
            },
        },
        health_monitoring: HealthMonitoringConfig {
            enabled: true,
            check_interval: Duration::from_millis(200), // Fast for tests
            check_timeout: Duration::from_millis(100),
            strategies: vec![
                HealthCheckStrategy {
                    name: "process".to_string(),
                    strategy_type: HealthCheckType::Process,
                    config: std::collections::HashMap::new(),
                    enabled: true,
                },
            ],
            unhealthy_threshold: 2, // Lower for tests
            recovery: RecoveryConfig {
                enabled: true,
                max_restart_attempts: 2, // Lower for tests
                restart_delay: Duration::from_millis(100),
                backoff_strategy: BackoffStrategy::Fixed,
                escalation: EscalationConfig {
                    enabled: false, // Disable for tests
                    thresholds: vec![],
                    actions: vec![],
                },
            },
        },
        communication: CommunicationConfig {
            ipc: IpcConfig {
                transport_type: IpcTransportType::UnixSocket,
                socket_path: Some(std::path::PathBuf::from("/tmp/test-plugin-ipc.sock")),
                port_range: Some(9000..9100),
                connection_timeout: Duration::from_millis(500),
                max_message_size: 1024 * 1024, // 1MB
                pool_size: 2, // Small for tests
            },
            ..Default::default()
        },
        logging: LoggingConfig {
            level: LogLevel::Debug,
            file_path: None, // No file logging for tests
            format: LogFormat::Plain,
            rotation: LogRotationConfig {
                enabled: false,
                max_file_size: 0,
                max_files: 0,
                rotation_interval: None,
            },
            plugin_logging: PluginLoggingConfig {
                capture_stdout: false,
                capture_stderr: false,
                separate_files: false,
                log_directory: None,
            },
        },
        lifecycle: LifecycleConfig {
            auto_start: false, // Disable auto-start for tests
            shutdown_timeout: Duration::from_millis(500),
            startup_order: vec![],
            shutdown_order: vec![],
            concurrent_startup_limit: Some(2), // Small for tests
        },
        performance: PerformanceConfig {
            thread_pool_size: 2, // Small for tests
            async_runtime: AsyncRuntimeConfig {
                worker_threads: Some(2),
                max_blocking_threads: 4,
                thread_stack_size: Some(1024 * 1024), // 1MB
            },
            caching: CachingConfig {
                enabled: true,
                max_size: 1024 * 1024, // 1MB
                ttl: Duration::from_secs(60),
                eviction_policy: CacheEvictionPolicy::LRU,
            },
            optimization: OptimizationConfig {
                enabled: false, // Disable for tests
                ..Default::default()
            },
        },
    }
}

/// Test assertion helper with detailed error messages
#[macro_export]
macro_rules! assert_eventually {
    ($condition:expr, $timeout:expr, $msg:expr) => {
        let start = std::time::Instant::now();
        while start.elapsed() < $timeout {
            if $condition {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("Condition not met within timeout: {}", $msg);
    };
}

/// Helper to create a test plugin manager service
pub async fn create_test_plugin_manager() -> PluginManagerService {
    let config = default_test_config();
    PluginManagerService::new(config)
}

/// Helper to start and stop a plugin manager service
pub async fn with_plugin_manager<F, Fut>(test_fn: F)
where
    F: FnOnce(PluginManagerService) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut service = create_test_plugin_manager().await;

    // Start the service
    service.start().await.expect("Failed to start plugin manager");

    // Run the test
    test_fn(service).await;

    // Service will be automatically stopped when dropped
}

/// Helper to create a test event receiver
pub async fn create_event_receiver(service: &mut PluginManagerService) -> mpsc::UnboundedReceiver<PluginManagerEvent> {
    service.subscribe_events().await
}

/// Helper to wait for specific event
pub async fn wait_for_event(
    mut receiver: mpsc::UnboundedReceiver<PluginManagerEvent>,
    event_type: &str,
    timeout: Duration,
) -> PluginManagerEvent {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match receiver.try_recv() {
            Ok(event) => {
                let event_str = format!("{:?}", event);
                if event_str.contains(event_type) {
                    return event;
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                panic!("Event channel disconnected while waiting for: {}", event_type);
            }
        }
    }

    panic!("Timeout waiting for event: {}", event_type);
}

/// Test resource usage generator
pub fn generate_test_resource_usage() -> ResourceUsage {
    ResourceUsage {
        memory_bytes: 128 * 1024 * 1024, // 128MB
        cpu_percentage: 25.5,
        disk_bytes: 1024 * 1024, // 1MB
        network_bytes: 512 * 1024, // 512KB
        open_files: 5,
        active_threads: 2,
        child_processes: 0,
        measured_at: SystemTime::now(),
    }
}

/// Test performance metrics generator
pub fn generate_test_performance_metrics() -> PerformanceMetrics {
    PerformanceMetrics {
        request_times: vec![
            Duration::from_millis(10),
            Duration::from_millis(15),
            Duration::from_millis(8),
        ],
        memory_usage: 64 * 1024 * 1024, // 64MB
        cpu_usage: 15.0,
        active_connections: 5,
        queue_sizes: std::collections::HashMap::from([
            ("default".to_string(), 2),
            ("high_priority".to_string(), 0),
        ]),
        custom_metrics: std::collections::HashMap::from([
            ("operations_per_second".to_string(), 50.0),
            ("average_response_time".to_string(), 11.0),
        ]),
        timestamp: SystemTime::now(),
    }
}