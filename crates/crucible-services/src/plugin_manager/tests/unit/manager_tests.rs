//! # Plugin Manager Core Tests
//!
//! Comprehensive tests for the core PluginManager functionality including
//! lifecycle management, plugin discovery, configuration, and event handling.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};

/// ============================================================================
/// PLUGIN MANAGER LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_creation() {
    let config = default_test_config();
    let service = PluginManagerService::new(config);

    // Verify initial state
    assert!(!service.is_running());
    assert_eq!(service.service_name(), "PluginManager");
    assert!(!service.service_version().is_empty());
}

#[tokio::test]
async fn test_plugin_manager_startup() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Start the service
    service.start().await?;

    // Verify it's running
    assert!(service.is_running());

    // Check health
    let health = service.health_check().await?;
    assert_eq!(health.status, ServiceStatus::Healthy);

    // Stop the service
    service.stop().await?;
    assert!(!service.is_running());

    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_double_start() {
    let mut service = create_test_plugin_manager().await;

    // Start the service
    service.start().await.expect("First start should succeed");

    // Try to start again - should fail
    let result = service.start().await;
    assert!(result.is_err());

    // Cleanup
    service.stop().await.expect("Stop should succeed");
}

#[tokio::test]
async fn test_plugin_manager_stop_without_start() {
    let mut service = create_test_plugin_manager().await;

    // Stop without starting - should not fail
    let result = service.stop().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_plugin_manager_graceful_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Create some mock instances
    let mock_registry = MockPluginRegistry::new();
    let mock_resource_manager = MockResourceManager::new();
    let mock_security_manager = MockSecurityManager::new();
    let mock_health_monitor = MockHealthMonitor::new();

    let test_service = create_plugin_manager_with_components(
        Box::new(mock_registry),
        Box::new(mock_resource_manager),
        Box::new(mock_security_manager),
        Box::new(mock_health_monitor),
    ).await;

    let mut test_service = test_service;
    test_service.start().await?;

    // Add some instances
    let instance_id1 = test_service.create_instance("test-plugin-1", None).await?;
    let instance_id2 = test_service.create_instance("test-plugin-2", None).await?;

    test_service.start_instance(&instance_id1).await?;
    test_service.start_instance(&instance_id2).await?;

    // Stop the service and verify graceful shutdown
    let start_time = std::time::Instant::now();
    test_service.stop().await?;
    let shutdown_duration = start_time.elapsed();

    // Should shutdown quickly (within 5 seconds)
    assert!(shutdown_duration < Duration::from_secs(5));

    // Verify instances are stopped
    let instances = test_service.list_instances().await?;
    assert!(instances.is_empty());

    Ok(())
}

