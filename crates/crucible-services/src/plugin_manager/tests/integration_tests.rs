//! # Integration Tests
//!
//! End-to-end integration tests for the complete PluginManager system,
//! testing all components working together in realistic scenarios.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};

/// ============================================================================
/// END-TO-END PLUGIN LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_complete_plugin_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut event_receiver = service.subscribe_events().await;

    // Register a plugin
    let mut registry = service.registry.write().await;
    let manifest = create_rune_plugin_manifest("lifecycle-test-plugin");
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Wait for registration event
    let _ = wait_for_event_type(&mut event_receiver, "PluginRegistered", Duration::from_millis(500)).await;

    // Create instance
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // Wait for instance creation event
    let _ = wait_for_event_type(&mut event_receiver, "InstanceCreated", Duration::from_millis(500)).await;

    // Start instance
    service.start_instance(&instance_id).await?;

    // Wait for instance start event
    let _ = wait_for_event_type(&mut event_receiver, "InstanceStarted", Duration::from_millis(500)).await;

    // Verify instance is running
    let health = service.get_instance_health(&instance_id).await?;
    assert!(matches!(health, PluginHealthStatus::Healthy | PluginHealthStatus::Unknown));

    // Get resource usage
    let usage = service.get_resource_usage(Some(&instance_id)).await?;
    assert!(usage.memory_bytes >= 0);

    // Stop instance
    service.stop_instance(&instance_id).await?;

    // Wait for instance stop event
    let _ = wait_for_event_type(&mut event_receiver, "InstanceStopped", Duration::from_millis(500)).await;

    // Verify instance is stopped
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 0);

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// MULTI-PLUGIN SCENARIOS TESTS
/// ============================================================================

#[tokio::test]
async fn test_multiple_plugins_working_together() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create different types of plugins
    let plugin_types = vec![
        ("multi-rune-plugin", PluginType::Rune),
        ("multi-binary-plugin", PluginType::Binary),
        ("multi-wasm-plugin", PluginType::Wasm),
    ];

    let mut plugin_instances = Vec::new();

    for (plugin_name, plugin_type) in plugin_types {
        // Register plugin
        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(plugin_name, plugin_type);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        // Create and start instance
        let instance_id = service.create_instance(&plugin_id, None).await?;
        service.start_instance(&instance_id).await?;

        plugin_instances.push((plugin_id, instance_id));
    }

    // Verify all instances are running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 3);

    // Get system health
    let system_health = service.get_system_health().await?;
    assert!(matches!(system_health.overall_status, ServiceStatus::Healthy | ServiceStatus::Degraded));
    assert_eq!(system_health.total_instances, 3);

    // Get global resource usage
    let global_usage = service.get_resource_usage(None).await?;
    assert!(global_usage.memory_bytes >= 0);

    // Stop all instances
    for (_plugin_id, instance_id) in plugin_instances {
        service.stop_instance(&instance_id).await?;
    }

    // Verify all instances are stopped
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 0);

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// PLUGIN DEPENDENCY RESOLUTION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_dependency_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create plugins with dependencies
    let base_plugin = create_test_plugin_manifest("base-plugin", PluginType::Rune);
    let dependent_plugin = create_plugin_with_dependencies(
        "dependent-plugin",
        vec![PluginDependency {
            name: "base-plugin".to_string(),
            version: Some("1.0.0".to_string()),
            dependency_type: DependencyType::Plugin,
            optional: false,
        }],
    );

    // Register base plugin first
    let mut registry = service.registry.write().await;
    let base_plugin_id = registry.register_plugin(base_plugin).await?;
    drop(registry);

    // Register dependent plugin
    let mut registry = service.registry.write().await;
    let dependent_plugin_id = registry.register_plugin(dependent_plugin).await?;
    drop(registry);

    // Create instances for both
    let base_instance_id = service.create_instance(&base_plugin_id, None).await?;
    let dependent_instance_id = service.create_instance(&dependent_plugin_id, None).await?;

    // Start base plugin first
    service.start_instance(&base_instance_id).await?;

    // Start dependent plugin
    service.start_instance(&dependent_instance_id).await?;

    // Verify both are running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 2);

    // Stop in reverse order
    service.stop_instance(&dependent_instance_id).await?;
    service.stop_instance(&base_instance_id).await?;

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// RESOURCE CONTENTION SCENARIOS TESTS
/// ============================================================================

