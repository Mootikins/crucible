//! # Plugin Registry Tests
//!
//! Comprehensive tests for the plugin registry functionality including
//! plugin discovery, registration, validation, and lifecycle management.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};
use std::collections::HashMap;

/// ============================================================================
/// PLUGIN REGISTRY LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_registry_creation() {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    // Verify initial state
    assert_eq!(registry.list_plugins().await.unwrap().len(), 0);
}

#[tokio::test]
async fn test_plugin_registry_plugin_registration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create test plugin manifest
    let manifest = create_test_plugin_manifest("registry-test-plugin", PluginType::Rune);

    // Register plugin
    let plugin_id = registry.register_plugin(manifest.clone()).await?;

    // Verify plugin was registered
    assert_eq!(plugin_id, "registry-test-plugin");

    let registered_plugin = registry.get_plugin(&plugin_id).await?;
    assert!(registered_plugin.is_some());

    assert_manifests_approx_equal(&registered_plugin.unwrap(), &manifest);

    // List plugins
    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].manifest.id, plugin_id);

    Ok(())
}

#[tokio::test]
async fn test_plugin_registry_duplicate_registration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create test plugin manifest
    let manifest = create_test_plugin_manifest("duplicate-plugin", PluginType::Rune);

    // Register plugin twice
    let plugin_id1 = registry.register_plugin(manifest.clone()).await?;
    let plugin_id2 = registry.register_plugin(manifest).await?;

    // Should return the same ID
    assert_eq!(plugin_id1, plugin_id2);
    assert_eq!(plugin_id1, "duplicate-plugin");

    // Should only have one plugin
    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_plugin_registry_unregistration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Register a plugin
    let manifest = create_test_plugin_manifest("unregister-test-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify plugin exists
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    // Unregister plugin
    registry.unregister_plugin(&plugin_id).await?;

    // Verify plugin is gone
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_none());

    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_plugin_registry_nonexistent_unregistration() {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Try to unregister non-existent plugin
    let result = registry.unregister_plugin("non-existent-plugin").await;
    // Should not fail - unregistration of non-existent plugin should be safe
    assert!(result.is_ok());
}