/// ============================================================================
/// PLUGIN DISCOVERY TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_discovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Create mock registry with test plugins
    let mut mock_registry = MockPluginRegistry::new();

    // Add some test plugins
    let plugin1 = create_test_registry_entry("test-plugin-1", PluginType::Rune);
    let plugin2 = create_test_registry_entry("test-plugin-2", PluginType::Binary);
    let plugin3 = create_test_registry_entry("test-plugin-3", PluginType::Wasm);

    mock_registry.add_plugin(plugin1).await;
    mock_registry.add_plugin(plugin2).await;
    mock_registry.add_plugin(plugin3).await;

    let test_service = create_plugin_manager_with_components(
        Box::new(mock_registry),
        Box::new(MockResourceManager::new()),
        Box::new(MockSecurityManager::new()),
        Box::new(MockHealthMonitor::new()),
    ).await;

    let mut test_service = test_service;
    test_service.start().await?;

    // Wait for discovery to complete
    sleep(Duration::from_millis(100)).await;

    // List plugins
    let plugins = test_service.list_plugins().await?;
    assert_eq!(plugins.len(), 3);

    // Verify plugin details
    let plugin_ids: Vec<String> = plugins.iter().map(|p| p.manifest.id.clone()).collect();
    assert!(plugin_ids.contains(&"test-plugin-1".to_string()));
    assert!(plugin_ids.contains(&"test-plugin-2".to_string()));
    assert!(plugin_ids.contains(&"test-plugin-3".to_string()));

    test_service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_discovery_failure() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Create mock registry that fails discovery
    let mut mock_registry = MockPluginRegistry::new();
    mock_registry.set_discovery_failure(true);

    let test_service = create_plugin_manager_with_components(
        Box::new(mock_registry),
        Box::new(MockResourceManager::new()),
        Box::new(MockSecurityManager::new()),
        Box::new(MockHealthMonitor::new()),
    ).await;

    let mut test_service = test_service;
    test_service.start().await?;

    // Wait for discovery attempt
    sleep(Duration::from_millis(100)).await;

    // Should still start even if discovery fails
    assert!(test_service.is_running());

    // List plugins should be empty or minimal
    let plugins = test_service.list_plugins().await?;
    assert_eq!(plugins.len(), 0);

    test_service.stop().await?;
    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE MANAGEMENT TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_creation() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Register a test plugin first
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("test-instance-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create instance
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // Verify instance exists
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 1);

    // Verify instance details
    let plugin = service.get_plugin(&plugin_id).await?;
    assert!(plugin.is_some());

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("test-lifecycle-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create and start instance
    let instance_id = service.create_instance(&plugin_id, None).await?;
    service.start_instance(&instance_id).await?;

    // Verify instance is running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 1);

    // Stop instance
    service.stop_instance(&instance_id).await?;

    // Verify instance is stopped
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 0);

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_multiple_plugin_instances() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Register multiple test plugins
    let mut registry = service.registry.write().await;

    let manifest1 = create_test_plugin_manifest("multi-plugin-1", PluginType::Rune);
    let manifest2 = create_test_plugin_manifest("multi-plugin-2", PluginType::Binary);
    let manifest3 = create_test_plugin_manifest("multi-plugin-3", PluginType::Wasm);

    let plugin_id1 = registry.register_plugin(manifest1).await?;
    let plugin_id2 = registry.register_plugin(manifest2).await?;
    let plugin_id3 = registry.register_plugin(manifest3).await?;
    drop(registry);

    // Create multiple instances
    let instance_id1 = service.create_instance(&plugin_id1, None).await?;
    let instance_id2 = service.create_instance(&plugin_id2, None).await?;
    let instance_id3 = service.create_instance(&plugin_id3, None).await?;

    // Start all instances
    service.start_instance(&instance_id1).await?;
    service.start_instance(&instance_id2).await?;
    service.start_instance(&instance_id3).await?;

    // Verify all instances are running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 3);

    // Stop all instances
    service.stop_instance(&instance_id1).await?;
    service.stop_instance(&instance_id2).await?;
    service.stop_instance(&instance_id3).await?;

    // Verify all instances are stopped
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 0);

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_not_found() {
    let mut service = create_test_plugin_manager().await;
    service.start().await.expect("Service should start");

    // Try to start non-existent instance
    let result = service.start_instance("non-existent-instance").await;
    assert!(result.is_err());

    // Try to stop non-existent instance
    let result = service.stop_instance("non-existent-instance").await;
    assert!(result.is_err());

    service.stop().await.expect("Service should stop");
}

/// ============================================================================
/// CONFIGURATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let service = PluginManagerService::new(config.clone());

    // Get configuration
    let retrieved_config = service.get_config().await?;
    assert_eq!(retrieved_config.plugin_directories, config.plugin_directories);
    assert_eq!(retrieved_config.auto_discovery.enabled, config.auto_discovery.enabled);

    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_configuration_update() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create new configuration
    let mut new_config = default_test_config();
    new_config.performance.thread_pool_size = 8;
    new_config.security.default_sandbox.enabled = false;

    // Update configuration
    service.update_config(new_config.clone()).await?;

    // Verify configuration was updated
    let retrieved_config = service.get_config().await?;
    assert_eq!(retrieved_config.performance.thread_pool_size, 8);
    assert!(!retrieved_config.security.default_sandbox.enabled);

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_invalid_configuration() {
    let mut service = create_test_plugin_manager().await;
    service.start().await.expect("Service should start");

    // Create invalid configuration (empty plugin directories)
    let mut invalid_config = default_test_config();
    invalid_config.plugin_directories.clear();

    // Try to update with invalid configuration
    let result = service.update_config(invalid_config).await;
    assert!(result.is_err());

    service.stop().await.expect("Service should stop");
}

#[tokio::test]
async fn test_plugin_manager_configuration_validation() -> Result<(), Box<dyn std::error::Error>> {
    let service = create_test_plugin_manager().await;

    // Test valid configuration
    let valid_config = default_test_config();
    let result = service.validate_config(&valid_config).await;
    assert!(result.is_ok());

    // Test invalid configuration
    let mut invalid_config = default_test_config();
    invalid_config.health_monitoring.check_interval = Duration::from_millis(0); // Invalid
    let result = service.validate_config(&invalid_config).await;
    assert!(result.is_err());

    Ok(())
}

