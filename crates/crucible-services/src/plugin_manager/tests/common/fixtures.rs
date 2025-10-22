//! # Test Fixtures and Sample Data
//!
//! Sample plugin manifests, configurations, and test data used across
//! the PluginManager test suite.

use super::*;
use crate::plugin_manager::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// ============================================================================
/// PLUGIN MANIFEST FIXTURES
/// ============================================================================

/// Create a test plugin manifest
pub fn create_test_plugin_manifest(plugin_id: &str, plugin_type: PluginType) -> PluginManifest {
    PluginManifest {
        id: plugin_id.to_string(),
        name: format!("Test Plugin {}", plugin_id),
        description: format!("A test plugin for {}", plugin_id),
        version: "1.0.0".to_string(),
        plugin_type,
        author: "Test Author".to_string(),
        license: Some("MIT".to_string()),
        homepage: Some(format!("https://example.com/plugins/{}", plugin_id)),
        repository: Some(format!("https://github.com/example/{}", plugin_id)),
        tags: vec!["test".to_string(), "demo".to_string()],
        entry_point: PathBuf::from(format!("/tmp/plugins/{}/main.rn", plugin_id)),
        capabilities: vec![
            PluginCapability::IpcCommunication,
            PluginCapability::FileSystem {
                read_paths: vec!["/tmp".to_string()],
                write_paths: vec!["/tmp".to_string()],
            },
        ],
        permissions: vec![
            PluginPermission::FileSystemRead,
            PluginPermission::IpcCommunication,
        ],
        dependencies: vec![],
        resource_limits: ResourceLimits {
            max_memory_bytes: Some(128 * 1024 * 1024), // 128MB
            max_cpu_percentage: Some(25.0),
            max_concurrent_operations: Some(5),
            operation_timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        },
        sandbox_config: SandboxConfig {
            enabled: true,
            sandbox_type: SandboxType::Process,
            namespace_isolation: true,
            filesystem_isolation: true,
            network_isolation: false,
            process_isolation: true,
            resource_limits: ResourceLimits::default(),
            allowed_syscalls: vec!["read".to_string(), "write".to_string()],
            blocked_syscalls: vec![],
            mount_points: vec![],
            environment: HashMap::new(),
        },
        environment: HashMap::from([
            ("TEST_MODE".to_string(), "true".to_string()),
            ("LOG_LEVEL".to_string(), "debug".to_string()),
        ]),
        config_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "timeout": {
                    "type": "integer",
                    "default": 30
                },
                "retries": {
                    "type": "integer",
                    "default": 3
                }
            }
        })),
        min_crucible_version: Some("1.0.0".to_string()),
        max_crucible_version: None,
        created_at: SystemTime::now(),
        modified_at: SystemTime::now(),
    }
}

/// Create a Rune plugin manifest
pub fn create_rune_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Rune);
    manifest.entry_point = PathBuf::from(format!("/tmp/plugins/{}/script.rn", plugin_id));
    manifest.capabilities.push(PluginCapability::ScriptExecution);
    manifest.permissions.push(PluginPermission::FileExecute);
    manifest
}

/// Create a binary plugin manifest
pub fn create_binary_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Binary);
    manifest.entry_point = PathBuf::from(format!("/tmp/plugins/{}/bin/plugin", plugin_id));
    manifest.permissions.push(PluginPermission::ProcessControl);
    manifest
}

/// Create a WASM plugin manifest
pub fn create_wasm_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Wasm);
    manifest.entry_point = PathBuf::from(format!("/tmp/plugins/{}/plugin.wasm", plugin_id));
    manifest
}

/// Create a Python plugin manifest
pub fn create_python_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Python);
    manifest.entry_point = PathBuf::from(format!("/tmp/plugins/{}/main.py", plugin_id));
    manifest.permissions.push(PluginPermission::FileExecute);
    manifest
}

/// Create a plugin with dependencies
pub fn create_plugin_with_dependencies(
    plugin_id: &str,
    dependencies: Vec<PluginDependency>,
) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Rune);
    manifest.dependencies = dependencies;
    manifest
}

/// Create a plugin with security issues
pub fn create_insecure_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Rune);
    manifest.capabilities.push(PluginCapability::SystemCalls {
        allowed_calls: vec!["execve".to_string(), "ptrace".to_string()],
    });
    manifest.permissions.push(PluginPermission::SystemCalls);
    manifest.permissions.push(PluginPermission::HardwareAccess);
    manifest.sandbox_config.enabled = false; // No sandboxing
    manifest
}