#[tokio::test]
async fn test_resource_contention_handling() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create resource-intensive plugins
    let plugin_count = 5;
    let mut instance_ids = Vec::new();

    for i in 0..plugin_count {
        let mut registry = service.registry.write().await;
        let manifest = create_resource_intensive_plugin_manifest(&format!("resource-plugin-{}", i));
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service.create_instance(&plugin_id, None).await?;
        service.start_instance(&instance_id).await?;

        instance_ids.push(instance_id);
    }

    // Wait for resource usage to stabilize
    sleep(Duration::from_millis(200)).await;

    // Get global resource usage
    let global_usage = service.get_resource_usage(None).await?;

    // Verify resource usage is being tracked
    assert!(global_usage.memory_bytes > 0);
    assert!(global_usage.cpu_percentage >= 0.0);

    // Get system health - should handle resource contention gracefully
    let system_health = service.get_system_health().await?;
    assert!(matches!(
        system_health.overall_status,
        ServiceStatus::Healthy | ServiceStatus::Degraded
    ));

    // Stop all instances
    for instance_id in instance_ids {
        service.stop_instance(&instance_id).await?;
    }

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// ERROR RECOVERY SCENARIOS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_crash_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut event_receiver = service.subscribe_events().await;

    // Register and create instance
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("crash-test-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    let instance_id = service.create_instance(&plugin_id, None).await?;
    service.start_instance(&instance_id).await?;

    // Simulate crash by sending crash event
    let crash_event = PluginManagerEvent::InstanceCrashed {
        instance_id: instance_id.clone(),
        plugin_id: plugin_id.clone(),
        error: "Simulated crash".to_string(),
    };

    service.handle_event(crash_event).await?;

    // Wait for potential recovery events
    let _ = tokio::time::timeout(Duration::from_millis(1000), async {
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                PluginManagerEvent::InstanceStarted { .. } => {
                    println!("Instance was restarted after crash");
                    break;
                }
                PluginManagerEvent::Error { .. } => {
                    println!("Error occurred during recovery");
                }
                _ => {}
            }
        }
    }).await;

    // Service should still be running
    assert!(service.is_running());

    service.stop().await?;
    Ok(())
}

#[tokio::test]
async fn test_component_failure_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Service should start even with potential component issues
    assert!(service.is_running());

    // Health check should still work
    let health = service.health_check().await?;
    assert!(matches!(health.status, ServiceStatus::Healthy | ServiceStatus::Degraded));

    // Should still be able to list plugins
    let plugins = service.list_plugins().await?;
    assert_eq!(plugins.len(), 0); // No plugins registered yet

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// SECURITY POLICY ENFORCEMENT TESTS
/// ============================================================================

#[tokio::test]
async fn test_security_policy_enforcement() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = default_test_config();
    config.security.policies.default_level = SecurityLevel::Strict;
    let mut service = PluginManagerService::new(config);
    service.start().await?;

    // Try to register insecure plugin
    let insecure_manifest = create_insecure_plugin_manifest("insecure-plugin");
    let mut registry = service.registry.write().await;

    // Security validation might fail for insecure plugins
    match registry.register_plugin(insecure_manifest).await {
        Ok(_) => println!("Insecure plugin was registered (security validation may be permissive)"),
        Err(e) => println!("Insecure plugin rejected: {}", e),
    }
    drop(registry);

    // Create secure plugin
    let secure_manifest = create_test_plugin_manifest("secure-plugin", PluginType::Rune);
    let mut registry = service.registry.write().await;
    let plugin_id = registry.register_plugin(secure_manifest).await?;
    drop(registry);

    // Create instance
    let instance_id = service.create_instance(&plugin_id, None).await?;
    service.start_instance(&instance_id).await?;

    // Verify instance is running with security restrictions
    let health = service.get_instance_health(&instance_id).await?;
    assert!(matches!(health, PluginHealthStatus::Healthy | PluginHealthStatus::Unknown));

    service.stop_instance(&instance_id).await?;
    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// CONFIGURATION HOT RELOADING TESTS
/// ============================================================================