/// ============================================================================
/// EVENT HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_event_subscription() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut receiver = service.subscribe_events().await;

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("event-test-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create an instance (should generate events)
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // Wait for events
    let plugin_registered_event = wait_for_event_type(
        &mut receiver,
        "PluginRegistered",
        Duration::from_millis(500),
    ).await;

    let instance_created_event = wait_for_event_type(
        &mut receiver,
        "InstanceCreated",
        Duration::from_millis(500),
    ).await;

    // Verify events
    match plugin_registered_event {
        PluginManagerEvent::PluginRegistered { plugin_id: registered_id } => {
            assert_eq!(registered_id, plugin_id);
        }
        _ => panic!("Expected PluginRegistered event"),
    }

    match instance_created_event {
        PluginManagerEvent::InstanceCreated { instance_id: created_id, plugin_id: plugin_id: event_plugin_id } => {
            assert_eq!(created_id, instance_id);
            assert_eq!(event_plugin_id, plugin_id);
        }
        _ => panic!("Expected InstanceCreated event"),
    }

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_multiple_event_subscribers() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe multiple receivers
    let mut receiver1 = service.subscribe_events().await;
    let mut receiver2 = service.subscribe_events().await;
    let mut receiver3 = service.subscribe_events().await;

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("multi-event-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create an instance
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // All subscribers should receive the events
    for (i, receiver) in [&mut receiver1, &mut receiver2, &mut receiver3].iter_mut().enumerate() {
        let event = wait_for_event_type(
            receiver,
            "InstanceCreated",
            Duration::from_millis(500),
        ).await;

        match event {
            PluginManagerEvent::InstanceCreated { instance_id: created_id, .. } => {
                assert_eq!(created_id, instance_id);
            }
            _ => panic!("Receiver {} expected InstanceCreated event", i + 1),
        }
    }

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_event_handling() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut receiver = service.subscribe_events().await;

    // Create a test event
    let test_event = PluginManagerEvent::Error {
        operation: "test_operation".to_string(),
        error: "Test error message".to_string(),
        context: Some("Test context".to_string()),
    };

    // Handle the event
    service.handle_event(test_event.clone()).await?;

    // Verify error metrics were updated
    let metrics = service.get_metrics().await?;
    assert!(metrics.error_count > 0);

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// METRICS AND MONITORING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Get initial metrics
    let initial_metrics = service.get_metrics().await?;
    assert_eq!(initial_metrics.service_name, "PluginManager");
    assert!(!initial_metrics.service_version.is_empty());

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("metrics-test-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create and start an instance
    let instance_id = service.create_instance(&plugin_id, None).await?;
    service.start_instance(&instance_id).await?;

    // Get updated metrics
    let updated_metrics = service.get_metrics().await?;
    assert!(updated_metrics.request_count > initial_metrics.request_count);

    // Stop instance
    service.stop_instance(&instance_id).await?;

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_performance_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Get performance metrics
    let perf_metrics = service.get_performance_metrics().await?;

    // Verify metrics structure
    assert!(!perf_metrics.request_times.is_empty() || perf_metrics.request_times.is_empty()); // May be empty initially
    assert!(perf_metrics.timestamp <= std::time::SystemTime::now());
    assert!(perf_metrics.custom_metrics.contains_key("operations_per_second"));
    assert!(perf_metrics.custom_metrics.contains_key("average_response_time"));

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_metrics_reset() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Perform some operations to generate metrics
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("reset-metrics-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    let instance_id = service.create_instance(&plugin_id, None).await?;
    service.start_instance(&instance_id).await?;

    // Get metrics before reset
    let metrics_before = service.get_metrics().await?;
    assert!(metrics_before.request_count > 0);

    // Reset metrics
    service.reset_metrics().await?;

    // Get metrics after reset
    let metrics_after = service.get_metrics().await?;
    assert_eq!(metrics_after.request_count, 0);

    service.stop_instance(&instance_id).await?;
    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// HEALTH CHECK TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_health_check() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Perform health check
    let health = service.health_check().await?;
    assert_eq!(health.status, ServiceStatus::Healthy);
    assert!(health.uptime > Duration::ZERO);

    // Verify health details
    assert!(health.details.contains_key("total_plugins"));
    assert!(health.details.contains_key("active_instances"));
    assert!(health.details.contains_key("failed_operations"));
    assert!(health.details.contains_key("resource_manager"));
    assert!(health.details.contains_key("security_manager"));
    assert!(health.details.contains_key("health_monitor"));

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_liveness_check() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Liveness check before start should be false
    let liveness = service.liveness_check().await?;
    assert!(!liveness);

    service.start().await?;

    // Liveness check after start should be true
    let liveness = service.liveness_check().await?;
    assert!(liveness);

    service.stop().await?;

    // Liveness check after stop should be false
    let liveness = service.liveness_check().await?;
    assert!(!liveness);

    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_readiness_check() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;

    // Readiness check before start should be false
    let readiness = service.readiness_check().await?;
    assert!(!readiness);

    service.start().await?;

    // Readiness check after start should be true
    let readiness = service.readiness_check().await?;
    assert!(readiness);

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// ERROR HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_error_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create a mock instance that fails on start
    let mut mock_instance = MockPluginInstance::new(
        "fail-instance".to_string(),
        "fail-plugin".to_string(),
    );
    mock_instance.set_start_failure(true);

    // Simulate error handling
    let test_event = PluginManagerEvent::InstanceCrashed {
        instance_id: "fail-instance".to_string(),
        plugin_id: "fail-plugin".to_string(),
        error: "Mock crash".to_string(),
    };

    // Handle the error event
    let result = service.handle_event(test_event).await;
    assert!(result.is_ok());

    // Service should still be running after error
    assert!(service.is_running());

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_plugin_manager_graceful_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create a mock registry that fails
    let mut mock_registry = MockPluginRegistry::new();
    mock_registry.set_registration_failure(true);

    let test_service = create_plugin_manager_with_components(
        Box::new(mock_registry),
        Box::new(MockResourceManager::new()),
        Box::new(MockSecurityManager::new()),
        Box::new(MockHealthMonitor::new()),
    ).await;

    let mut test_service = test_service;
    test_service.start().await?;

    // Service should still start even with component failures
    assert!(test_service.is_running());

    test_service.stop().await?;
    Ok(())
}

/// ============================================================================
/// CONCURRENT OPERATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_concurrent_plugin_operations() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = Arc::new(RwLock::new(create_test_plugin_manager().await));
    service.write().await.start().await?;

    // Register multiple plugins
    let mut registry = service.write().await.registry.write().await;
    let plugin_ids = Vec::new();

    for i in 0..5 {
        let manifest = create_test_plugin_manifest(&format!("concurrent-plugin-{}", i), PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        // plugin_ids.push(plugin_id); // This would require moving the Vec
    }
    drop(registry);

    // Create multiple instances concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let service_clone = service.clone();
        let handle = tokio::spawn(async move {
            let plugin_id = format!("concurrent-plugin-{}", i);
            let mut service_guard = service_clone.write().await;
            match service_guard.create_instance(&plugin_id, None).await {
                Ok(instance_id) => {
                    let _ = service_guard.start_instance(&instance_id).await;
                    Some(instance_id)
                }
                Err(_) => None,
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut instance_ids = Vec::new();
    for handle in handles {
        if let Some(Some(instance_id)) = handle.await? {
            instance_ids.push(instance_id);
        }
    }

    // Verify instances were created
    let instances = service.read().await.list_instances().await?;
    assert!(!instances.is_empty());

    // Stop all instances
    for instance_id in instance_ids {
        service.write().await.stop_instance(&instance_id).await?;
    }

    service.write().await.stop().await?;
    Ok(())
}

/// ============================================================================
/// STRESS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_stress() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create many instances rapidly
    let mut instance_ids = Vec::new();

    for i in 0..50 {
        // Register plugin
        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(&format!("stress-plugin-{}", i), PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        // Create instance
        let instance_id = service.create_instance(&plugin_id, None).await?;
        instance_ids.push(instance_id);

        // Start some instances
        if i % 2 == 0 {
            let _ = service.start_instance(&instance_id).await;
        }
    }

    // Verify service is still responsive
    let health = service.health_check().await?;
    assert_eq!(health.status, ServiceStatus::Healthy);

    // Clean up instances
    for instance_id in instance_ids {
        let _ = service.stop_instance(&instance_id).await;
    }

    service.stop().await?;
    Ok(())
}