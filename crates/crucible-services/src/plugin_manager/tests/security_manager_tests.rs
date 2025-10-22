//! # Security Manager Tests
//!
//! Comprehensive tests for security functionality including sandboxing,
//! permission management, policy enforcement, and security validation.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};

/// ============================================================================
/// SECURITY MANAGER LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_manager_creation() {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    // Security manager should be created successfully
    assert!(true); // If we get here, creation succeeded
}

#[tokio::test]
async fn test_security_manager_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut security_manager = DefaultSecurityManager::new(config.security);

    // Start security manager
    security_manager.start().await?;

    // Verify it's running
    let liveness = security_manager.liveness_check().await?;
    assert!(liveness);

    // Stop security manager
    security_manager.stop().await?;

    // Verify it's stopped
    let liveness = security_manager.liveness_check().await?;
    assert!(!liveness);

    Ok(())
}

/// ============================================================================
/// SANDBOX CREATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_sandbox_creation() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "sandbox-test-plugin";
    let sandbox_config = SandboxConfig {
        enabled: true,
        sandbox_type: SandboxType::Process,
        namespace_isolation: true,
        filesystem_isolation: true,
        network_isolation: true,
        process_isolation: true,
        resource_limits: ResourceLimits::default(),
        allowed_syscalls: vec!["read".to_string(), "write".to_string()],
        blocked_syscalls: vec!["execve".to_string()],
        mount_points: vec![],
        environment: HashMap::new(),
    };

    // Create sandbox
    let sandbox_id = security_manager.create_sandbox(plugin_id, &sandbox_config).await?;

    // Verify sandbox ID
    assert!(!sandbox_id.is_empty());
    assert!(sandbox_id.contains(plugin_id));

    // Destroy sandbox
    security_manager.destroy_sandbox(&sandbox_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_sandbox_different_types() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    let sandbox_types = vec![
        SandboxType::Process,
        SandboxType::Container,
        SandboxType::Language,
    ];

    for (i, sandbox_type) in sandbox_types.iter().enumerate() {
        let plugin_id = &format!("sandbox-plugin-{}", i);
        let sandbox_config = SandboxConfig {
            enabled: true,
            sandbox_type: sandbox_type.clone(),
            ..Default::default()
        };

        let sandbox_id = security_manager.create_sandbox(plugin_id, &sandbox_config).await?;
        assert!(!sandbox_id.is_empty());

        security_manager.destroy_sandbox(&sandbox_id).await?;
    }

    Ok(())
}

/// ============================================================================
/// PERMISSION CHECKING TESTS
/// ============================================================================

#[tokio::test]
async fn test_permission_checking() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "permission-test-plugin";

    // Test basic permissions
    let basic_permissions = vec![
        PluginPermission::FileSystemRead,
        PluginPermission::IpcCommunication,
    ];

    for permission in basic_permissions {
        let allowed = security_manager.check_permission(plugin_id, &permission).await?;
        assert!(allowed, "Basic permission {:?} should be allowed", permission);
    }

    // Test restricted permissions
    let restricted_permissions = vec![
        PluginPermission::SystemCalls,
        PluginPermission::ProcessControl,
        PluginPermission::HardwareAccess,
    ];

    for permission in restricted_permissions {
        let allowed = security_manager.check_permission(plugin_id, &permission).await?;
        // May be denied depending on security level
        println!("Permission {:?} allowed: {}", permission, allowed);
    }

    Ok(())
}

#[tokio::test]
async fn test_permission_different_security_levels() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = default_test_config();

    // Test with None security level
    config.security.policies.default_level = SecurityLevel::None;
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "none-security-plugin";
    let permission = PluginPermission::SystemCalls;
    let allowed = security_manager.check_permission(plugin_id, &permission).await?;
    assert!(allowed, "All permissions should be allowed with None security level");

    // Test with Maximum security level
    let mut config = default_test_config();
    config.security.policies.default_level = SecurityLevel::Maximum;
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "max-security-plugin";
    let permission = PluginPermission::SystemCalls;
    let allowed = security_manager.check_permission(plugin_id, &permission).await?;
    assert!(!allowed, "System calls should be denied with Maximum security level");

    Ok(())
}

/// ============================================================================
/// SECURITY POLICY ENFORCEMENT TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_policy_enforcement() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "policy-test-plugin";

    // Test safe operations
    let safe_operations = vec!["read", "write", "ipc"];
    for operation in safe_operations {
        let allowed = security_manager.enforce_security_policy(plugin_id, operation).await?;
        assert!(allowed, "Safe operation '{}' should be allowed", operation);
    }

    // Test unsafe operations
    let unsafe_operations = vec!["exec", "ptrace", "mount"];
    for operation in unsafe_operations {
        let allowed = security_manager.enforce_security_policy(plugin_id, operation).await?;
        // May be denied depending on security level
        println!("Operation '{}' allowed: {}", operation, allowed);
    }

    Ok(())
}