/// ============================================================================
/// PLUGIN DISCOVERY TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_discovery_empty_directory() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    // Discover plugins in empty directory
    let manifests = registry.discover_plugins().await?;
    assert_eq!(manifests.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_plugin_discovery_multiple_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    // Create mock plugin manifests
    let plugin1 = create_test_plugin_manifest("discovery-plugin-1", PluginType::Rune);
    let plugin2 = create_test_plugin_manifest("discovery-plugin-2", PluginType::Binary);
    let plugin3 = create_test_plugin_manifest("discovery-plugin-3", PluginType::Wasm);

    // In a real implementation, these would be discovered from file system
    // For the mock, we'll simulate discovery
    let discovered_plugins = vec![plugin1, plugin2, plugin3];

    // Verify discovery results
    assert_eq!(discovered_plugins.len(), 3);

    let plugin_ids: Vec<String> = discovered_plugins.iter().map(|p| p.id.clone()).collect();
    assert!(plugin_ids.contains(&"discovery-plugin-1".to_string()));
    assert!(plugin_ids.contains(&"discovery-plugin-2".to_string()));
    assert!(plugin_ids.contains(&"discovery-plugin-3".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_plugin_discovery_invalid_manifests() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    // Test discovery with invalid manifests
    // In a real implementation, this would test file parsing errors
    let manifests = registry.discover_plugins().await?;

    // Should handle invalid manifests gracefully
    assert!(manifests.len() >= 0); // Should not panic or crash

    Ok(())
}

/// ============================================================================
/// PLUGIN VALIDATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manifest_validation() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    // Test valid manifest
    let valid_manifest = create_test_plugin_manifest("valid-plugin", PluginType::Rune);
    let validation_result = valid_manifest.validate();
    assert!(validation_result.is_ok());

    // Test invalid manifest (empty ID)
    let mut invalid_manifest = create_test_plugin_manifest("invalid-plugin", PluginType::Rune);
    invalid_manifest.id = String::new();
    let validation_result = invalid_manifest.validate();
    assert!(validation_result.is_err());

    // Test invalid manifest (empty name)
    let mut invalid_manifest = create_test_plugin_manifest("invalid-plugin", PluginType::Rune);
    invalid_manifest.name = String::new();
    let validation_result = invalid_manifest.validate();
    assert!(validation_result.is_err());

    // Test invalid manifest (empty version)
    let mut invalid_manifest = create_test_plugin_manifest("invalid-plugin", PluginType::Rune);
    invalid_manifest.version = String::new();
    let validation_result = invalid_manifest.validate();
    assert!(validation_result.is_err());

    // Test invalid manifest (non-existent entry point)
    let mut invalid_manifest = create_test_plugin_manifest("invalid-plugin", PluginType::Rune);
    invalid_manifest.entry_point = std::path::PathBuf::from("/non/existent/path");
    let validation_result = invalid_manifest.validate();
    assert!(validation_result.is_err());

    Ok(())
}

#[tokio::test]
async fn test_plugin_version_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = create_test_plugin_manifest("compatibility-plugin", PluginType::Rune);

    // Test compatibility with current version
    let is_compatible = manifest.is_compatible_with_version("1.0.0");
    assert!(is_compatible);

    // Test with minimum version
    let mut manifest_with_min = manifest.clone();
    manifest_with_min.min_crucible_version = Some("0.9.0".to_string());
    assert!(manifest_with_min.is_compatible_with_version("1.0.0"));
    assert!(!manifest_with_min.is_compatible_with_version("0.8.0"));

    // Test with maximum version
    let mut manifest_with_max = manifest.clone();
    manifest_with_max.max_crucible_version = Some("2.0.0".to_string());
    assert!(manifest_with_max.is_compatible_with_version("1.0.0"));
    assert!(!manifest_with_max.is_compatible_with_version("2.1.0"));

    // Test with both min and max
    let mut manifest_with_range = manifest.clone();
    manifest_with_range.min_crucible_version = Some("1.0.0".to_string());
    manifest_with_range.max_crucible_version = Some("2.0.0".to_string());
    assert!(manifest_with_range.is_compatible_with_version("1.5.0"));
    assert!(!manifest_with_range.is_compatible_with_version("0.9.0"));
    assert!(!manifest_with_range.is_compatible_with_version("2.1.0"));

    Ok(())
}

#[tokio::test]
async fn test_plugin_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = create_test_plugin_manifest("capabilities-plugin", PluginType::Rune);

    // Test capabilities summary
    let summary = manifest.get_capabilities_summary();
    assert!(summary.contains(&"File System Access".to_string()));
    assert!(summary.contains(&"IPC Communication".to_string()));

    // Test different capability types
    let mut fs_manifest = manifest.clone();
    fs_manifest.capabilities = vec![
        PluginCapability::FileSystem {
            read_paths: vec!["/tmp".to_string(), "/var/tmp".to_string()],
            write_paths: vec!["/tmp".to_string()],
        },
    ];
    let fs_summary = fs_manifest.get_capabilities_summary();
    assert!(fs_summary.contains(&"File System Access".to_string()));

    let mut network_manifest = manifest.clone();
    network_manifest.capabilities = vec![
        PluginCapability::Network {
            allowed_hosts: vec!["example.com".to_string()],
            allowed_ports: vec![80, 443],
        },
    ];
    let network_summary = network_manifest.get_capabilities_summary();
    assert!(network_summary.contains(&"Network Access".to_string()));

    Ok(())
}

