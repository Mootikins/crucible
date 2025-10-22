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

// ============================================================================
// LIFECYCLE MANAGEMENT TEST UTILITIES
// ============================================================================

use crate::plugin_manager::lifecycle_manager::*;
use crate::plugin_manager::state_machine::*;
use crate::plugin_manager::dependency_resolver::*;
use crate::plugin_manager::lifecycle_policy::*;
use crate::plugin_manager::automation_engine::*;
use crate::plugin_manager::batch_operations::*;

/// Create test lifecycle manager configuration
pub fn test_lifecycle_config() -> LifecycleConfig {
    LifecycleConfig {
        auto_start: false,
        shutdown_timeout: Duration::from_millis(500),
        startup_order: vec![],
        shutdown_order: vec![],
        concurrent_startup_limit: Some(2),
    }
}

/// Create test plugin instance
pub fn create_test_plugin_instance(id: &str, plugin_id: &str) -> PluginInstance {
    PluginInstance {
        instance_id: id.to_string(),
        plugin_id: plugin_id.to_string(),
        version: "1.0.0".to_string(),
        state: PluginInstanceState::Created,
        health_status: PluginHealthStatus::Unknown,
        config: std::collections::HashMap::new(),
        process_info: None,
        resource_usage: None,
        metadata: std::collections::HashMap::new(),
        created_at: SystemTime::now(),
        updated_at: SystemTime::now(),
        last_health_check: None,
        dependencies: vec![],
        dependents: vec![],
    }
}

/// Create test state machine
pub fn create_test_state_machine() -> PluginStateMachine {
    PluginStateMachine::new()
}

/// Create test dependency resolver
pub fn create_test_dependency_resolver() -> DependencyResolver {
    DependencyResolver::new()
}

/// Create test lifecycle policy engine
pub fn create_test_lifecycle_policy() -> LifecyclePolicy {
    LifecyclePolicy::new()
}

/// Create test automation rule
pub fn create_test_automation_rule(id: &str, name: &str) -> AutomationRule {
    AutomationRule {
        id: id.to_string(),
        name: name.to_string(),
        description: format!("Test automation rule: {}", name),
        version: "1.0.0".to_string(),
        enabled: true,
        priority: AutomationPriority::Normal,
        triggers: vec![],
        conditions: vec![],
        actions: vec![],
        scope: AutomationScope {
            plugins: vec![],
            instances: vec![],
            environments: vec![],
            exclude_plugins: vec![],
            exclude_instances: vec![],
        },
        schedule: None,
        limits: None,
        metadata: AutomationMetadata {
            created_at: SystemTime::now(),
            created_by: "test".to_string(),
            updated_at: SystemTime::now(),
            updated_by: "test".to_string(),
            tags: vec!["test".to_string()],
            documentation: None,
            additional_info: std::collections::HashMap::new(),
        },
    }
}

/// Create test batch operation
pub fn create_test_batch_operation(id: &str, name: &str) -> BatchOperation {
    BatchOperation {
        batch_id: id.to_string(),
        name: name.to_string(),
        description: format!("Test batch operation: {}", name),
        operations: vec![],
        strategy: BatchExecutionStrategy::Sequential {
            stop_on_failure: true,
            failure_handling: FailureHandling::Stop,
        },
        config: BatchConfig::default(),
        scope: BatchScope::default(),
        metadata: BatchMetadata::default(),
    }
}

/// Create test lifecycle operation request
pub fn create_test_lifecycle_operation_request(
    operation: LifecycleOperation,
    priority: OperationPriority,
) -> LifecycleOperationRequest {
    LifecycleOperationRequest {
        operation_id: uuid::Uuid::new_v4().to_string(),
        operation,
        requested_at: SystemTime::now(),
        priority,
        timeout: Some(Duration::from_secs(30)),
        requester: RequesterContext {
            requester_id: "test".to_string(),
            requester_type: RequesterType::System,
            source: "test".to_string(),
            auth_token: None,
            metadata: std::collections::HashMap::new(),
        },
        parameters: std::collections::HashMap::new(),
        depends_on: vec![],
        rollback_config: None,
    }
}

/// Wait for state transition
pub async fn wait_for_state_transition(
    state_machine: &PluginStateMachine,
    instance_id: &str,
    expected_state: PluginInstanceState,
    timeout: Duration,
) -> PluginResult<()> {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match state_machine.get_state(instance_id).await {
            Ok(current_state) if current_state == expected_state => return Ok(()),
            Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }

    Err(PluginError::timeout(format!(
        "State transition to {:?} not reached within timeout for instance {}",
        expected_state, instance_id
    )))
}

/// Wait for health status change
pub async fn wait_for_health_status(
    state_machine: &PluginStateMachine,
    instance_id: &str,
    expected_health: PluginHealthStatus,
    timeout: Duration,
) -> PluginResult<()> {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        match state_machine.get_health_status(instance_id).await {
            Ok(current_health) if current_health == expected_health => return Ok(()),
            Ok(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }

    Err(PluginError::timeout(format!(
        "Health status {:?} not reached within timeout for instance {}",
        expected_health, instance_id
    )))
}

/// Create test dependency graph
pub fn create_test_dependency_graph() -> DependencyGraph {
    let mut graph = DependencyGraph::new();

    // Add some test nodes and edges
    graph.add_node("plugin-a".to_string(), DependencyNode::Plugin {
        plugin_id: "plugin-a".to_string(),
        version: "1.0.0".to_string(),
        required: true,
        health_required: false,
    });

    graph.add_node("plugin-b".to_string(), DependencyNode::Plugin {
        plugin_id: "plugin-b".to_string(),
        version: "1.0.0".to_string(),
        required: true,
        health_required: false,
    });

    graph.add_dependency("plugin-a".to_string(), "plugin-b".to_string()).unwrap();

    graph
}

/// Test performance benchmark helper
pub async fn benchmark_operation<F, Fut>(
    operation_name: &str,
    operation: F,
    iterations: usize,
) -> (Duration, Vec<Duration>)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut durations = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = std::time::Instant::now();
        operation().await;
        durations.push(start.elapsed());
    }

    let total_time: Duration = durations.iter().sum();
    let average_time = total_time / iterations as u32;

    println!("Benchmark: {} - Average time: {:?}", operation_name, average_time);

    (average_time, durations)
}