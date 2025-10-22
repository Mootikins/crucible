//! # Plugin System Integration Tests
//!
//! End-to-end integration tests that validate the entire plugin ecosystem
//! working together. These tests cover complete plugin lifecycles, multi-plugin
//! scenarios, and system-wide behavior.

use super::common::*;
use super::super::config::*;
use super::super::error::PluginResult;
use super::super::manager::{PluginManagerService, PluginManagerEvent};
use super::super::types::*;
use super::super::state_machine::{PluginStateMachine, StateTransition, PluginInstanceState};
use super::super::dependency_resolver::DependencyResolver;
use super::super::lifecycle_policy::LifecyclePolicyEngine;
use super::super::automation_engine::AutomationEngine;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// ============================================================================
/// END-TO-END PLUGIN LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_complete_plugin_lifecycle() -> PluginResult<()> {
    // Initialize test environment
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    // Create and register a test plugin
    let manifest = create_mock_plugin_manifest("lifecycle-test-plugin", "Lifecycle Test Plugin");
    let plugin_id = env.register_mock_plugin(manifest).await?;

    info!("Starting complete plugin lifecycle test for plugin: {}", plugin_id);

    // Phase 1: Plugin Discovery and Registration
    info!("Phase 1: Testing plugin discovery and registration");

    // Wait for plugin discovery event
    let discovery_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::PluginDiscovered { .. }),
        Duration::from_secs(5)
    ).await;

    assert!(discovery_event.is_some(), "Plugin discovery event should be received");

    // Wait for registration event
    let registration_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::PluginRegistered { plugin_id: ref id } if id == &plugin_id),
        Duration::from_secs(5)
    ).await;

    assert!(registration_event.is_some(), "Plugin registration event should be received");

    // Verify plugin is registered
    let plugins = env.list_plugins().await?;
    assert!(!plugins.is_empty(), "Plugins should be registered");

    let registered_plugin = plugins.iter().find(|p| p.manifest.id == plugin_id);
    assert!(registered_plugin.is_some(), "Test plugin should be registered");

    // Phase 2: Instance Creation
    info!("Phase 2: Testing instance creation");

    let mut manager = env.plugin_manager.write().await;
    let instance_id = manager.create_instance(&plugin_id, None).await?;
    drop(manager);

    info!("Created instance: {}", instance_id);

    // Wait for instance creation event
    let creation_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::InstanceCreated { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(5)
    ).await;

    assert!(creation_event.is_some(), "Instance creation event should be received");

    // Verify instance state
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Created, Duration::from_secs(5)).await?;

    // Phase 3: Instance Startup
    info!("Phase 3: Testing instance startup");

    let mut manager = env.plugin_manager.write().await;
    manager.start_instance(&instance_id).await?;
    drop(manager);

    info!("Started instance: {}", instance_id);

    // Wait for instance start event
    let start_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::InstanceStarted { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(10)
    ).await;

    assert!(start_event.is_some(), "Instance start event should be received");

    // Verify instance is running and healthy
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    assert_plugin_healthy(&env, &instance_id, Duration::from_secs(10)).await?;

    // Phase 4: Resource Monitoring
    info!("Phase 4: Testing resource monitoring");

    // Get initial resource usage
    let initial_usage = {
        let manager = env.plugin_manager.read().await;
        manager.get_resource_usage(Some(&instance_id)).await?
    };

    info!("Initial resource usage: {:?}", initial_usage);

    // Start resource monitoring
    let monitor = ResourceMonitor::new(instance_id.clone(), Duration::from_millis(500));
    monitor.start(env.plugin_manager.clone()).await;

    // Wait for some monitoring data
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check that monitoring data is being collected
    let measurements = monitor.get_measurements().await;
    assert!(!measurements.is_empty(), "Resource monitoring should collect data");

    info!("Collected {} resource measurements", measurements.len());

    // Phase 5: Health Monitoring
    info!("Phase 5: Testing health monitoring");

    // Perform manual health check
    let health_result = {
        let manager = env.plugin_manager.read().await;
        manager.perform_health_check(&instance_id).await?
    };

    assert!(health_result.healthy, "Plugin should be healthy");
    info!("Health check result: {:?}", health_result);

    // Get health status history
    let health_history = {
        let manager = env.plugin_manager.read().await;
        manager.get_plugin_health_history(&instance_id).await?
    };

    assert!(health_history.is_some(), "Health history should be available");
    info!("Health history available with {} entries",
          health_history.as_ref().map(|h| h.checks.len()).unwrap_or(0));

    // Phase 6: Configuration Updates
    info!("Phase 6: Testing configuration updates");

    // Update resource thresholds
    let mut thresholds = HashMap::new();
    thresholds.insert(
        super::super::resource_monitor::ResourceType::Memory,
        super::super::resource_monitor::ResourceThreshold {
            warning_threshold: 60.0,
            critical_threshold: 85.0,
            grace_period: Duration::from_secs(10),
            auto_throttle: true,
            enable_notifications: true,
        }
    );

    {
        let manager = env.plugin_manager.read().await;
        manager.update_resource_thresholds(&instance_id, thresholds).await?;
    }

    info!("Updated resource thresholds for instance");

    // Phase 7: Dependency Resolution
    info!("Phase 7: Testing dependency resolution");

    // Get dependency resolution for the plugin
    let dependency_result = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&[instance_id.clone()]).await?
    };

    info!("Dependency resolution result: {:?}", dependency_result);
    assert!(dependency_result.success, "Dependency resolution should succeed");

    // Phase 8: Instance Restart
    info!("Phase 8: Testing instance restart");

    // Perform graceful restart
    let mut manager = env.plugin_manager.write().await;
    manager.stop_instance(&instance_id).await?;

    // Wait for stop event
    let stop_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::InstanceStopped { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(10)
    ).await;

    assert!(stop_event.is_some(), "Instance stop event should be received");

    // Restart the instance
    manager.start_instance(&instance_id).await?;
    drop(manager);

    // Wait for restart event
    let restart_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::InstanceStarted { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(10)
    ).await;

    assert!(restart_event.is_some(), "Instance restart event should be received");

    // Verify instance is running again
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    assert_plugin_healthy(&env, &instance_id, Duration::from_secs(10)).await?;

    info!("Successfully restarted instance: {}", instance_id);

    // Phase 9: Cleanup and Shutdown
    info!("Phase 9: Testing cleanup and shutdown");

    // Stop monitoring
    monitor.stop().await;

    // Final resource usage statistics
    let final_usage = {
        let manager = env.plugin_manager.read().await;
        manager.get_resource_usage(Some(&instance_id)).await?
    };

    info!("Final resource usage: {:?}", final_usage);

    // Stop the instance
    let mut manager = env.plugin_manager.write().await;
    manager.stop_instance(&instance_id).await?;
    drop(manager);

    // Verify final state
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Stopped, Duration::from_secs(5)).await?;

    info!("Successfully completed plugin lifecycle test");

    // Collect final statistics
    let final_events = event_collector.get_events().await;
    info!("Total events collected: {}", final_events.len());

    let metrics = env.get_metrics().await;
    info!("Final metrics: {:?}", metrics);

    // Verify overall system health
    let system_health = env.get_system_health().await?;
    info!("System health: {:?}", system_health);

    Ok(())
}