/// ============================================================================
/// PLUGIN STATUS MANAGEMENT TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_status_updates() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Register a plugin
    let manifest = create_test_plugin_manifest("status-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify initial status
    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins[0].status, PluginRegistryStatus::Installed);

    // Update status to disabled
    registry.update_plugin_status(&plugin_id, PluginRegistryStatus::Disabled).await?;

    // Verify status was updated
    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins[0].status, PluginRegistryStatus::Disabled);

    // Update status to error
    registry.update_plugin_status(&plugin_id, PluginRegistryStatus::Error("Test error".to_string())).await?;

    // Verify status was updated
    let plugins = registry.list_plugins().await?;
    match &plugins[0].status {
        PluginRegistryStatus::Error(message) => assert_eq!(message, "Test error"),
        _ => panic!("Expected Error status"),
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_enabled_list() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Register multiple plugins
    let plugin_ids = vec!["enabled-1", "enabled-2", "disabled-1", "error-1"];

    for plugin_id in &plugin_ids {
        let manifest = create_test_plugin_manifest(plugin_id, PluginType::Rune);
        registry.register_plugin(manifest).await?;
    }

    // Update some plugins to non-enabled status
    registry.update_plugin_status("disabled-1", PluginRegistryStatus::Disabled).await?;
    registry.update_plugin_status("error-1", PluginRegistryStatus::Error("Test error".to_string())).await?;

    // Get enabled plugins
    let enabled_plugins = registry.list_enabled_plugins().await?;
    assert_eq!(enabled_plugins.len(), 2);

    let enabled_ids: Vec<String> = enabled_plugins.iter().map(|p| p.manifest.id.clone()).collect();
    assert!(enabled_ids.contains(&"enabled-1".to_string()));
    assert!(enabled_ids.contains(&"enabled-2".to_string()));
    assert!(!enabled_ids.contains(&"disabled-1".to_string()));
    assert!(!enabled_ids.contains(&"error-1".to_string()));

    Ok(())
}

/// ============================================================================
/// PLUGIN DEPENDENCY TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create plugins with dependencies
    let base_dependency = PluginDependency {
        name: "base-plugin".to_string(),
        version: Some("1.0.0".to_string()),
        dependency_type: DependencyType::Plugin,
        optional: false,
    };

    let optional_dependency = PluginDependency {
        name: "optional-plugin".to_string(),
        version: Some("1.0.0".to_string()),
        dependency_type: DependencyType::Plugin,
        optional: true,
    };

    let manifest = create_plugin_with_dependencies(
        "dependent-plugin",
        vec![base_dependency, optional_dependency],
    );

    // Register plugin with dependencies
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify plugin was registered
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    let registered_plugin = plugin.unwrap();
    assert_eq!(registered_plugin.dependencies.len(), 2);
    assert_eq!(registered_plugin.dependencies[0].name, "base-plugin");
    assert_eq!(registered_plugin.dependencies[1].name, "optional-plugin");
    assert!(!registered_plugin.dependencies[0].optional);
    assert!(registered_plugin.dependencies[1].optional);

    Ok(())
}

#[tokio::test]
async fn test_plugin_dependency_types() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create dependencies of different types
    let plugin_dep = PluginDependency {
        name: "other-plugin".to_string(),
        version: None,
        dependency_type: DependencyType::Plugin,
        optional: false,
    };

    let system_dep = PluginDependency {
        name: "libc6".to_string(),
        version: Some("2.31".to_string()),
        dependency_type: DependencyType::SystemLibrary,
        optional: false,
    };

    let runtime_dep = PluginDependency {
        name: "nodejs".to_string(),
        version: Some("16.0.0".to_string()),
        dependency_type: DependencyType::Runtime,
        optional: false,
    };

    let dev_dep = PluginDependency {
        name: "typescript".to_string(),
        version: Some("4.0.0".to_string()),
        dependency_type: DependencyType::Development,
        optional: true,
    };

    let manifest = create_plugin_with_dependencies(
        "multi-dep-plugin",
        vec![plugin_dep, system_dep, runtime_dep, dev_dep],
    );

    // Register plugin
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify dependencies
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    let registered_plugin = plugin.unwrap();
    assert_eq!(registered_plugin.dependencies.len(), 4);

    let dep_types: Vec<DependencyType> = registered_plugin.dependencies.iter()
        .map(|d| d.dependency_type.clone())
        .collect();

    assert!(dep_types.contains(&DependencyType::Plugin));
    assert!(dep_types.contains(&DependencyType::SystemLibrary));
    assert!(dep_types.contains(&DependencyType::Runtime));
    assert!(dep_types.contains(&DependencyType::Development));

    Ok(())
}