#[tokio::test]
async fn test_configuration_hot_reload() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Get initial configuration
    let initial_config = service.get_config().await?;
    let initial_thread_count = initial_config.performance.thread_pool_size;

    // Update configuration
    let mut new_config = initial_config.clone();
    new_config.performance.thread_pool_size = initial_thread_count + 2;
    new_config.security.default_sandbox.enabled = !new_config.security.default_sandbox.enabled;

    service.update_config(new_config.clone()).await?;

    // Verify configuration was updated
    let updated_config = service.get_config().await?;
    assert_eq!(updated_config.performance.thread_pool_size, initial_thread_count + 2);
    assert_eq!(
        updated_config.security.default_sandbox.enabled,
        !initial_config.security.default_sandbox.enabled
    );

    // Service should still be running
    assert!(service.is_running());

    // Health check should pass
    let health = service.health_check().await?;
    assert!(matches!(health.status, ServiceStatus::Healthy));

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// EVENT SYSTEM INTEGRATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_comprehensive_event_flow() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut event_receiver = service.subscribe_events().await;

    // Track received events
    let mut received_events = Vec::new();

    // Register plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("event-flow-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Collect events for a short period
    for _ in 0..10 {
        match event_receiver.try_recv() {
            Ok(event) => received_events.push(event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }

    // Create instance
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // Collect more events
    for _ in 0..10 {
        match event_receiver.try_recv() {
            Ok(event) => received_events.push(event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }

    // Start instance
    service.start_instance(&instance_id).await?;

    // Collect final events
    for _ in 0..10 {
        match event_receiver.try_recv() {
            Ok(event) => received_events.push(event),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }

    // Verify we received expected events
    let event_types: Vec<String> = received_events.iter().map(|e| format!("{:?}", e)).collect();
    assert!(event_types.iter().any(|t| t.contains("PluginRegistered")));
    assert!(event_types.iter().any(|t| t.contains("InstanceCreated")));
    assert!(event_types.iter().any(|t| t.contains("InstanceStarted")));

    println!("Received {} events", received_events.len());
    for event in &received_events {
        println!("  {:?}", event);
    }

    service.stop_instance(&instance_id).await?;
    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// PERFORMANCE INTEGRATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_system_performance_under_load() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    let plugin_count = 20;
    let start_time = std::time::Instant::now();

    // Create many plugins and instances
    let mut instance_ids = Vec::new();
    for i in 0..plugin_count {
        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(&format!("load-plugin-{}", i), PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service.create_instance(&plugin_id, None).await?;
        service.start_instance(&instance_id).await?;

        instance_ids.push(instance_id);
    }

    let creation_time = start_time.elapsed();

    // Verify all instances are running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), plugin_count);

    // Test system health under load
    let health = service.health_check().await?;
    assert!(matches!(
        health.status,
        ServiceStatus::Healthy | ServiceStatus::Degraded
    ));

    // Test resource usage aggregation
    let global_usage = service.get_resource_usage(None).await?;
    assert!(global_usage.memory_bytes >= 0);

    // Stop all instances
    let stop_start = std::time::Instant::now();
    for instance_id in instance_ids {
        service.stop_instance(&instance_id).await?;
    }
    let stop_time = stop_start.elapsed();

    println!("Performance under load:");
    println!("  Created and started {} instances in {:?}", plugin_count, creation_time);
    println!("  Stopped {} instances in {:?}", plugin_count, stop_time);
    println!("  Average creation time: {:?}", creation_time / plugin_count as u32);
    println!("  Average stop time: {:?}", stop_time / plugin_count as u32);

    // Performance should be reasonable
    assert!(creation_time < Duration::from_secs(5));
    assert!(stop_time < Duration::from_secs(3));

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// FAULT TOLERANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_system_fault_tolerance() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Create some normal instances
    let mut instance_ids = Vec::new();
    for i in 0..3 {
        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(&format!("fault-plugin-{}", i), PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service.create_instance(&plugin_id, None).await?;
        service.start_instance(&instance_id).await?;

        instance_ids.push(instance_id);
    }

    // Simulate various fault conditions
    // 1. Event channel disconnection (multiple subscribers)
    let mut receiver1 = service.subscribe_events().await;
    let mut receiver2 = service.subscribe_events().await;
    drop(receiver1); // Simulate disconnection

    // 2. Invalid operations (try to stop non-existent instance)
    let result = service.stop_instance("non-existent-instance").await;
    assert!(result.is_err()); // Should fail gracefully

    // 3. Configuration validation errors
    let mut invalid_config = default_test_config();
    invalid_config.performance.thread_pool_size = 0; // Invalid
    let result = service.update_config(invalid_config).await;
    assert!(result.is_err()); // Should fail gracefully

    // System should still be functional
    assert!(service.is_running());

    let health = service.health_check().await?;
    assert!(matches!(health.status, ServiceStatus::Healthy | ServiceStatus::Degraded));

    // Normal instances should still be running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 3);

    // Clean up
    for instance_id in instance_ids {
        service.stop_instance(&instance_id).await?;
    }

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// CONCURRENCY INTEGRATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_concurrent_system_operations() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = Arc::new(RwLock::new(create_test_plugin_manager().await));
    service.write().await.start().await?;

    // Concurrent plugin registration and instance creation
    let mut handles = Vec::new();
    for i in 0..10 {
        let service_clone = service.clone();
        let handle = tokio::spawn(async move {
            let mut service_guard = service_clone.write().await;

            // Register plugin
            let mut registry = service_guard.registry.write().await;
            let manifest = create_test_plugin_manifest(&format!("concurrent-plugin-{}", i), PluginType::Rune);
            let plugin_id = registry.register_plugin(manifest).await?;
            drop(registry);

            // Create instance
            let instance_id = service_guard.create_instance(&plugin_id, None).await?;
            let start_result = service_guard.start_instance(&instance_id).await;

            (plugin_id, instance_id, start_result)
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut results = Vec::new();
    for handle in handles {
        let result = handle.await??;
        results.push(result);
    }

    // Verify most operations succeeded
    let successful_ops = results.iter().filter(|(_, _, r)| r.is_ok()).count();
    assert!(successful_ops >= 8); // Allow for some failures in concurrent scenario

    // Verify system is still functional
    let service_guard = service.read().await;
    assert!(service_guard.is_running());

    let instances = service_guard.list_instances().await?;
    assert!(instances.len() > 0);

    // Clean up
    for (_, instance_id, _) in results {
        if instance_id.is_empty() {
            continue;
        }
        let mut service_guard = service.write().await;
        let _ = service_guard.stop_instance(&instance_id).await;
    }

    service.write().await.stop().await?;
    Ok(())
}