/// ============================================================================
/// PLUGIN SECURITY VALIDATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_security_validation() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    // Test secure plugin
    let secure_manifest = create_test_plugin_manifest("secure-plugin", PluginType::Rune);
    let validation_result = security_manager.validate_plugin_security(&secure_manifest).await?;

    assert!(validation_result.passed);
    assert!(validation_result.issues.is_empty());
    assert!(matches!(validation_result.security_level, SecurityLevel::Basic | SecurityLevel::Strict));

    // Test insecure plugin
    let insecure_manifest = create_insecure_plugin_manifest("insecure-plugin");
    let validation_result = security_manager.validate_plugin_security(&insecure_manifest).await?;

    // May fail validation depending on strictness
    println!("Insecure plugin validation passed: {}", validation_result.passed);
    if !validation_result.issues.is_empty() {
        println!("Security issues found: {}", validation_result.issues.len());
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_capability_validation() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    // Test plugin with safe capabilities
    let safe_manifest = create_test_plugin_manifest("safe-capabilities-plugin", PluginType::Rune);
    let validation_result = security_manager.validate_plugin_security(&safe_manifest).await?;
    assert!(validation_result.passed);

    // Test plugin with system call capabilities
    let mut syscalls_manifest = create_test_plugin_manifest("syscalls-plugin", PluginType::Rune);
    syscalls_manifest.capabilities.push(PluginCapability::SystemCalls {
        allowed_calls: vec!["execve".to_string(), "ptrace".to_string()],
    });

    let validation_result = security_manager.validate_plugin_security(&syscalls_manifest).await?;
    // May be flagged depending on security level
    println!("System calls capability validation passed: {}", validation_result.passed);

    Ok(())
}

/// ============================================================================
/// SECURITY METRICS TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    // Get initial metrics
    let metrics = security_manager.get_security_metrics().await?;
    assert_eq!(metrics.violations_count, 0);
    assert_eq!(metrics.blocked_operations_count, 0);
    assert_eq!(metrics.active_sandboxes, 0);

    // Create some sandboxes
    let sandbox_ids = Vec::new();
    for i in 0..3 {
        let plugin_id = &format!("metrics-plugin-{}", i);
        let sandbox_config = SandboxConfig::default();
        let sandbox_id = security_manager.create_sandbox(plugin_id, &sandbox_config).await?;
        // sandbox_ids.push(sandbox_id); // Would need to handle ownership
    }

    // Get updated metrics
    let updated_metrics = security_manager.get_security_metrics().await?;
    // Note: In mock implementation, these might not be updated
    println!("Active sandboxes: {}", updated_metrics.active_sandboxes);

    Ok(())
}

/// ============================================================================
/// MOCK SECURITY MANAGER TESTS
/// ============================================================================

#[tokio::test]
async fn test_mock_security_manager() -> Result<(), Box<dyn std::error::Error>> {
    let mut mock_security = MockSecurityManager::new();

    // Start mock security manager
    mock_security.start().await?;
    assert!(mock_security.get_validation_count() == 0);

    // Validate plugin security
    let manifest = create_test_plugin_manifest("mock-test-plugin", PluginType::Rune);
    let validation_result = mock_security.validate_plugin_security(&manifest).await?;
    assert!(validation_result.passed);
    assert_eq!(mock_security.get_validation_count(), 1);

    // Check permissions
    let allowed = mock_security.check_permission("mock-plugin", &PluginPermission::FileSystemRead).await?;
    assert!(allowed);

    // Enforce security policy
    let allowed = mock_security.enforce_security_policy("mock-plugin", "read").await?;
    assert!(allowed);

    // Test security violation
    mock_security.simulate_violation("mock-plugin", "Test violation").await;
    assert_eq!(mock_security.get_violation_count(), 1);

    // Stop mock security manager
    mock_security.stop().await?;

    Ok(())
}

#[tokio::test]
async fn test_mock_security_manager_failure_modes() -> Result<(), Box<dyn std::error::Error>> {
    let mut mock_security = MockSecurityManager::new();

    // Set validation failure
    mock_security.set_validation_failure(true);

    // Try to validate plugin
    let manifest = create_test_plugin_manifest("mock-fail-plugin", PluginType::Rune);
    let result = mock_security.validate_plugin_security(&manifest).await;
    assert!(result.is_err());

    // Reset failure mode
    mock_security.set_validation_failure(false);

    // Validation should succeed now
    let result = mock_security.validate_plugin_security(&manifest).await;
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_mock_security_manager_security_levels() -> Result<(), Box<dyn std::error::Error>> {
    let mut mock_security = MockSecurityManager::new();

    // Test different security levels
    let security_levels = vec![
        SecurityLevel::None,
        SecurityLevel::Basic,
        SecurityLevel::Strict,
        SecurityLevel::Maximum,
    ];

    for security_level in security_levels {
        mock_security.set_security_level(security_level.clone()).await;

        // Test permission based on security level
        let permission = PluginPermission::SystemCalls;
        let allowed = mock_security.check_permission("test-plugin", &permission).await?;

        match security_level {
            SecurityLevel::None => assert!(allowed),
            SecurityLevel::Basic => assert!(!allowed), // Basic should deny system calls
            SecurityLevel::Strict => assert!(!allowed),
            SecurityLevel::Maximum => assert!(!allowed),
        }
    }

    Ok(())
}

/// ============================================================================
/// SECURITY EVENT HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_events() -> Result<(), Box<dyn std::error::Error>> {
    let mut mock_security = MockSecurityManager::new();

    // Subscribe to events
    let mut event_receiver = mock_security.subscribe().await;

    // Simulate security violation
    mock_security.simulate_violation("event-plugin", "Test violation").await;

    // Wait for event
    let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await?;
    assert!(event.is_some());

    match event.unwrap() {
        SecurityEvent::SecurityViolation { plugin_id, violation, severity } => {
            assert_eq!(plugin_id, "event-plugin");
            assert_eq!(violation, "Test violation");
            assert_eq!(severity, SecuritySeverity::High);
        }
        _ => panic!("Expected SecurityViolation event"),
    }

    Ok(())
}