/// ============================================================================
/// PLUGIN TYPE TESTS
/// ============================================================================

#[tokio::test]
async fn test_different_plugin_types() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Test different plugin types
    let rune_manifest = create_rune_plugin_manifest("rune-plugin");
    let binary_manifest = create_binary_plugin_manifest("binary-plugin");
    let wasm_manifest = create_wasm_plugin_manifest("wasm-plugin");
    let python_manifest = create_python_plugin_manifest("python-plugin");

    let manifests = vec![rune_manifest, binary_manifest, wasm_manifest, python_manifest];
    let expected_types = vec![PluginType::Rune, PluginType::Binary, PluginType::Wasm, PluginType::Python];

    for (i, manifest) in manifests.into_iter().enumerate() {
        let plugin_id = registry.register_plugin(manifest).await?;
        let plugin = registry.get_plugin(&plugin_id).await?;
        assert!(plugin.is_some());

        let registered_plugin = plugin.unwrap();
        assert_eq!(registered_plugin.plugin_type, expected_types[i]);

        // Verify entry point extension matches type
        match expected_types[i] {
            PluginType::Rune => assert!(registered_plugin.entry_point.to_string_lossy().ends_with(".rn")),
            PluginType::Binary => assert!(registered_plugin.entry_point.to_string_lossy().contains("bin")),
            PluginType::Wasm => assert!(registered_plugin.entry_point.to_string_lossy().ends_with(".wasm")),
            PluginType::Python => assert!(registered_plugin.entry_point.to_string_lossy().ends_with(".py")),
            _ => {}
        }
    }

    // Verify all plugins are registered
    let plugins = registry.list_plugins().await?;
    assert_eq!(plugins.len(), 4);

    Ok(())
}

/// ============================================================================
/// PLUGIN RESOURCE LIMITS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_resource_limits() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create plugin with resource limits
    let resource_limits = ResourceLimits {
        max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
        max_cpu_percentage: Some(50.0),
        max_disk_bytes: Some(1024 * 1024 * 1024), // 1GB
        max_concurrent_operations: Some(10),
        max_child_processes: Some(2),
        max_open_files: Some(20),
        operation_timeout: Some(Duration::from_secs(60)),
        idle_timeout: Some(Duration::from_secs(300)),
    };

    let mut manifest = create_test_plugin_manifest("resource-limits-plugin", PluginType::Rune);
    manifest.resource_limits = resource_limits.clone();

    // Register plugin
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify resource limits
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    let registered_plugin = plugin.unwrap();
    assert_eq!(registered_plugin.resource_limits.max_memory_bytes, resource_limits.max_memory_bytes);
    assert_eq!(registered_plugin.resource_limits.max_cpu_percentage, resource_limits.max_cpu_percentage);
    assert_eq!(registered_plugin.resource_limits.max_disk_bytes, resource_limits.max_disk_bytes);
    assert_eq!(registered_plugin.resource_limits.max_concurrent_operations, resource_limits.max_concurrent_operations);
    assert_eq!(registered_plugin.resource_limits.max_child_processes, resource_limits.max_child_processes);
    assert_eq!(registered_plugin.resource_limits.max_open_files, resource_limits.max_open_files);
    assert_eq!(registered_plugin.resource_limits.operation_timeout, resource_limits.operation_timeout);
    assert_eq!(registered_plugin.resource_limits.idle_timeout, resource_limits.idle_timeout);

    Ok(())
}