/// Create a resource-intensive plugin manifest
pub fn create_resource_intensive_plugin_manifest(plugin_id: &str) -> PluginManifest {
    let mut manifest = create_test_plugin_manifest(plugin_id, PluginType::Binary);
    manifest.resource_limits = ResourceLimits {
        max_memory_bytes: Some(2 * 1024 * 1024 * 1024), // 2GB
        max_cpu_percentage: Some(90.0),
        max_concurrent_operations: Some(50),
        operation_timeout: Some(std::time::Duration::from_secs(300)),
        ..Default::default()
    };
    manifest
}

/// ============================================================================
/// PLUGIN REGISTRY ENTRY FIXTURES
/// ============================================================================

/// Create a test plugin registry entry
pub fn create_test_registry_entry(plugin_id: &str, plugin_type: PluginType) -> PluginRegistryEntry {
    PluginRegistryEntry {
        manifest: create_test_plugin_manifest(plugin_id, plugin_type),
        install_path: PathBuf::from(format!("/tmp/test-plugins/{}", plugin_id)),
        installed_at: SystemTime::now(),
        status: PluginRegistryStatus::Installed,
        validation_results: Some(PluginValidationResults {
            valid: true,
            security_validation: SecurityValidationResult {
                passed: true,
                issues: vec![],
                security_level: SecurityLevel::Basic,
                recommendations: vec![],
            },
            dependency_validation: DependencyValidationResult {
                passed: true,
                missing_dependencies: vec![],
                version_conflicts: vec![],
                optional_missing: vec![],
            },
            compatibility_validation: CompatibilityValidationResult {
                passed: true,
                crucible_version_compatible: true,
                platform_compatible: true,
                architecture_compatible: true,
                issues: vec![],
            },
            validated_at: SystemTime::now(),
        }),
        instance_ids: vec![],
    }
}

/// Create a disabled plugin registry entry
pub fn create_disabled_registry_entry(plugin_id: &str) -> PluginRegistryEntry {
    let mut entry = create_test_registry_entry(plugin_id, PluginType::Rune);
    entry.status = PluginRegistryStatus::Disabled;
    entry
}

/// Create an invalid plugin registry entry
pub fn create_invalid_registry_entry(plugin_id: &str) -> PluginRegistryEntry {
    let mut entry = create_test_registry_entry(plugin_id, PluginType::Rune);
    entry.status = PluginRegistryStatus::Error("Invalid manifest".to_string());
    if let Some(ref mut validation_results) = entry.validation_results {
        validation_results.valid = false;
        validation_results.security_validation.passed = false;
        validation_results.security_validation.issues.push(SecurityIssue {
            issue_type: SecurityIssueType::FileSystemAccess,
            severity: SecuritySeverity::High,
            description: "Plugin requests unrestricted file system access".to_string(),
            location: Some("manifest.json".to_string()),
            recommendation: Some("Restrict file system access to specific directories".to_string()),
        });
    }
    entry
}

/// ============================================================================
/// PLUGIN INSTANCE FIXTURES
/// ============================================================================

/// Create a test plugin instance
pub fn create_test_plugin_instance(instance_id: &str, plugin_id: &str) -> PluginInstance {
    PluginInstance {
        instance_id: instance_id.to_string(),
        plugin_id: plugin_id.to_string(),
        state: PluginInstanceState::Created,
        pid: None,
        created_at: SystemTime::now(),
        started_at: None,
        last_activity: None,
        config: HashMap::from([
            ("timeout".to_string(), serde_json::Value::Number(30.into())),
            ("retries".to_string(), serde_json::Value::Number(3.into())),
            ("debug".to_string(), serde_json::Value::Bool(true)),
        ]),
        resource_usage: ResourceUsage::default(),
        resource_limits: ResourceLimits {
            max_memory_bytes: Some(128 * 1024 * 1024),
            max_cpu_percentage: Some(25.0),
            max_concurrent_operations: Some(5),
            operation_timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        },
        health_status: PluginHealthStatus::Unknown,
        error_info: None,
        restart_count: 0,
        execution_stats: PluginExecutionStats::default(),
    }
}