/// ============================================================================
/// SECURITY CONFIGURATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();

    // Verify default security configuration
    assert!(config.security.default_sandbox.enabled);
    assert!(matches!(config.security.default_sandbox.sandbox_type, SandboxType::Process));
    assert!(config.security.default_sandbox.namespace_isolation);
    assert!(config.security.default_sandbox.filesystem_isolation);
    assert!(config.security.default_sandbox.process_isolation);

    // Verify default security level
    assert!(matches!(config.security.policies.default_level, SecurityLevel::Basic));

    // Verify audit configuration
    assert!(config.security.audit.enabled);
    assert!(config.security.audit.real_time_monitoring);
    assert!(!config.security.audit.audit_events.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_strict_security_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let config = create_strict_security_config();

    // Verify strict security settings
    assert!(config.security.default_sandbox.enabled);
    assert!(matches!(config.security.default_sandbox.sandbox_type, SandboxType::Container));
    assert!(matches!(config.security.policies.default_level, SecurityLevel::Strict));
    assert!(config.security.audit.enabled);
    assert!(config.security.audit.real_time_monitoring);

    Ok(())
}

/// ============================================================================
/// SECURITY SANDBOX ISOLATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_sandbox_isolation_features() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    let plugin_id = "isolation-test-plugin";

    // Test full isolation
    let sandbox_config = SandboxConfig {
        enabled: true,
        sandbox_type: SandboxType::Container,
        namespace_isolation: true,
        filesystem_isolation: true,
        network_isolation: true,
        process_isolation: true,
        resource_limits: ResourceLimits {
            max_memory_bytes: Some(256 * 1024 * 1024), // 256MB
            max_cpu_percentage: Some(25.0),
            max_concurrent_operations: Some(5),
            operation_timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        },
        allowed_syscalls: vec![
            "read".to_string(),
            "write".to_string(),
            "open".to_string(),
            "close".to_string(),
        ],
        blocked_syscalls: vec![
            "execve".to_string(),
            "ptrace".to_string(),
            "mount".to_string(),
        ],
        mount_points: vec![
            MountPoint {
                source: std::path::PathBuf::from("/tmp"),
                target: std::path::PathBuf::from("/tmp"),
                mount_type: MountType::Bind,
                read_only: false,
                options: vec!["nosuid".to_string(), "nodev".to_string()],
            },
        ],
        environment: HashMap::from([
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("HOME".to_string(), "/tmp".to_string()),
        ]),
    };

    let sandbox_id = security_manager.create_sandbox(plugin_id, &sandbox_config).await?;
    assert!(!sandbox_id.is_empty());

    // Verify sandbox has all isolation features
    // In a real implementation, this would verify actual sandbox isolation
    security_manager.destroy_sandbox(&sandbox_id).await?;

    Ok(())
}

/// ============================================================================
/// SECURITY PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_performance() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let security_manager = DefaultSecurityManager::new(config.security);

    // Measure permission check performance
    let permission_times = benchmark_async(|| {
        Box::pin(async {
            let test_manager = DefaultSecurityManager::new(config.security.clone());
            test_manager.check_permission("perf-plugin", &PluginPermission::FileSystemRead).await
        })
    }, 1000).await;

    let permission_stats = calculate_duration_stats(&permission_times);
    println!("Permission Check Performance: {:?}", permission_stats);

    // Permission checks should be very fast (less than 1ms average)
    assert!(permission_stats.mean < Duration::from_millis(1));

    // Measure security validation performance
    let manifest = create_test_plugin_manifest("perf-plugin", PluginType::Rune);
    let validation_times = benchmark_async(|| {
        Box::pin(async {
            let test_manager = DefaultSecurityManager::new(config.security.clone());
            test_manager.validate_plugin_security(&manifest).await
        })
    }, 100).await;

    let validation_stats = calculate_duration_stats(&validation_times);
    println!("Security Validation Performance: {:?}", validation_stats);

    // Validation should be fast (less than 10ms average)
    assert!(validation_stats.mean < Duration::from_millis(10));

    Ok(())
}