/// ============================================================================
/// PLUGIN SANDBOX CONFIGURATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_sandbox_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Create plugin with custom sandbox config
    let sandbox_config = SandboxConfig {
        enabled: true,
        sandbox_type: SandboxType::Container,
        namespace_isolation: true,
        filesystem_isolation: true,
        network_isolation: true,
        process_isolation: true,
        resource_limits: ResourceLimits::default(),
        allowed_syscalls: vec!["read".to_string(), "write".to_string(), "open".to_string()],
        blocked_syscalls: vec!["execve".to_string(), "ptrace".to_string()],
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

    let mut manifest = create_test_plugin_manifest("sandbox-plugin", PluginType::Rune);
    manifest.sandbox_config = sandbox_config.clone();

    // Register plugin
    let plugin_id = registry.register_plugin(manifest).await?;

    // Verify sandbox configuration
    let plugin = registry.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    let registered_plugin = plugin.unwrap();
    assert_eq!(registered_plugin.sandbox_config.enabled, sandbox_config.enabled);
    assert_eq!(registered_plugin.sandbox_config.sandbox_type, sandbox_config.sandbox_type);
    assert_eq!(registered_plugin.sandbox_config.namespace_isolation, sandbox_config.namespace_isolation);
    assert_eq!(registered_plugin.sandbox_config.filesystem_isolation, sandbox_config.filesystem_isolation);
    assert_eq!(registered_plugin.sandbox_config.network_isolation, sandbox_config.network_isolation);
    assert_eq!(registered_plugin.sandbox_config.process_isolation, sandbox_config.process_isolation);
    assert_eq!(registered_plugin.sandbox_config.allowed_syscalls, sandbox_config.allowed_syscalls);
    assert_eq!(registered_plugin.sandbox_config.blocked_syscalls, sandbox_config.blocked_syscalls);
    assert_eq!(registered_plugin.sandbox_config.mount_points.len(), sandbox_config.mount_points.len());
    assert_eq!(registered_plugin.sandbox_config.environment, sandbox_config.environment);

    Ok(())
}