/// Create a running plugin instance
pub fn create_running_plugin_instance(instance_id: &str, plugin_id: &str) -> PluginInstance {
    let mut instance = create_test_plugin_instance(instance_id, plugin_id);
    instance.state = PluginInstanceState::Running;
    instance.pid = Some(12345);
    instance.started_at = Some(SystemTime::now());
    instance.last_activity = Some(SystemTime::now());
    instance.health_status = PluginHealthStatus::Healthy;
    instance.resource_usage = ResourceUsage {
        memory_bytes: 64 * 1024 * 1024, // 64MB
        cpu_percentage: 15.5,
        disk_bytes: 1024 * 1024, // 1MB
        network_bytes: 512 * 1024, // 512KB
        open_files: 3,
        active_threads: 2,
        child_processes: 0,
        measured_at: SystemTime::now(),
    };
    instance
}

/// Create a failed plugin instance
pub fn create_failed_plugin_instance(instance_id: &str, plugin_id: &str, error: &str) -> PluginInstance {
    let mut instance = create_test_plugin_instance(instance_id, plugin_id);
    instance.state = PluginInstanceState::Error(error.to_string());
    instance.health_status = PluginHealthStatus::Unhealthy;
    instance.error_info = Some(PluginErrorInfo {
        code: "EXECUTION_ERROR".to_string(),
        message: error.to_string(),
        stack_trace: Some("at plugin_main (line 42)".to_string()),
        timestamp: SystemTime::now(),
        occurrence_count: 1,
    });
    instance
}

/// Create a crashed plugin instance
pub fn create_crashed_plugin_instance(instance_id: &str, plugin_id: &str) -> PluginInstance {
    let mut instance = create_test_plugin_instance(instance_id, plugin_id);
    instance.state = PluginInstanceState::Crashed;
    instance.health_status = PluginHealthStatus::Unhealthy;
    instance.restart_count = 2;
    instance.execution_stats.failed_executions = 1;
    instance
}

/// ============================================================================
/// RESOURCE USAGE FIXTURES
/// ============================================================================

/// Create low resource usage
pub fn create_low_resource_usage() -> ResourceUsage {
    ResourceUsage {
        memory_bytes: 32 * 1024 * 1024, // 32MB
        cpu_percentage: 5.0,
        disk_bytes: 512 * 1024, // 512KB
        network_bytes: 256 * 1024, // 256KB
        open_files: 2,
        active_threads: 1,
        child_processes: 0,
        measured_at: SystemTime::now(),
    }
}

/// Create moderate resource usage
pub fn create_moderate_resource_usage() -> ResourceUsage {
    ResourceUsage {
        memory_bytes: 256 * 1024 * 1024, // 256MB
        cpu_percentage: 35.5,
        disk_bytes: 10 * 1024 * 1024, // 10MB
        network_bytes: 5 * 1024 * 1024, // 5MB
        open_files: 10,
        active_threads: 4,
        child_processes: 1,
        measured_at: SystemTime::now(),
    }
}

/// Create high resource usage
pub fn create_high_resource_usage() -> ResourceUsage {
    ResourceUsage {
        memory_bytes: 1024 * 1024 * 1024, // 1GB
        cpu_percentage: 85.0,
        disk_bytes: 100 * 1024 * 1024, // 100MB
        network_bytes: 50 * 1024 * 1024, // 50MB
        open_files: 50,
        active_threads: 10,
        child_processes: 3,
        measured_at: SystemTime::now(),
    }
}

/// Create resource usage that violates limits
pub fn create_violating_resource_usage() -> ResourceUsage {
    ResourceUsage {
        memory_bytes: 2 * 1024 * 1024 * 1024, // 2GB (exceeds typical limits)
        cpu_percentage: 95.0, // High CPU usage
        disk_bytes: 500 * 1024 * 1024, // 500MB
        network_bytes: 100 * 1024 * 1024, // 100MB
        open_files: 100, // Many file descriptors
        active_threads: 20, // Many threads
        child_processes: 5, // Many child processes
        measured_at: SystemTime::now(),
    }
}

/// ============================================================================
/// CONFIGURATION FIXTURES
/// ============================================================================