#[tokio::test]
async fn test_plugin_crash_recovery() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin crash recovery test");

    // Create a plugin that will crash
    let mut manifest = create_mock_plugin_manifest("crash-test-plugin", "Crash Test Plugin");
    manifest.metadata.insert("auto_restart".to_string(), serde_json::Value::Bool(true));
    let plugin_id = env.register_mock_plugin(manifest).await?;

    // Create and start instance
    let mut manager = env.plugin_manager.write().await;
    let instance_id = manager.create_instance(&plugin_id, None).await?;
    manager.start_instance(&instance_id).await?;
    drop(manager);

    // Wait for instance to start
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(5)).await?;

    info!("Instance started, simulating crash");

    // Simulate plugin crash
    env.update_mock_instance(&instance_id, |instance| {
        instance.crash();
    }).await;

    // Wait for crash event
    let crash_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::InstanceCrashed { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(10)
    ).await;

    assert!(crash_event.is_some(), "Instance crash event should be received");

    // Verify instance is in error state
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Error(_), Duration::from_secs(5)).await?;

    info!("Plugin crashed, testing automatic recovery");

    // Test automatic recovery (if enabled)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check if plugin was automatically restarted
    let events = event_collector.get_events().await;
    let restart_events: Vec<_> = events.iter()
        .filter(|e| matches!(e, PluginManagerEvent::InstanceStarted { instance_id: ref id, .. } if id == &instance_id))
        .collect();

    if !restart_events.is_empty() {
        info!("Plugin was automatically restarted");
        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    }

    // Test manual recovery
    info!("Testing manual recovery");

    // Create new instance to replace crashed one
    let mut manager = env.plugin_manager.write().await;
    let new_instance_id = manager.create_instance(&plugin_id, None).await?;
    manager.start_instance(&new_instance_id).await?;
    drop(manager);

    // Verify new instance is running
    assert_plugin_state(&env, &new_instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    assert_plugin_healthy(&env, &new_instance_id, Duration::from_secs(10)).await?;

    info!("Manual recovery successful, new instance: {}", new_instance_id);

    // Cleanup
    let mut manager = env.plugin_manager.write().await;
    manager.stop_instance(&new_instance_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_plugin_dependency_resolution() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin dependency resolution test");

    // Get the dependency resolver
    let dependency_resolver = {
        let manager = env.plugin_manager.read().await;
        manager.get_dependency_resolver().await
    };

    // Test dependency graph creation
    let plugins = env.list_plugins().await?;
    let plugin_ids: Vec<String> = plugins.iter().map(|p| p.manifest.id.clone()).collect();

    info!("Found {} plugins for dependency testing", plugin_ids.len());

    // Resolve dependencies for all plugins
    let resolution_result = {
        let manager = env.plugin_manager.read().await;
        manager.resolve_plugin_dependencies(&plugin_ids).await?
    };

    assert!(resolution_result.success, "Dependency resolution should succeed");
    info!("Dependency resolution successful");

    // Verify startup order
    assert!(!resolution_result.startup_order.is_empty(), "Startup order should be calculated");
    info!("Calculated startup order: {:?}", resolution_result.startup_order);

    // Test instance creation with dependencies
    info!("Testing instance creation with dependency resolution");

    // Create instances in dependency order
    let mut instance_ids = Vec::new();
    for plugin_id in &resolution_result.startup_order {
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(plugin_id, None).await?;
        instance_ids.push(instance_id);
        drop(manager);
        info!("Created instance for plugin: {}", plugin_id);
    }

    // Start instances in dependency order
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        manager.start_instance(instance_id).await?;
        drop(manager);

        // Wait for instance to start
        assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
        info!("Started instance: {}", instance_id);
    }

    // Verify all instances are running
    for instance_id in &instance_ids {
        assert_plugin_healthy(&env, instance_id, Duration::from_secs(5)).await?;
    }

    info!("All instances started successfully with dependencies resolved");

    // Test dependency violation handling
    info!("Testing dependency violation handling");

    // Try to stop a dependency (core plugin)
    if let Some(core_instance) = instance_ids.first() {
        let mut manager = env.plugin_manager.write().await;

        // This might fail due to dependent plugins
        let stop_result = manager.stop_instance(core_instance).await;

        match stop_result {
            Ok(_) => {
                warn!("Core plugin stopped, checking dependent plugins");

                // Check if dependent plugins are affected
                tokio::time::sleep(Duration::from_millis(500)).await;

                for instance_id in instance_ids.iter().skip(1) {
                    let state = {
                        let manager = manager.read().await;
                        manager.get_all_instance_states().await.ok()
                            .and_then(|s| s.get(instance_id).cloned())
                    };

                    if let Some(current_state) = state {
                        if matches!(current_state, PluginInstanceState::Error(_)) {
                            info!("Dependent plugin correctly entered error state: {}", instance_id);
                        }
                    }
                }

                // Restart core plugin
                manager.start_instance(core_instance).await?;
            }
            Err(e) => {
                info!("Core plugin stop prevented as expected: {}", e);
            }
        }
    }

    // Cleanup
    info!("Cleaning up dependency test instances");
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_lifecycle_automation() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin lifecycle automation test");

    // Create automation rule for health-based restart
    let automation_rule = super::super::automation_engine::AutomationRule {
        rule_id: "health-restart-rule".to_string(),
        name: "Health-Based Restart Rule".to_string(),
        description: "Automatically restart unhealthy plugins".to_string(),
        enabled: true,
        trigger_conditions: vec![
            super::super::automation_engine::TriggerCondition {
                event_type: "health_status_change".to_string(),
                field: "status".to_string(),
                operator: super::super::lifecycle_policy::ComparisonOperator::Equals,
                value: serde_json::Value::String("Unhealthy".to_string()),
            },
        ],
        actions: vec![
            super::super::automation_engine::AutomationAction {
                action_type: "restart_instance".to_string(),
                parameters: HashMap::from([
                    ("delay_ms".to_string(), serde_json::Value::Number(1000.into())),
                ]),
            },
        ],
        cooldown_period: Duration::from_secs(30),
        max_executions_per_hour: 10,
        priority: 100,
        metadata: HashMap::new(),
    };

    // Add automation rule
    {
        let manager = env.plugin_manager.read().await;
        manager.add_automation_rule(automation_rule).await?;
    }

    info!("Added automation rule for health-based restart");

    // Create test plugin
    let manifest = create_mock_plugin_manifest("automation-test-plugin", "Automation Test Plugin");
    let plugin_id = env.register_mock_plugin(manifest).await?;

    // Create and start instance
    let mut manager = env.plugin_manager.write().await;
    let instance_id = manager.create_instance(&plugin_id, None).await?;
    manager.start_instance(&instance_id).await?;
    drop(manager);

    // Wait for instance to start
    assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(5)).await?;

    info!("Instance started, triggering unhealthy state");

    // Simulate unhealthy state
    env.update_mock_instance(&instance_id, |instance| {
        instance.health_status = PluginHealthStatus::Unhealthy;
        instance.log_event("BECAME_UNHEALTHY").await;
    }).await;

    // Wait for automation to trigger
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check if automation was triggered
    let events = event_collector.get_events().await;
    let restart_events: Vec<_> = events.iter()
        .filter(|e| matches!(e, PluginManagerEvent::InstanceStarted { instance_id: ref id, .. } if id == &instance_id))
        .collect();

    if restart_events.len() > 1 {
        info!("Automation rule successfully triggered instance restart");

        // Verify instance is running again
        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    }

    // Test policy evaluation
    info!("Testing lifecycle policy evaluation");

    // Create a policy that prevents stopping critical plugins
    let policy = super::super::lifecycle_policy::LifecyclePolicy {
        policy_id: "critical-plugin-protection".to_string(),
        name: "Critical Plugin Protection".to_string(),
        description: "Prevent stopping critical plugins".to_string(),
        enabled: true,
        conditions: vec![
            super::super::lifecycle_policy::PolicyCondition {
                name: "is_critical_plugin".to_string(),
                field: "plugin.category".to_string(),
                operator: super::super::lifecycle_policy::ComparisonOperator::Equals,
                value: serde_json::Value::String("Core".to_string()),
            },
        ],
        actions: vec![
            super::super::lifecycle_policy::PolicyAction {
                action_type: super::super::lifecycle_policy::PolicyActionType::Deny,
                parameters: HashMap::new(),
            },
        ],
        priority: 1000,
        metadata: HashMap::new(),
    };

    // Add policy
    {
        let manager = env.plugin_manager.read().await;
        manager.add_lifecycle_policy(policy).await?;
    }

    info!("Added lifecycle policy for critical plugin protection");

    // Cleanup
    let mut manager = env.plugin_manager.write().await;
    manager.stop_instance(&instance_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_plugin_configuration_management() -> PluginResult<()> {
    let env = TestEnvironment::new().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin configuration management test");

    // Test initial configuration
    let initial_config = {
        let manager = env.plugin_manager.read().await;
        manager.get_config().await?
    };

    info!("Initial configuration loaded");

    // Test configuration updates
    let mut updated_config = initial_config.clone();
    updated_config.lifecycle.auto_start = false;
    updated_config.health_monitoring.check_interval = Duration::from_secs(5);

    {
        let mut manager = env.plugin_manager.write().await;
        manager.update_config(updated_config.clone()).await?;
    }

    info!("Configuration updated successfully");

    // Verify configuration changes
    let current_config = {
        let manager = env.plugin_manager.read().await;
        manager.get_config().await?
    };

    assert!(!current_config.lifecycle.auto_start, "Auto-start should be disabled");
    assert_eq!(current_config.health_monitoring.check_interval, Duration::from_secs(5),
               "Health check interval should be updated");

    info!("Configuration changes verified");

    // Test configuration validation
    let mut invalid_config = current_config.clone();
    invalid_config.health_monitoring.check_interval = Duration::from_millis(100); // Too short

    {
        let mut manager = env.plugin_manager.write().await;
        let validation_result = manager.validate_config(&invalid_config).await;
        assert!(validation_result.is_err(), "Invalid configuration should be rejected");
    }

    info!("Configuration validation working correctly");

    // Test plugin-specific configuration
    info!("Testing plugin-specific configuration");

    // Create a plugin with custom configuration
    let mut manifest = create_mock_plugin_manifest("config-test-plugin", "Configuration Test Plugin");
    manifest.metadata.insert("custom_config".to_string(), serde_json::Value::Object(
        serde_json::Map::from([
            ("max_connections".to_string(), serde_json::Value::Number(100.into())),
            ("timeout_ms".to_string(), serde_json::Value::Number(5000.into())),
            ("enable_logging".to_string(), serde_json::Value::Bool(true)),
        ])
    ));

    let plugin_id = env.register_mock_plugin(manifest).await?;

    // Create instance with custom configuration
    let instance_config = super::super::manager::PluginInstanceConfig {
        instance_config: super::super::instance::PluginInstanceConfig {
            auto_restart: true,
            dependencies: Vec::new(),
            startup_priority: 50,
            health_check_config: None,
        },
    };

    let mut manager = env.plugin_manager.write().await;
    let instance_id = manager.create_instance(&plugin_id, Some(instance_config)).await?;
    drop(manager);

    info!("Created instance with custom configuration: {}", instance_id);

    // Verify instance was created with custom configuration
    let plugins = env.list_plugins().await?;
    let test_plugin = plugins.iter().find(|p| p.manifest.id == plugin_id);
    assert!(test_plugin.is_some(), "Test plugin should be found");

    if let Some(plugin) = test_plugin {
        let custom_config = plugin.manifest.metadata.get("custom_config");
        assert!(custom_config.is_some(), "Custom configuration should be preserved");
    }

    // Cleanup
    let mut manager = env.plugin_manager.write().await;
    manager.stop_instance(&instance_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_plugin_system_metrics() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;

    info!("Starting plugin system metrics test");

    // Get initial metrics
    let initial_metrics = env.get_metrics().await;
    info!("Initial metrics: {:?}", initial_metrics);

    // Create and start multiple instances
    let mut instance_ids = Vec::new();
    let plugins = env.list_plugins().await?;

    for plugin in plugins.iter().take(3) {
        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin.manifest.id, None).await?;
        manager.start_instance(&instance_id).await?;
        instance_ids.push(instance_id);
        drop(manager);
    }

    // Wait for instances to start
    for instance_id in &instance_ids {
        assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    }

    // Get updated metrics
    let updated_metrics = env.get_metrics().await;
    info!("Updated metrics after starting instances: {:?}", updated_metrics);

    // Verify metrics changes
    assert!(updated_metrics.total_monitored_instances > initial_metrics.total_monitored_instances,
            "Monitored instances count should increase");

    // Test resource usage aggregation
    let aggregated_usage = {
        let manager = env.plugin_manager.read().await;
        manager.get_aggregated_resource_usage().await?
    };

    info!("Aggregated resource usage: {:?}", aggregated_usage);
    assert!(!aggregated_usage.is_empty(), "Aggregated usage should contain data");

    // Test system resource info
    let system_info = {
        let manager = env.plugin_manager.read().await;
        manager.get_system_resource_info().await?
    };

    info!("System resource info: {:?}", system_info);

    // Test lifecycle analytics
    let analytics = {
        let manager = env.plugin_manager.read().await;
        manager.get_lifecycle_analytics().await?
    };

    info!("Lifecycle analytics: {:?}", analytics);

    // Test monitoring statistics
    let monitoring_stats = {
        let manager = env.plugin_manager.read().await;
        manager.get_monitoring_statistics().await?
    };

    info!("Monitoring statistics: {:?}", monitoring_stats);

    // Cleanup instances
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        manager.stop_instance(instance_id).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_error_scenarios() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting plugin error scenarios test");

    // Test 1: Invalid plugin ID
    info!("Testing invalid plugin ID handling");
    let mut manager = env.plugin_manager.write().await;
    let invalid_result = manager.create_instance("non-existent-plugin", None).await;
    assert!(invalid_result.is_err(), "Creating instance for non-existent plugin should fail");
    drop(manager);

    // Test 2: Resource quota violations
    info!("Testing resource quota violations");

    // Create a plugin with very low resource limits
    let mut manifest = create_mock_plugin_manifest("resource-test-plugin", "Resource Test Plugin");
    manifest.resource_limits.max_memory_bytes = Some(10 * 1024 * 1024); // 10MB
    manifest.resource_limits.max_cpu_percentage = Some(5.0); // 5% CPU

    let plugin_id = env.register_mock_plugin(manifest).await?;

    let mut manager = env.plugin_manager.write().await;
    let instance_id = manager.create_instance(&plugin_id, None).await?;
    manager.start_instance(&instance_id).await?;
    drop(manager);

    // Simulate resource usage exceeding limits
    env.update_mock_instance(&instance_id, |instance| {
        instance.simulate_resource_usage(50, 80.0).await; // 50MB, 80% CPU
    }).await;

    // Wait for resource violation event
    let violation_event = event_collector.wait_for_event(
        |e| matches!(e, PluginManagerEvent::ResourceViolation { instance_id: ref id, .. } if id == &instance_id),
        Duration::from_secs(10)
    ).await;

    if let Some(event) = violation_event {
        info!("Resource violation correctly detected: {:?}", event);
    }

    // Test 3: Security violations
    info!("Testing security violations");

    // Create a plugin with restricted capabilities
    let mut restricted_manifest = create_mock_plugin_manifest("restricted-plugin", "Restricted Plugin");
    restricted_manifest.security_level = SecurityLevel::Strict;
    restricted_manifest.capabilities = vec![]; // No capabilities

    let restricted_plugin_id = env.register_mock_plugin(restricted_manifest).await?;

    // Test 4: Instance timeout scenarios
    info!("Testing instance timeout scenarios");

    // Create a plugin that hangs
    let mut hanging_manifest = create_mock_plugin_manifest("hanging-plugin", "Hanging Plugin");
    hanging_manifest.metadata.insert("hang_on_start".to_string(), serde_json::Value::Bool(true));

    let hanging_plugin_id = env.register_mock_plugin(hanging_manifest).await?;

    let mut manager = env.plugin_manager.write().await;
    let hanging_instance_id = manager.create_instance(&hanging_plugin_id, None).await;

    match hanging_instance_id {
        Ok(instance_id) => {
            // Try to start hanging instance
            let start_result = manager.start_instance(&instance_id).await;

            // This might timeout or fail
            match start_result {
                Ok(_) => {
                    info!("Hanging instance started, testing timeout handling");

                    // Wait for potential timeout
                    tokio::time::sleep(Duration::from_secs(5)).await;

                    // Check if instance is in error state due to timeout
                    let state = {
                        let manager = manager.read().await;
                        manager.get_all_instance_states().await.ok()
                            .and_then(|s| s.get(&instance_id).cloned())
                    };

                    if let Some(current_state) = state {
                        if matches!(current_state, PluginInstanceState::Error(_)) {
                            info!("Instance correctly entered error state due to timeout");
                        }
                    }

                    // Cleanup
                    let _ = manager.stop_instance(&instance_id).await;
                }
                Err(e) => {
                    info!("Hanging instance start failed as expected: {}", e);
                }
            }
        }
        Err(e) => {
            info!("Failed to create hanging instance: {}", e);
        }
    }

    // Test 5: Concurrent operation conflicts
    info!("Testing concurrent operation conflicts");

    // Create an instance and try multiple conflicting operations
    let mut manager = env.plugin_manager.write().await;
    let conflict_instance_id = manager.create_instance(&plugin_id, None).await?;
    drop(manager);

    // Try to start and stop the same instance concurrently
    let manager_clone = env.plugin_manager.clone();
    let instance_id_clone = conflict_instance_id.clone();

    let start_handle = tokio::spawn(async move {
        let mut manager = manager_clone.write().await;
        manager.start_instance(&instance_id_clone).await
    });

    let manager_clone = env.plugin_manager.clone();
    let instance_id_clone = conflict_instance_id.clone();

    let stop_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut manager = manager_clone.write().await;
        manager.stop_instance(&instance_id_clone).await
    });

    let (start_result, stop_result) = tokio::join!(start_handle, stop_handle);

    info!("Concurrent operations results: start={:?}, stop={:?}", start_result, stop_result);

    // At least one operation should succeed or fail gracefully
    assert!(start_result.is_ok() || stop_result.is_ok(),
            "At least one concurrent operation should complete");

    // Cleanup
    if let Ok(Ok(_)) = start_result {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(&conflict_instance_id).await;
    }

    // Collect final error statistics
    let final_metrics = env.get_metrics().await;
    info!("Final error metrics: {:?}", final_metrics);

    Ok(())
}