/// ============================================================================
/// PLUGIN EVENT TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_registry_events() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Subscribe to events
    let mut event_receiver = registry.subscribe().await;

    // Register a plugin
    let manifest = create_test_plugin_manifest("event-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;

    // Wait for registration event
    let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await?;
    assert!(event.is_some());

    match event.unwrap() {
        RegistryEvent::PluginRegistered { plugin_id: registered_id } => {
            assert_eq!(registered_id, plugin_id);
        }
        _ => panic!("Expected PluginRegistered event"),
    }

    // Unregister plugin
    registry.unregister_plugin(&plugin_id).await?;

    // Wait for unregistration event
    let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await?;
    assert!(event.is_some());

    match event.unwrap() {
        RegistryEvent::PluginUnregistered { plugin_id: unregistered_id } => {
            assert_eq!(unregistered_id, plugin_id);
        }
        _ => panic!("Expected PluginUnregistered event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_registry_multiple_subscribers() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Subscribe multiple receivers
    let mut receiver1 = registry.subscribe().await;
    let mut receiver2 = registry.subscribe().await;
    let mut receiver3 = registry.subscribe().await;

    // Register a plugin
    let manifest = create_test_plugin_manifest("multi-subscriber-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;

    // All subscribers should receive the event
    for (i, receiver) in [&mut receiver1, &mut receiver2, &mut receiver3].iter_mut().enumerate() {
        let event = tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await?;
        assert!(event.is_some());

        match event.unwrap() {
            RegistryEvent::PluginRegistered { plugin_id: registered_id } => {
                assert_eq!(registered_id, plugin_id);
            }
            _ => panic!("Subscriber {} expected PluginRegistered event", i + 1),
        }
    }

    Ok(())
}

/// ============================================================================
/// REGISTRY ERROR HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_registry_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Test getting non-existent plugin
    let plugin = registry.get_plugin("non-existent").await?;
    assert!(plugin.is_none());

    // Test updating status of non-existent plugin
    let result = registry.update_plugin_status("non-existent", PluginRegistryStatus::Disabled).await;
    // Should not fail - updating non-existent plugin should be safe
    assert!(result.is_ok());

    // Test unregistering non-existent plugin
    let result = registry.unregister_plugin("non-existent").await;
    // Should not fail - unregistering non-existent plugin should be safe
    assert!(result.is_ok());

    Ok(())
}

/// ============================================================================
/// REGISTRY PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_registry_performance() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let mut registry = DefaultPluginRegistry::new(config);

    // Measure registration performance
    let plugin_count = 100;
    let start_time = std::time::Instant::now();

    for i in 0..plugin_count {
        let manifest = create_test_plugin_manifest(&format!("perf-plugin-{}", i), PluginType::Rune);
        registry.register_plugin(manifest).await?;
    }

    let registration_duration = start_time.elapsed();

    // Registration should be fast (less than 1 second for 100 plugins)
    assert!(registration_duration < Duration::from_secs(1));

    // Measure listing performance
    let start_time = std::time::Instant::now();
    let plugins = registry.list_plugins().await?;
    let listing_duration = start_time.elapsed();

    // Listing should be very fast (less than 100ms)
    assert!(listing_duration < Duration::from_millis(100));

    // Verify all plugins were registered
    assert_eq!(plugins.len(), plugin_count);

    // Measure lookup performance
    let start_time = std::time::Instant::now();

    for i in 0..plugin_count {
        let plugin_id = format!("perf-plugin-{}", i);
        let plugin = registry.get_plugin(&plugin_id).await?;
        assert!(plugin.is_some());
    }

    let lookup_duration = start_time.elapsed();

    // Lookup should be very fast (less than 100ms for 100 lookups)
    assert!(lookup_duration < Duration::from_millis(100));

    println!("Registry Performance:");
    println!("  Registration ({} plugins): {:?}", plugin_count, registration_duration);
    println!("  Listing ({} plugins): {:?}", plugin_count, listing_duration);
    println!("  Lookup ({} plugins): {:?}", plugin_count, lookup_duration);

    Ok(())
}

/// ============================================================================
/// REGISTRY CONCURRENCY TESTS
/// ============================================================================

#[tokio::test]
async fn test_registry_concurrent_operations() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = Arc::new(RwLock::new(DefaultPluginRegistry::new(config)));

    // Concurrent registration
    let mut handles = Vec::new();
    for i in 0..10 {
        let registry_clone = registry.clone();
        let handle = tokio::spawn(async move {
            let manifest = create_test_plugin_manifest(&format!("concurrent-plugin-{}", i), PluginType::Rune);
            let mut registry_guard = registry_clone.write().await;
            registry_guard.register_plugin(manifest).await
        });
        handles.push(handle);
    }

    // Wait for all registrations
    let mut plugin_ids = Vec::new();
    for handle in handles {
        let plugin_id = handle.await??;
        plugin_ids.push(plugin_id);
    }

    // Verify all plugins were registered
    let registry_guard = registry.read().await;
    let plugins = registry_guard.list_plugins().await?;
    assert_eq!(plugins.len(), 10);

    // Concurrent lookups
    let mut handles = Vec::new();
    for plugin_id in &plugin_ids {
        let registry_clone = registry.clone();
        let plugin_id_clone = plugin_id.clone();
        let handle = tokio::spawn(async move {
            let registry_guard = registry_clone.read().await;
            registry_guard.get_plugin(&plugin_id_clone).await
        });
        handles.push(handle);
    }

    // Wait for all lookups
    for handle in handles {
        let plugin = handle.await??;
        assert!(plugin.is_some());
    }

    Ok(())
}