/// Create minimal test configuration
pub fn create_minimal_config() -> PluginManagerConfig {
    PluginManagerConfig {
        plugin_directories: vec![PathBuf::from("/tmp/minimal-plugins")],
        auto_discovery: AutoDiscoveryConfig {
            enabled: false,
            scan_interval: std::time::Duration::from_secs(300),
            file_patterns: vec![],
            watch_filesystem: false,
            auto_install: false,
            validation: DiscoveryValidationConfig {
                validate_manifests: false,
                validate_signatures: false,
                security_scan: false,
                validate_dependencies: false,
                strict: false,
            },
        },
        security: SecurityConfig {
            default_sandbox: SandboxConfig {
                enabled: false,
                sandbox_type: SandboxType::None,
                namespace_isolation: false,
                filesystem_isolation: false,
                network_isolation: false,
                process_isolation: false,
                resource_limits: ResourceLimits::default(),
                allowed_syscalls: vec![],
                blocked_syscalls: vec![],
                mount_points: vec![],
                environment: HashMap::new(),
            },
            trusted_signatures: vec![],
            policies: SecurityPolicyConfig {
                default_level: SecurityLevel::None,
                level_configs: HashMap::new(),
                custom_rules: vec![],
            },
            audit: AuditConfig {
                enabled: false,
                log_file: None,
                audit_events: vec![],
                retention_period: None,
                real_time_monitoring: false,
                alert_thresholds: AlertThresholds {
                    errors_per_minute: None,
                    memory_usage_percent: None,
                    cpu_usage_percent: None,
                    failed_login_attempts: None,
                },
            },
        },
        resource_management: ResourceManagementConfig {
            global_limits: ResourceLimits::default(),
            per_plugin_limits: ResourceLimits::default(),
            monitoring: ResourceMonitoringConfig {
                enabled: false,
                interval: std::time::Duration::from_secs(60),
                metrics: vec![],
                retention_period: std::time::Duration::from_secs(3600),
            },
            enforcement: ResourceEnforcementConfig {
                enabled: false,
                strategy: EnforcementStrategy::Hard,
                grace_period: std::time::Duration::from_secs(30),
                limit_exceeded_action: LimitExceededAction::Terminate,
            },
        },
        health_monitoring: HealthMonitoringConfig {
            enabled: false,
            check_interval: std::time::Duration::from_secs(60),
            check_timeout: std::time::Duration::from_secs(30),
            strategies: vec![],
            unhealthy_threshold: 5,
            recovery: RecoveryConfig {
                enabled: false,
                max_restart_attempts: 0,
                restart_delay: std::time::Duration::from_secs(10),
                backoff_strategy: BackoffStrategy::Fixed,
                escalation: EscalationConfig {
                    enabled: false,
                    thresholds: vec![],
                    actions: vec![],
                },
            },
        },
        communication: CommunicationConfig {
            ipc: IpcConfig {
                transport_type: IpcTransportType::UnixSocket,
                socket_path: Some(PathBuf::from("/tmp/minimal-ipc.sock")),
                port_range: None,
                connection_timeout: std::time::Duration::from_secs(5),
                max_message_size: 1024 * 1024,
                pool_size: 1,
            },
            message_handling: MessageHandlingConfig {
                default_timeout: std::time::Duration::from_secs(30),
                max_queue_size: 100,
                priority_handling: false,
                persistence: MessagePersistenceConfig {
                    enabled: false,
                    storage_path: None,
                    max_messages: 0,
                    retention_period: std::time::Duration::from_secs(3600),
                },
            },
            security: CommunicationSecurityConfig {
                encryption_enabled: false,
                encryption_algorithm: None,
                authentication_enabled: false,
                authentication_method: None,
                certificate_path: None,
                private_key_path: None,
            },
        },
        logging: LoggingConfig {
            level: LogLevel::Error,
            file_path: None,
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
            auto_start: false,
            shutdown_timeout: std::time::Duration::from_secs(5),
            startup_order: vec![],
            shutdown_order: vec![],
            concurrent_startup_limit: Some(1),
        },
        performance: PerformanceConfig {
            thread_pool_size: 1,
            async_runtime: AsyncRuntimeConfig {
                worker_threads: Some(1),
                max_blocking_threads: 2,
                thread_stack_size: Some(512 * 1024),
            },
            caching: CachingConfig {
                enabled: false,
                max_size: 0,
                ttl: std::time::Duration::from_secs(3600),
                eviction_policy: CacheEvictionPolicy::LRU,
            },
            optimization: OptimizationConfig {
                enabled: false,
                memory_optimization: OptimizationLevel::None,
                cpu_optimization: OptimizationLevel::None,
                network_optimization: OptimizationLevel::None,
            },
        },
    }
}

