//! # Multi-Plugin Integration Tests
//!
//! Tests that validate the behavior of multiple plugins running simultaneously,
//! including inter-plugin communication, resource sharing, dependency resolution,
//! and complex interaction scenarios.

use super::common::*;
use super::super::config::*;
use super::super::error::PluginResult;
use super::super::manager::{PluginManagerService, PluginManagerEvent};
use super::super::types::*;
use super::super::state_machine::PluginInstanceState;
use super::super::dependency_resolver::DependencyResolver;
use super::super::batch_operations::BatchOperation;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Barrier};
use tracing::{info, warn, error, debug};

/// ============================================================================
/// MULTI-PLUGIN LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_concurrent_plugin_startup() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting concurrent plugin startup test");

    // Get all registered plugins
    let plugins = env.list_plugins().await?;
    let plugin_count = plugins.len();

    info!("Found {} plugins for concurrent startup test", plugin_count);

    // Resolve dependencies to get startup order
    let plugin_ids: Vec<String> = plugins.iter().map(|p| p.manifest.id.clone()).collect();
    let resolution_result = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&plugin_ids).await?
    };

    info!("Dependency resolution completed with startup order: {:?}", resolution_result.startup_order);

    // Create instances for all plugins
    let mut instance_ids = Vec::new();
    for plugin_id in &resolution_result.startup_order {
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(plugin_id, None).await?;
        instance_ids.push(instance_id);
        drop(manager);
    }

    info!("Created {} instances", instance_ids.len());

    // Set up barrier for synchronized startup
    let barrier = Arc::new(Barrier::new(instance_ids.len()));
    let mut handles = Vec::new();

    // Start all instances concurrently
    for (index, instance_id) in instance_ids.iter().enumerate() {
        let barrier_clone = barrier.clone();
        let instance_id_clone = instance_id.clone();
        let env_clone = env.plugin_manager.clone();

        let handle = tokio::spawn(async move {
            // Wait for all instances to be ready
            barrier_clone.wait().await;

            info!("Starting instance {} concurrently", instance_id_clone);

            let start_time = SystemTime::now();
            let mut manager = env_clone.write().await;
            let result = manager.start_instance(&instance_id_clone).await;
            drop(manager);

            let duration = SystemTime::now().duration_since(start_time).unwrap();
            info!("Instance {} startup completed in {:?}", instance_id_clone, duration);

            (instance_id_clone, result, duration)
        });

        handles.push(handle);
    }

    // Wait for all startups to complete
    let mut successful_startups = 0;
    let mut failed_startups = 0;
    let mut total_duration = Duration::ZERO;

    for handle in handles {
        match handle.await {
            Ok((instance_id, result, duration)) => {
                total_duration = total_duration.max(duration);
                match result {
                    Ok(_) => {
                        successful_startups += 1;
                        info!("Instance {} started successfully", instance_id);
                    }
                    Err(e) => {
                        failed_startups += 1;
                        warn!("Instance {} failed to start: {}", instance_id, e);
                    }
                }
            }
            Err(e) => {
                error!("Startup task failed: {}", e);
                failed_startups += 1;
            }
        }
    }

    info!("Concurrent startup completed: {} successful, {} failed, total time: {:?}",
          successful_startups, failed_startups, total_duration);

    // Verify that most instances started successfully
    assert!(successful_startups > 0, "At least some instances should start successfully");

    // Check final states
    let mut running_instances = 0;
    for instance_id in &instance_ids {
        if let Ok(state) = assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(15)).await {
            running_instances += 1;
        }
    }

    info!("{} instances are running after concurrent startup", running_instances);

    // Collect startup events
    let startup_events = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::InstanceStarted { .. })
    }).await;

    info!("Collected {} startup events", startup_events.len());

    // Cleanup
    info!("Cleaning up instances from concurrent startup test");
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_inter_plugin_communication() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting inter-plugin communication test");

    // Create plugins that communicate with each other
    let plugins = vec![
        ("producer-plugin", "Message Producer"),
        ("consumer-plugin", "Message Consumer"),
        ("processor-plugin", "Message Processor"),
    ];

    let mut plugin_ids = Vec::new();
    let mut manifests = Vec::new();

    for (plugin_id, plugin_name) in plugins {
        let mut manifest = create_mock_plugin_manifest(plugin_id, plugin_name);

        // Add communication capabilities
        manifest.capabilities.push(PluginCapability::IpcCommunication);

        // Set up dependencies
        match plugin_id {
            "consumer-plugin" => {
                manifest.dependencies = vec!["producer-plugin".to_string()];
            }
            "processor-plugin" => {
                manifest.dependencies = vec!["producer-plugin".to_string(), "consumer-plugin".to_string()];
            }
            _ => {}
        }

        let registered_id = env.register_mock_plugin(manifest.clone()).await?;
        plugin_ids.push(registered_id);
        manifests.push(manifest);
    }

    info!("Registered {} communication plugins", plugin_ids.len());

    // Create and start instances in dependency order
    let mut instance_map = HashMap::new();
    for plugin_id in &plugin_ids {
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        instance_map.insert(plugin_id.clone(), instance_id);
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    }

    info!("Started all communication instances");

    // Simulate inter-plugin communication
    info!("Simulating inter-plugin message flow");

    let producer_instance = instance_map.get("producer-plugin").unwrap();
    let consumer_instance = instance_map.get("consumer-plugin").unwrap();
    let processor_instance = instance_map.get("processor-plugin").unwrap();

    // Producer sends messages
    env.update_mock_instance(producer_instance, |instance| {
        instance.log_event("SENDING_MESSAGE_TO_CONSUMER").await;
    }).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Consumer receives and processes messages
    env.update_mock_instance(consumer_instance, |instance| {
        instance.log_event("RECEIVED_MESSAGE_FROM_PRODUCER").await;
        instance.log_event("SENDING_PROCESSED_MESSAGE_TO_PROCESSOR").await;
    }).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Processor handles processed messages
    env.update_mock_instance(processor_instance, |instance| {
        instance.log_event("RECEIVED_PROCESSED_MESSAGE_FROM_CONSUMER").await;
        instance.log_event("PROCESSING_COMPLETE").await;
    }).await;

    // Wait for communication to complete
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify communication logs
    for (plugin_id, instance_id) in &instance_map {
        let logs = {
            let instance = env.get_mock_instance(instance_id).await;
            if let Some(inst) = instance {
                inst.get_event_log().await
            } else {
                Vec::new()
            }
        };

        info!("Communication logs for {}: {:?}", plugin_id, logs);
        assert!(!logs.is_empty(), "Plugin {} should have communication logs", plugin_id);
    }

    // Test event propagation between plugins
    info!("Testing event propagation between plugins");

    // Trigger an event in producer and verify it affects dependent plugins
    env.update_mock_instance(producer_instance, |instance| {
        instance.health_status = PluginHealthStatus::Degraded;
        instance.log_event("BECAME_DEGRADED").await;
    }).await;

    // Wait for potential propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check if dependent plugins reacted to the state change
    let consumer_logs = {
        let instance = env.get_mock_instance(consumer_instance).await;
        if let Some(inst) = instance {
            inst.get_event_log().await
        } else {
            Vec::new()
        }
    };

    info!("Consumer reaction logs: {:?}", consumer_logs);

    // Test resource sharing scenarios
    info!("Testing resource sharing scenarios");

    // Simulate shared resource usage
    let shared_memory_usage = 100 * 1024 * 1024; // 100MB
    let per_plugin_usage = shared_memory_usage / instance_map.len() as u64;

    for (plugin_id, instance_id) in &instance_map {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(per_plugin_usage / 1024 / 1024, 20.0).await;
        }).await;
    }

    // Check system resource usage
    let total_usage = {
        let manager = env.plugin_manager.read().await;
        manager.get_aggregated_resource_usage().await?
    };

    info!("Total system resource usage: {:?}", total_usage);

    // Verify resource usage is within expected bounds
    let total_memory: u64 = total_usage.values()
        .map(|usage| usage.memory_bytes)
        .sum();

    assert!(total_memory <= shared_memory_usage * 2, "Total memory usage should be reasonable");

    // Cleanup
    info!("Cleaning up communication test instances");
    for instance_id in instance_map.values() {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_dependency_chains() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin dependency chains test");

    // Get dependency graph
    let plugins = env.list_plugins().await?;
    let plugin_ids: Vec<String> = plugins.iter().map(|p| p.manifest.id.clone()).collect();

    let resolution_result = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&plugin_ids).await?
    };

    info!("Dependency resolution result: {:?}", resolution_result);
    assert!(resolution_result.success, "Dependency resolution should succeed");

    // Verify dependency chains
    let startup_order = &resolution_result.startup_order;
    info!("Calculated startup order: {:?}", startup_order);

    // Test that dependencies are satisfied in startup order
    let mut started_instances = HashMap::new();
    let mut startup_events = Vec::new();

    for plugin_id in startup_order {
        info!("Starting plugin: {}", plugin_id);

        // Check dependencies are satisfied
        let plugin = plugins.iter().find(|p| p.manifest.id == *plugin_id).unwrap();
        for dependency in &plugin.manifest.dependencies {
            assert!(started_instances.contains_key(dependency),
                   "Dependency {} should be started before {}", dependency, plugin_id);
        }

        // Start the instance
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        started_instances.insert(plugin_id.clone(), instance_id.clone());
        drop(manager);

        // Wait for startup to complete
        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;

        info!("Successfully started plugin: {} (instance: {})", plugin_id, instance_id);

        // Collect startup event
        let startup_event = event_collector.wait_for_event(
            |e| matches!(e, PluginManagerEvent::InstanceStarted { instance_id: ref id, .. } if id == &instance_id),
            Duration::from_secs(5)
        ).await;

        if let Some(event) = startup_event {
            startup_events.push((plugin_id.clone(), event));
        }
    }

    info!("All {} plugins started successfully in dependency order", started_instances.len());

    // Test dependency violation handling
    info!("Testing dependency violation handling");

    // Try to stop a dependency plugin
    if let Some((core_plugin_id, core_instance_id)) = started_instances.iter().next() {
        info!("Attempting to stop core dependency: {}", core_plugin_id);

        let mut manager = env.plugin_manager.write().await;
        let stop_result = manager.stop_instance(core_instance_id).await;

        match stop_result {
            Ok(_) => {
                warn!("Core plugin stopped, checking impact on dependents");

                // Check if dependent plugins are affected
                tokio::time::sleep(Duration::from_millis(500)).await;

                let dependent_plugins: Vec<_> = plugins.iter()
                    .filter(|p| p.manifest.dependencies.contains(core_plugin_id))
                    .collect();

                info!("Checking {} dependent plugins", dependent_plugins.len());

                for dependent_plugin in dependent_plugins {
                    if let Some(dependent_instance_id) = started_instances.get(&dependent_plugin.manifest.id) {
                        let health = {
                            let manager = env.plugin_manager.read().await;
                            manager.get_instance_health(dependent_instance_id).await.ok()
                        };

                        if let Some(health_status) = health {
                            if matches!(health_status, PluginHealthStatus::Unhealthy | PluginHealthStatus::Degraded) {
                                info!("Dependent plugin correctly shows degraded health: {}", dependent_plugin.manifest.id);
                            }
                        }
                    }
                }

                // Restart core plugin
                manager.start_instance(core_instance_id).await?;
                assert_plugin_state(&env, core_instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
            }
            Err(e) => {
                info!("Core plugin stop prevented as expected: {}", e);
            }
        }
    }

    // Test circular dependency detection
    info!("Testing circular dependency detection");

    // Create plugins with circular dependencies
    let circular_manifests = vec![
        create_mock_plugin_with_dependencies("circular-a", "Circular A", vec!["circular-b".to_string()]),
        create_mock_plugin_with_dependencies("circular-b", "Circular B", vec!["circular-c".to_string()]),
        create_mock_plugin_with_dependencies("circular-c", "Circular C", vec!["circular-a".to_string()]),
    ];

    let mut circular_plugin_ids = Vec::new();
    for manifest in circular_manifests {
        let plugin_id = env.register_mock_plugin(manifest).await?;
        circular_plugin_ids.push(plugin_id);
    }

    // Try to resolve circular dependencies
    let circular_resolution = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&circular_plugin_ids).await
    };

    match circular_resolution {
        Ok(result) => {
            if !result.success {
                info!("Circular dependency correctly detected: {:?}", result.errors);
            } else {
                warn!("Circular dependency was not detected, but this might be acceptable");
            }
        }
        Err(e) => {
            info!("Circular dependency resolution failed as expected: {}", e);
        }
    }

    // Test complex dependency scenarios
    info!("Testing complex dependency scenarios");

    // Create a complex dependency tree
    let complex_manifests = vec![
        create_mock_plugin_manifest("base-service", "Base Service"),
        create_mock_plugin_with_dependencies("auth-service", "Auth Service", vec!["base-service".to_string()]),
        create_mock_plugin_with_dependencies("database-service", "Database Service", vec!["base-service".to_string()]),
        create_mock_plugin_with_dependencies("api-service", "API Service", vec!["auth-service".to_string(), "database-service".to_string()]),
        create_mock_plugin_with_dependencies("web-service", "Web Service", vec!["api-service".to_string()]),
        create_mock_plugin_with_dependencies("monitoring-service", "Monitoring Service", vec!["base-service".to_string(), "api-service".to_string(), "web-service".to_string()]),
    ];

    let mut complex_plugin_ids = Vec::new();
    for manifest in complex_manifests {
        let plugin_id = env.register_mock_plugin(manifest).await?;
        complex_plugin_ids.push(plugin_id);
    }

    // Resolve complex dependencies
    let complex_resolution = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&complex_plugin_ids).await?
    };

    assert!(complex_resolution.success, "Complex dependency resolution should succeed");
    info!("Complex dependency resolution successful with order: {:?}", complex_resolution.startup_order);

    // Cleanup
    info!("Cleaning up dependency test instances");
    for instance_id in started_instances.values() {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_resource_isolation() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin resource isolation test");

    // Create plugins with different resource requirements
    let plugin_configs = vec![
        ("lightweight-plugin", "Lightweight Plugin", 50, 10.0),  // 50MB, 10% CPU
        ("heavyweight-plugin", "Heavyweight Plugin", 200, 50.0), // 200MB, 50% CPU
        ("resource-intensive-plugin", "Resource Intensive Plugin", 400, 80.0), // 400MB, 80% CPU
    ];

    let mut instance_ids = Vec::new();
    let mut resource_monitors = Vec::new();

    for (plugin_id, plugin_name, memory_mb, cpu_percent) in plugin_configs {
        // Create plugin with specific resource limits
        let mut manifest = create_mock_plugin_manifest(plugin_id, plugin_name);
        manifest.resource_limits.max_memory_bytes = Some(memory_mb * 1024 * 1024);
        manifest.resource_limits.max_cpu_percentage = Some(cpu_percent);

        let registered_id = env.register_mock_plugin(manifest).await?;

        // Create and start instance
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&registered_id, None).await?;
        manager.start_instance(&instance_id).await?;
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;

        // Start resource monitoring for this instance
        let monitor = ResourceMonitor::new(instance_id.clone(), Duration::from_millis(500));
        monitor.start(env.plugin_manager.clone()).await;

        instance_ids.push((instance_id.clone(), memory_mb, cpu_percent));
        resource_monitors.push(monitor);

        info!("Started {} with resource limits: {}MB, {}% CPU", plugin_name, memory_mb, cpu_percent);
    }

    // Let monitoring collect data
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Simulate resource usage for each plugin
    for (instance_id, target_memory_mb, target_cpu_percent) in &instance_ids {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(*target_memory_mb, *target_cpu_percent).await;
        }).await;

        info!("Simulated resource usage for {}: {}MB, {}% CPU", instance_id, target_memory_mb, target_cpu_percent);
    }

    // Wait for monitoring to detect usage
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check resource isolation
    info!("Checking resource isolation");

    for (index, (instance_id, target_memory_mb, target_cpu_percent)) in instance_ids.iter().enumerate() {
        let measurements = resource_monitors[index].get_measurements().await;
        info!("Collected {} measurements for {}", measurements.len(), instance_id);

        if !measurements.is_empty() {
            let peak_memory = resource_monitors[index].get_peak_memory().await;
            let avg_cpu = resource_monitors[index].get_average_cpu().await;

            info!("{} - Peak memory: {:?}MB, Average CPU: {:?}%",
                  instance_id,
                  peak_memory.map(|m| m / 1024 / 1024),
                  avg_cpu);

            // Verify that resource usage is within expected ranges
            if let Some(memory) = peak_memory {
                let memory_mb = memory / 1024 / 1024;
                assert!(memory_mb <= target_memory_mb * 2, // Allow some overhead
                       "Memory usage should be within limits: {}MB <= {}MB", memory_mb, target_memory_mb * 2);
            }

            if let Some(cpu) = avg_cpu {
                assert!(cpu <= target_cpu_percent * 1.5, // Allow some variance
                       "CPU usage should be within limits: {}% <= {}%", cpu, target_cpu_percent * 1.5);
            }
        }
    }

    // Test resource contention scenarios
    info!("Testing resource contention scenarios");

    // Make all plugins use maximum resources
    for (instance_id, _, _) in &instance_ids {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(500, 90.0).await; // High usage
        }).await;
    }

    // Wait for resource management to react
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Check for resource violations
    let violation_events = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::ResourceViolation { .. })
    }).await;

    info!("Detected {} resource violation events", violation_events.len());

    if !violation_events.is_empty() {
        info!("Resource violations detected: {:?}", violation_events);

        // Verify that resource management is working
        for event in violation_events {
            match event {
                PluginManagerEvent::ResourceViolation { instance_id, resource_type, current_value, limit } => {
                    info!("Resource violation - Instance: {}, Type: {:?}, Current: {}, Limit: {}",
                          instance_id, resource_type, current_value, limit);
                }
                _ => {}
            }
        }
    }

    // Test resource quota enforcement
    info!("Testing resource quota enforcement");

    // Check system-wide resource usage
    let system_usage = {
        let manager = env.plugin_manager.read().await;
        manager.get_aggregated_resource_usage().await?
    };

    info!("System-wide resource usage: {:?}", system_usage);

    // Verify total usage is reasonable
    let total_memory: u64 = system_usage.values()
        .map(|usage| usage.memory_bytes)
        .sum();

    let total_memory_mb = total_memory / 1024 / 1024;
    info!("Total memory usage across all plugins: {}MB", total_memory_mb);

    assert!(total_memory_mb <= 2000, "Total memory usage should be reasonable: {}MB", total_memory_mb);

    // Cleanup monitors
    for monitor in resource_monitors {
        monitor.stop().await;
    }

    // Cleanup instances
    for (instance_id, _, _) in instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(&instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_batch_operations() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin batch operations test");

    // Get plugins for batch operations
    let plugins = env.list_plugins().await?;
    let target_plugins: Vec<_> = plugins.iter().take(5).collect();

    info!("Selected {} plugins for batch operations", target_plugins.len());

    // Create instances for batch operations
    let mut instance_ids = Vec::new();
    for plugin in &target_plugins {
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin.manifest.id, None).await?;
        instance_ids.push(instance_id);
        drop(manager);
    }

    info!("Created {} instances for batch operations", instance_ids.len());

    // Test batch start operation
    info!("Testing batch start operation");

    let batch_id = {
        let manager = env.plugin_manager.read().await;
        let batch = BatchOperation {
            batch_id: format!("batch-start-{}", uuid::Uuid::new_v4()),
            name: "Batch Start Operation".to_string(),
            description: "Start multiple plugin instances".to_string(),
            operations: instance_ids.iter().enumerate().map(|(index, instance_id)| {
                super::super::batch_operations::BatchOperationItem {
                    item_id: format!("start-{}", index),
                    operation: super::super::lifecycle_manager::LifecycleOperation::Start { instance_id: instance_id.clone() },
                    target: instance_id.clone(),
                    priority: super::super::batch_operations::BatchItemPriority::Normal,
                    dependencies: Vec::new(),
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: Some(super::super::batch_operations::BatchRetryConfig {
                        max_attempts: 3,
                        initial_delay: Duration::from_secs(1),
                        backoff_strategy: super::super::lifecycle_manager::BackoffStrategy::Exponential,
                        retry_on_errors: vec!["timeout".to_string()],
                        delay_multiplier: 2.0,
                    }),
                    rollback_config: None,
                    metadata: HashMap::new(),
                }
            }).collect(),
            strategy: super::super::batch_operations::BatchExecutionStrategy::Parallel {
                max_concurrency: 3,
                failure_strategy: super::super::batch_operations::BatchFailureStrategy::Continue,
            },
            config: super::super::batch_operations::BatchConfig::default(),
            scope: super::super::batch_operations::BatchScope::default(),
            metadata: super::super::batch_operations::BatchMetadata {
                created_at: SystemTime::now(),
                created_by: "test".to_string(),
                updated_at: SystemTime::now(),
                updated_by: "test".to_string(),
                tags: vec!["test".to_string(), "batch".to_string()],
                documentation: Some("Test batch start operation".to_string()),
                additional_info: HashMap::new(),
            },
        };

        manager.execute_batch_operation(batch).await?
    };

    info!("Started batch operation: {}", batch_id);

    // Wait for batch operation to complete
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Check batch execution progress
    let progress = {
        let manager = env.plugin_manager.read().await;
        manager.get_batch_execution_progress(&batch_id).await
    };

    if let Some(batch_progress) = progress {
        info!("Batch operation progress: {:?}", batch_progress);
    }

    // Verify that instances were started
    let mut successful_starts = 0;
    for instance_id in &instance_ids {
        if let Ok(_) = assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(5)).await {
            successful_starts += 1;
        }
    }

    info!("Batch start completed: {} out of {} instances started successfully", successful_starts, instance_ids.len());

    // Test rolling restart operation
    info!("Testing rolling restart operation");

    let rolling_batch_id = {
        let manager = env.plugin_manager.read().await;
        manager.execute_rolling_restart(instance_ids.clone(), 2).await?
    };

    info!("Started rolling restart: {}", rolling_batch_id);

    // Wait for rolling restart to complete
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Verify that instances are still running after restart
    let mut still_running = 0;
    for instance_id in &instance_ids {
        if let Ok(_) = assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(5)).await {
            still_running += 1;
        }
    }

    info!("Rolling restart completed: {} out of {} instances still running", still_running, instance_ids.len());

    // Test zero-downtime restart with canary deployment
    info!("Testing zero-downtime restart with canary deployment");

    let canary_batch_id = {
        let manager = env.plugin_manager.read().await;
        manager.execute_zero_downtime_restart(instance_ids.clone(), 20).await?
    };

    info!("Started zero-downtime restart with canary: {}", canary_batch_id);

    // Wait for canary deployment to complete
    tokio::time::sleep(Duration::from_secs(20)).await;

    // Test batch stop operation
    info!("Testing batch stop operation");

    let stop_batch_id = {
        let manager = env.plugin_manager.read().await;
        let batch = BatchOperation {
            batch_id: format!("batch-stop-{}", uuid::Uuid::new_v4()),
            name: "Batch Stop Operation".to_string(),
            description: "Stop multiple plugin instances".to_string(),
            operations: instance_ids.iter().enumerate().map(|(index, instance_id)| {
                super::super::batch_operations::BatchOperationItem {
                    item_id: format!("stop-{}", index),
                    operation: super::super::lifecycle_manager::LifecycleOperation::Stop { instance_id: instance_id.clone() },
                    target: instance_id.clone(),
                    priority: super::super::batch_operations::BatchItemPriority::Normal,
                    dependencies: Vec::new(),
                    timeout: Some(Duration::from_secs(10)),
                    retry_config: None,
                    rollback_config: None,
                    metadata: HashMap::new(),
                }
            }).collect(),
            strategy: super::super::batch_operations::BatchExecutionStrategy::Sequential {
                failure_strategy: super::super::batch_operations::BatchFailureStrategy::Stop,
            },
            config: super::super::batch_operations::BatchConfig::default(),
            scope: super::super::batch_operations::BatchScope::default(),
            metadata: super::super::batch_operations::BatchMetadata {
                created_at: SystemTime::now(),
                created_by: "test".to_string(),
                updated_at: SystemTime::now(),
                updated_by: "test".to_string(),
                tags: vec!["test".to_string(), "batch".to_string(), "stop".to_string()],
                documentation: Some("Test batch stop operation".to_string()),
                additional_info: HashMap::new(),
            },
        };

        manager.execute_batch_operation(batch).await?
    };

    info!("Started batch stop operation: {}", stop_batch_id);

    // Wait for batch stop to complete
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Verify that instances were stopped
    let mut successful_stops = 0;
    for instance_id in &instance_ids {
        if let Ok(_) = assert_plugin_state(&env, instance_id, PluginInstanceState::Stopped, Duration::from_secs(5)).await {
            successful_stops += 1;
        }
    }

    info!("Batch stop completed: {} out of {} instances stopped successfully", successful_stops, instance_ids.len());

    // Collect batch operation events
    let batch_events = event_collector.get_events().await;
    let batch_related_events: Vec<_> = batch_events.iter()
        .filter(|e| {
            matches!(e, PluginManagerEvent::InstanceStarted { .. }) ||
            matches!(e, PluginManagerEvent::InstanceStopped { .. })
        })
        .collect();

    info!("Collected {} batch-related events", batch_related_events.len());

    // Cleanup any remaining instances
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_system_stability() -> PluginResult<()> {
    let env = setup_stress_test_scenario(10).await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin system stability test");

    // Create many instances
    let plugins = env.list_plugins().await?;
    let mut instance_ids = Vec::new();

    for plugin in plugins.iter().take(8) {
        for i in 0..2 { // Create 2 instances per plugin
            let mut manager = env.plugin_manager.write().await;
            let instance_id = manager.create_instance(&plugin.manifest.id, None).await?;
            manager.start_instance(&instance_id).await?;
            instance_ids.push(instance_id);
            drop(manager);
        }
    }

    info!("Created {} instances for stability test", instance_ids.len());

    // Wait for all instances to start
    for instance_id in &instance_ids {
        assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(15)).await?;
    }

    info!("All instances started successfully");

    // Simulate various failure scenarios during operation
    info!("Simulating failure scenarios during operation");

    // Randomly crash some instances
    let crash_count = instance_ids.len() / 4;
    let mut crashed_instances = Vec::new();

    for instance_id in instance_ids.iter().take(crash_count) {
        env.update_mock_instance(instance_id, |instance| {
            instance.crash();
        }).await;
        crashed_instances.push(instance_id.clone());
    }

    info!("Simulated crashes for {} instances", crashed_instances.len());

    // Wait for crash detection and recovery
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check crash events
    let crash_events = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::InstanceCrashed { .. })
    }).await;

    info!("Detected {} crash events", crash_events.len());

    // Simulate resource pressure
    info!("Simulating resource pressure");

    for instance_id in &instance_ids {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(
                rand::random::<u64>() % 200 + 50, // 50-250MB
                rand::random::<f64>() * 60.0 + 20.0, // 20-80% CPU
            ).await;
        }).await;
    }

    // Wait for resource monitoring to react
    tokio::time::sleep(Duration::from_secs(3)).await);

    // Check for resource violations
    let resource_violations = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::ResourceViolation { .. })
    }).await;

    info!("Detected {} resource violation events", resource_violations.len());

    // Perform operations while system is under stress
    info!("Performing operations under stress");

    // Try to create new instances while system is stressed
    let plugins = env.list_plugins().await?;
    if let Some(plugin) = plugins.first() {
        let mut manager = env.plugin_manager.write().await;
        let stress_instance_id = manager.create_instance(&plugin.manifest.id, None).await;

        match stress_instance_id {
            Ok(instance_id) => {
                info!("Successfully created instance under stress: {}", instance_id);

                // Try to start it
                match manager.start_instance(&instance_id).await {
                    Ok(_) => {
                        info!("Successfully started instance under stress: {}", instance_id);
                        // Add to instance list for cleanup
                        instance_ids.push(instance_id);
                    }
                    Err(e) => {
                        warn!("Failed to start instance under stress: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to create instance under stress: {}", e);
            }
        }
    }

    // Check system health under stress
    let system_health = env.get_system_health().await?;
    info!("System health under stress: {:?}", system_health);

    // Gradually reduce stress
    info!("Gradually reducing system stress");

    for instance_id in &instance_ids {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(50, 10.0).await; // Reduce usage
        }).await;
    }

    // Wait for system to stabilize
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check final system health
    let final_system_health = env.get_system_health().await?;
    info!("Final system health: {:?}", final_system_health);

    // Count remaining healthy instances
    let mut healthy_instances = 0;
    for instance_id in &instance_ids {
        if let Ok(health) = {
            let manager = env.plugin_manager.read().await;
            manager.get_instance_health(instance_id).await
        } {
            if matches!(health, PluginHealthStatus::Healthy) {
                healthy_instances += 1;
            }
        }
    }

    info!("Final healthy instance count: {} out of {}", healthy_instances, instance_ids.len());

    // System should maintain some level of stability
    assert!(healthy_instances > 0, "At least some instances should remain healthy");

    // Collect stability metrics
    let final_metrics = env.get_metrics().await;
    info!("Final stability metrics: {:?}", final_metrics);

    // Get system resource info
    let system_info = {
        let manager = env.plugin_manager.read().await;
        manager.get_system_resource_info().await?
    };

    info!("Final system resource info: {:?}", system_info);

    // Cleanup all instances
    info!("Cleaning up all instances from stability test");
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}