/// Create strict security configuration
pub fn create_strict_security_config() -> PluginManagerConfig {
    let mut config = default_test_config();
    config.security.default_sandbox.enabled = true;
    config.security.default_sandbox.sandbox_type = SandboxType::Container;
    config.security.policies.default_level = SecurityLevel::Strict;
    config.security.audit.enabled = true;
    config.security.audit.real_time_monitoring = true;
    config
}

/// Create high-performance configuration
pub fn create_high_performance_config() -> PluginManagerConfig {
    let mut config = default_test_config();
    config.performance.thread_pool_size = 8;
    config.performance.async_runtime.worker_threads = Some(8);
    config.performance.async_runtime.max_blocking_threads = 1024;
    config.performance.caching.enabled = true;
    config.performance.caching.max_size = 1024 * 1024 * 1024; // 1GB
    config.performance.optimization.enabled = true;
    config.performance.optimization.memory_optimization = OptimizationLevel::Aggressive;
    config.performance.optimization.cpu_optimization = OptimizationLevel::Aggressive;
    config.performance.optimization.network_optimization = OptimizationLevel::Aggressive;
    config
}

/// ============================================================================
/// MESSAGE FIXTURES
/// ============================================================================

/// Create a test plugin message
pub fn create_test_message(message_type: PluginMessageType) -> PluginMessage {
    PluginMessage {
        message_id: uuid::Uuid::new_v4().to_string(),
        message_type,
        source_instance_id: Some("test-instance-1".to_string()),
        target_instance_id: Some("test-instance-2".to_string()),
        payload: serde_json::json!({
            "action": "test",
            "data": {
                "value": 42,
                "text": "hello world"
            }
        }),
        timestamp: SystemTime::now(),
        correlation_id: None,
        priority: MessagePriority::Normal,
        timeout: Some(std::time::Duration::from_secs(30)),
    }
}

/// Create a high-priority request message
pub fn create_high_priority_request() -> PluginMessage {
    let mut message = create_test_message(PluginMessageType::Request);
    message.priority = MessagePriority::High;
    message.timeout = Some(std::time::Duration::from_secs(10));
    message
}

/// Create a response message
pub fn create_response_message(correlation_id: &str) -> PluginMessage {
    let mut message = create_test_message(PluginMessageType::Response);
    message.correlation_id = Some(correlation_id.to_string());
    message.payload = serde_json::json!({
        "status": "success",
        "result": {
            "processed": true,
            "count": 5
        }
    });
    message
}

/// ============================================================================
/// HEALTH CHECK FIXTURES
/// ============================================================================

/// Create a successful health check result
pub fn create_healthy_check_result(instance_id: &str) -> HealthCheckResult {
    HealthCheckResult {
        instance_id: instance_id.to_string(),
        status: PluginHealthStatus::Healthy,
        timestamp: SystemTime::now(),
        details: HashMap::from([
            ("check_type".to_string(), "process".to_string()),
            ("duration_ms".to_string(), "15".to_string()),
            ("cpu_usage".to_string(), "12.5".to_string()),
            ("memory_usage".to_string(), "45MB".to_string()),
        ]),
    }
}

/// Create an unhealthy health check result
pub fn create_unhealthy_check_result(instance_id: &str) -> HealthCheckResult {
    HealthCheckResult {
        instance_id: instance_id.to_string(),
        status: PluginHealthStatus::Unhealthy,
        timestamp: SystemTime::now(),
        details: HashMap::from([
            ("check_type".to_string(), "resource".to_string()),
            ("duration_ms".to_string(), "100".to_string()),
            ("error".to_string(), "Resource limits exceeded".to_string()),
            ("memory_usage".to_string(), "512MB".to_string()),
            ("limit".to_string(), "256MB".to_string()),
        ]),
    }
}

/// Create a degraded health check result
pub fn create_degraded_check_result(instance_id: &str) -> HealthCheckResult {
    HealthCheckResult {
        instance_id: instance_id.to_string(),
        status: PluginHealthStatus::Degraded,
        timestamp: SystemTime::now(),
        details: HashMap::from([
            ("check_type".to_string(), "process".to_string()),
            ("duration_ms".to_string(), "50".to_string()),
            ("warning".to_string(), "High response time".to_string()),
            ("avg_response_time".to_string(), "250ms".to_string()),
            ("threshold".to_string(), "200ms".to_string()),
        ]),
    }
}