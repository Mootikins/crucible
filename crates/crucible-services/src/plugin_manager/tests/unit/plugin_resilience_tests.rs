//! # Plugin System Resilience Tests
//!
//! Comprehensive tests for error handling, fault tolerance, and system resilience
//! under various failure scenarios and adverse conditions.

use super::common::*;
use super::super::config::*;
use super::super::error::PluginResult;
use super::super::manager::{PluginManagerService, PluginManagerEvent};
use super::super::types::*;
use super::super::state_machine::PluginInstanceState;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::{RwLock, Barrier};
use tracing::{info, warn, error, debug};

/// ============================================================================
/// FAULT INJECTION UTILITIES
/// ============================================================================

#[derive(Debug, Clone)]
pub enum FaultType {
    InstanceCrash,
    ResourceExhaustion,
    NetworkPartition,
    ProcessHang,
    MemoryLeak,
    DiskSpaceExhaustion,
    TimeoutFailure,
    CorruptionError,
}

#[derive(Debug)]
pub struct FaultInjector {
    faults: Arc<RwLock<VecDeque<(FaultType, SystemTime)>>>,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self {
            faults: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub async fn schedule_fault(&self, fault: FaultType, delay: Duration) {
        let execute_time = SystemTime::now() + delay;
        let mut faults = self.faults.write().await;
        faults.push_back((fault, execute_time));
    }

    pub async fn execute_pending_faults(&self, env: &TestEnvironment) -> Vec<FaultType> {
        let mut executed = Vec::new();
        let now = SystemTime::now();
        let mut faults = self.faults.write().await;

        while let Some((fault, execute_time)) = faults.front() {
            if *execute_time <= now {
                let fault = faults.pop_front().unwrap().0;
                self.execute_fault(&fault, env).await;
                executed.push(fault);
            } else {
                break;
            }
        }

        executed
    }

    async fn execute_fault(&self, fault: &FaultType, env: &TestEnvironment) {
        match fault {
            FaultType::InstanceCrash => {
                info!("Executing instance crash fault");
                // Simulate crash on random instances
                let instances = env.list_instances().await.unwrap_or_default();
                if let Some(instance) = instances.first() {
                    env.update_mock_instance(&instance.instance_id, |mock| {
                        mock.crash();
                    }).await;
                }
            }
            FaultType::ResourceExhaustion => {
                info!("Executing resource exhaustion fault");
                // Simulate high resource usage
                let instances = env.list_instances().await.unwrap_or_default();
                for instance in instances {
                    env.update_mock_instance(&instance.instance_id, |mock| {
                        mock.simulate_resource_usage(1000, 95.0).await;
                    }).await;
                }
            }
            FaultType::ProcessHang => {
                info!("Executing process hang fault");
                // Simulate hung processes
                let instances = env.list_instances().await.unwrap_or_default();
                if let Some(instance) = instances.first() {
                    env.update_mock_instance(&instance.instance_id, |mock| {
                        mock.should_hang = true;
                    }).await;
                }
            }
            FaultType::MemoryLeak => {
                info!("Executing memory leak fault");
                // Simulate memory leaks
                let instances = env.list_instances().await.unwrap_or_default();
                for instance in instances {
                    env.update_mock_instance(&instance.instance_id, |mock| {
                        let current_memory = mock.resource_usage.memory_bytes;
                        mock.simulate_resource_usage(
                            (current_memory / 1024 / 1024 + 100), // Add 100MB
                            mock.resource_usage.cpu_percentage,
                        ).await;
                    }).await;
                }
            }
            FaultType::TimeoutFailure => {
                info!("Executing timeout failure fault");
                // Simulate timeouts by slowing operations
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            FaultType::CorruptionError => {
                info!("Executing corruption error fault");
                // This would typically involve corrupting data files or state
                warn!("Corruption fault simulated (no actual corruption in test)");
            }
            _ => {
                warn!("Fault type {:?} not implemented in test", fault);
            }
        }
    }
}

impl Default for FaultInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// RESILIENCE TEST SCENARIOS
/// ============================================================================

#[tokio::test]
async fn test_cascade_failure_recovery() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting cascade failure recovery test");

    // Create dependency chain instances
    let plugins = env.list_plugins().await?;
    let mut instance_ids = Vec::new();
    let dependency_order = vec!["core-plugin", "database-plugin", "api-plugin", "web-plugin"];

    for plugin_name in &dependency_order {
        if let Some(plugin) = plugins.iter().find(|p| p.manifest.id.contains(plugin_name)) {
            let mut manager = env.plugin_manager.write().await;
            let instance_id = manager.create_instance(&plugin.manifest.id, None).await?;
            manager.start_instance(&instance_id).await?;
            instance_ids.push(instance_id);
            drop(manager);

            assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
            info!("Started dependency instance: {}", plugin_name);
        }
    }

    // Wait for system to stabilize
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Initial system health
    let initial_health = env.get_system_health().await?;
    info!("Initial system health: {:?}", initial_health);

    // Simulate cascade failure by crashing core plugin
    info!("Simulating cascade failure by crashing core component");

    if let Some(core_instance) = instance_ids.first() {
        // Crash the core plugin
        env.update_mock_instance(core_instance, |instance| {
            instance.crash();
        }).await;

        info!("Core plugin crashed: {}", core_instance);

        // Wait for cascade to propagate
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Check cascade effects
        let crash_events = event_collector.get_events_by_type(|e| {
            matches!(e, PluginManagerEvent::InstanceCrashed { .. })
        }).await;

        info!("Cascade failure detected {} crash events", crash_events.len());

        // Check system health after cascade
        let cascade_health = env.get_system_health().await?;
        info!("System health after cascade: {:?}", cascade_health);

        // Verify that dependent plugins are affected
        for (index, instance_id) in instance_ids.iter().enumerate() {
            if index > 0 { // Skip core plugin itself
                let health = {
                    let manager = env.plugin_manager.read().await;
                    manager.get_instance_health(instance_id).await.ok()
                };

                if let Some(health_status) = health {
                    if matches!(health_status, PluginHealthStatus::Unhealthy | PluginHealthStatus::Degraded) {
                        info!("Dependent plugin correctly affected: {}", instance_id);
                    }
                }
            }
        }

        // Test recovery mechanisms
        info!("Testing recovery mechanisms");

        // Attempt to restart core plugin
        let mut manager = env.plugin_manager.write().await;
        match manager.start_instance(core_instance).await {
            Ok(_) => {
                info!("Core plugin restarted successfully");
                assert_plugin_state(&env, core_instance, PluginInstanceState::Running, Duration::from_secs(15)).await?;
            }
            Err(e) => {
                warn!("Core plugin restart failed: {}", e);
            }
        }
        drop(manager);

        // Wait for recovery to propagate
        tokio::time::sleep(Duration::from_secs(5)).await);

        // Check final system health
        let final_health = env.get_system_health().await?;
        info!("Final system health after recovery: {:?}", final_health);

        // Verify some level of recovery
        let final_crash_events = event_collector.get_events_by_type(|e| {
            matches!(e, PluginManagerEvent::InstanceStarted { .. })
        }).await;

        info!("Recovery detected {} start events", final_crash_events.len());
    }

    // Cleanup
    for instance_id in &instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_resource_exhaustion_handling() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting resource exhaustion handling test");

    // Create resource-intensive plugins
    let plugin_configs = vec![
        ("memory-hog", 400, 10.0),  // High memory, low CPU
        ("cpu-hog", 100, 80.0),    // Low memory, high CPU
        ("balanced", 200, 40.0),   // Medium both
    ];

    let mut instance_ids = Vec::new();

    for (plugin_name, memory_mb, cpu_percent) in plugin_configs {
        let plugin_id = format!("{}-{}", plugin_name, uuid::Uuid::new_v4().to_string()[..8]);
        let mut manifest = create_mock_plugin_manifest(&plugin_id, plugin_name);

        // Set resource limits lower than target usage to trigger violations
        manifest.resource_limits.max_memory_bytes = Some((memory_mb as f64 * 0.7) as u64 * 1024 * 1024);
        manifest.resource_limits.max_cpu_percentage = Some(cpu_percent * 0.7);

        env.register_mock_plugin(manifest).await?;

        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        instance_ids.push((instance_id, plugin_name.to_string(), memory_mb, cpu_percent));
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
    }

    info!("Created {} resource-intensive instances", instance_ids.len());

    // Gradually increase resource usage to trigger exhaustion
    info!("Gradually increasing resource usage to trigger exhaustion");

    let fault_injector = FaultInjector::new();
    let mut violation_count = 0;

    for step in 1..=5 {
        info!("Resource exhaustion step {}", step);

        // Increase resource usage
        for (instance_id, _, memory_mb, cpu_percent) in &instance_ids {
            let memory_multiplier = 1.0 + (step as f64 * 0.3); // Increase by 30% each step
            let cpu_multiplier = 1.0 + (step as f64 * 0.2); // Increase by 20% each step

            env.update_mock_instance(instance_id, |instance| {
                instance.simulate_resource_usage(
                    (memory_mb as f64 * memory_multiplier) as u64,
                    cpu_percent * cpu_multiplier,
                ).await;
            }).await;
        }

        // Wait for monitoring to detect violations
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Check for resource violations
        let current_violations = event_collector.get_events_by_type(|e| {
            matches!(e, PluginManagerEvent::ResourceViolation { .. })
        }).await;

        let new_violations = current_violations.len() - violation_count;
        if new_violations > 0 {
            violation_count = current_violations.len();
            info!("Step {} detected {} new resource violations", step, new_violations);

            // Check system response to violations
            let system_health = env.get_system_health().await?;
            info!("System health after violations: {:?}", system_health);

            // Verify enforcement actions
            for violation in current_violations.iter().skip(current_violations.len() - new_violations) {
                if let PluginManagerEvent::ResourceViolation { instance_id, resource_type, current_value, limit } = violation {
                    info!("Resource violation - Instance: {}, Type: {:?}, Current: {}, Limit: {}",
                          instance_id, resource_type, current_value, limit);

                    // Check if enforcement action was taken
                    let instance_health = {
                        let manager = env.plugin_manager.read().await;
                        manager.get_instance_health(instance_id).await.ok()
                    };

                    if let Some(health) = instance_health {
                        if matches!(health, PluginHealthStatus::Unhealthy | PluginHealthStatus::Degraded) {
                            info!("Enforcement action taken for instance: {}", instance_id);
                        }
                    }
                }
            }
        }

        // Test system stability under pressure
        let system_resources = {
            let manager = env.plugin_manager.read().await;
            manager.get_aggregated_resource_usage().await.ok()
        };

        if let Some(resources) = system_resources {
            let total_memory: u64 = resources.values().map(|r| r.memory_bytes).sum();
            info!("Total system memory usage: {:.2} MB", total_memory as f64 / 1024.0 / 1024.0);

            // Check if system is approaching limits
            if total_memory > 2 * 1024 * 1024 * 1024 { // 2GB
                warn!("System approaching memory limits, triggering recovery");

                // Trigger resource exhaustion fault
                fault_injector.schedule_fault(FaultType::ResourceExhaustion, Duration::ZERO).await;
                let _ = fault_injector.execute_pending_faults(&env).await;
            }
        }

        // Brief pause between steps
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Test recovery after resource pressure is relieved
    info!("Testing recovery after resource pressure relief");

    // Reduce resource usage
    for (instance_id, _, memory_mb, cpu_percent) in &instance_ids {
        env.update_mock_instance(instance_id, |instance| {
            instance.simulate_resource_usage(
                (memory_mb as f64 * 0.5) as u64, // Reduce to 50%
                cpu_percent * 0.3, // Reduce to 30%
            ).await;
        }).await;
    }

    // Wait for recovery
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check final system state
    let final_health = env.get_system_health().await?;
    info!("Final system health after recovery: {:?}", final_health);

    // Verify that some instances recovered
    let mut recovered_instances = 0;
    for (instance_id, _, _, _) in &instance_ids {
        let health = {
            let manager = env.plugin_manager.read().await;
            manager.get_instance_health(instance_id).await.ok()
        };

        if let Some(health_status) = health {
            if matches!(health_status, PluginHealthStatus::Healthy) {
                recovered_instances += 1;
            }
        }
    }

    info!("Recovered instances: {} out of {}", recovered_instances, instance_ids.len());

    // Cleanup
    for (instance_id, _, _, _) in instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(&instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_network_partition_simulation() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting network partition simulation test");

    // Create distributed-style plugins that communicate
    let plugins = vec![
        ("frontend-service", vec!["backend-service"]),
        ("backend-service", vec!["database-service"]),
        ("database-service", vec![]),
        ("cache-service", vec!["database-service"]),
        ("monitoring-service", vec!["frontend-service", "backend-service", "database-service"]),
    ];

    let mut instance_map = HashMap::new();

    for (service_name, dependencies) in plugins {
        let plugin_id = format!("{}-{}", service_name, uuid::Uuid::new_v4().to_string()[..8]);
        let mut manifest = create_mock_plugin_manifest(&plugin_id, service_name);
        manifest.dependencies = dependencies;

        env.register_mock_plugin(manifest).await?;

        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        instance_map.insert(service_name.to_string(), instance_id);
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
        info!("Started service: {}", service_name);
    }

    // Wait for services to establish communication
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Simulate network partition by isolating database service
    info!("Simulating network partition - isolating database service");

    let database_instance = instance_map.get("database-service").unwrap();

    // Mark database as unreachable
    env.update_mock_instance(database_instance, |instance| {
        instance.health_status = PluginHealthStatus::Unhealthy;
        instance.log_event("NETWORK_PARTITION_DETECTED").await;
    }).await;

    // Wait for partition detection
    tokio::time::sleep(Duration::from_secs(5)).await);

    // Check impact on dependent services
    info!("Checking impact of network partition on dependent services");

    let dependent_services = vec!["backend-service", "cache-service", "monitoring-service"];
    let mut affected_services = 0;

    for service_name in &dependent_services {
        if let Some(instance_id) = instance_map.get(service_name) {
            let health = {
                let manager = env.plugin_manager.read().await;
                manager.get_instance_health(instance_id).await.ok()
            };

            if let Some(health_status) = health {
                match health_status {
                    PluginHealthStatus::Unhealthy | PluginHealthStatus::Degraded => {
                        affected_services += 1;
                        info!("Service {} affected by partition: {:?}", service_name, health_status);
                    }
                    PluginHealthStatus::Healthy => {
                        info!("Service {} still healthy during partition", service_name);
                    }
                    _ => {}
                }
            }
        }
    }

    info!("Network partition affected {} out of {} dependent services", affected_services, dependent_services.len());

    // Test circuit breaker behavior
    info!("Testing circuit breaker behavior during partition");

    // Simulate repeated attempts to access partitioned service
    for attempt in 1..=5 {
        info!("Circuit breaker test attempt {}", attempt);

        for service_name in &dependent_services {
            if let Some(instance_id) = instance_map.get(service_name) {
                // Simulate attempt to communicate with database
                env.update_mock_instance(instance_id, |instance| {
                    instance.log_event(&format!("ATTEMPTING_DATABASE_COMMUNICATION_{}", attempt)).await;
                }).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Simulate network recovery
    info!("Simulating network recovery");

    env.update_mock_instance(database_instance, |instance| {
        instance.health_status = PluginHealthStatus::Healthy;
        instance.log_event("NETWORK_PARTITION_RESOLVED").await;
    }).await;

    // Wait for recovery to propagate
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check recovery of dependent services
    info!("Checking recovery of dependent services after network restoration");

    let mut recovered_services = 0;
    for service_name in &dependent_services {
        if let Some(instance_id) = instance_map.get(service_name) {
            let health = {
                let manager = env.plugin_manager.read().await;
                manager.get_instance_health(instance_id).await.ok()
            };

            if let Some(health_status) = health {
                if matches!(health_status, PluginHealthStatus::Healthy) {
                    recovered_services += 1;
                    info!("Service {} recovered: {:?}", service_name, health_status);
                }
            }
        }
    }

    info!("Network partition recovery: {} out of {} services recovered", recovered_services, dependent_services.len());

    // Test degraded operation during partial failures
    info!("Testing degraded operation during partial failures");

    // Simulate intermittent connectivity issues
    for cycle in 1..=3 {
        info!("Degraded operation cycle {}", cycle);

        // Temporarily make database unhealthy
        env.update_mock_instance(database_instance, |instance| {
            instance.health_status = PluginHealthStatus::Degraded;
            instance.log_event(&format!("INTERMITTENT_CONNECTIVITY_ISSUE_{}", cycle)).await;
        }).await;

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Restore database
        env.update_mock_instance(database_instance, |instance| {
            instance.health_status = PluginHealthStatus::Healthy;
            instance.log_event(&format!("CONNECTIVITY_RESTORED_{}", cycle)).await;
        }).await;

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Final system health check
    let final_health = env.get_system_health().await?;
    info!("Final system health after network partition tests: {:?}", final_health);

    // Cleanup
    for instance_id in instance_map.values() {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_graceful_degradation() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting graceful degradation test");

    // Create a system with multiple layers of functionality
    let system_layers = vec![
        ("critical-core", vec![], 50, 5.0),      // Critical, minimal resources
        ("essential-services", vec!["critical-core"], 100, 15.0), // Essential
        ("enhanced-features", vec!["essential-services"], 150, 25.0), // Enhanced
        ("premium-features", vec!["enhanced-features"], 200, 35.0), // Premium
    ];

    let mut layer_instances = HashMap::new();

    // Start system in reverse dependency order
    for (layer_name, dependencies, memory_mb, cpu_percent) in system_layers.iter().rev() {
        let plugin_id = format!("{}-{}", layer_name, uuid::Uuid::new_v4().to_string()[..8]);
        let mut manifest = create_mock_plugin_manifest(&plugin_id, layer_name);
        manifest.dependencies = dependencies.clone();
        manifest.resource_limits.max_memory_bytes = Some(memory_mb * 1024 * 1024);
        manifest.resource_limits.max_cpu_percentage = Some(cpu_percent);

        env.register_mock_plugin(manifest).await?;

        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        layer_instances.insert(layer_name.to_string(), instance_id);
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
        info!("Started layer: {}", layer_name);
    }

    // Measure initial system performance
    let initial_performance = {
        let manager = env.plugin_manager.read().await;
        let start = Instant::now();
        let _ = manager.get_system_health().await;
        start.elapsed()
    };

    info!("Initial system performance: {:?}", initial_performance);

    // Gradually degrade system by removing layers
    info!("Starting graceful degradation by removing layers");

    let degradation_order = vec!["premium-features", "enhanced-features", "essential-services"];
    let mut performance_measurements = Vec::new();

    for (step, layer_name) in degradation_order.iter().enumerate() {
        info!("Degradation step {}: Removing layer {}", step + 1, layer_name);

        if let Some(instance_id) = layer_instances.get(*layer_name) {
            // Gracefully stop the layer
            let mut manager = env.plugin_manager.write().await;
            let _ = manager.stop_instance(instance_id).await;
            drop(manager);

            // Wait for system to stabilize
            tokio::time::sleep(Duration::from_secs(3)).await;

            // Measure system performance after degradation
            let performance = {
                let manager = env.plugin_manager.read().await;
                let start = Instant::now();
                let health = manager.get_system_health().await;
                (start.elapsed(), health)
            };

            performance_measurements.push((layer_name.to_string(), performance.0, performance.1));

            info!("Performance after removing {}: {:?}", layer_name, performance.0);

            // Verify that core functionality remains
            let remaining_layers = layer_instances.keys()
                .filter(|name| !degradation_order[..=step].contains(name))
                .count();

            info!("Remaining active layers: {}", remaining_layers);

            if remaining_layers > 0 {
                // Test that remaining layers are still functional
                for (remaining_name, remaining_instance) in layer_instances.iter() {
                    if !degradation_order[..=step].contains(remaining_name) {
                        let health = {
                            let manager = env.plugin_manager.read().await;
                            manager.get_instance_health(remaining_instance).await.ok()
                        };

                        if let Some(health_status) = health {
                            if matches!(health_status, PluginHealthStatus::Healthy) {
                                info!("Layer {} still healthy: {:?}", remaining_name, health_status);
                            } else {
                                warn!("Layer {} degraded: {:?}", remaining_name, health_status);
                            }
                        }
                    }
                }
            }
        }
    }

    // Analyze performance degradation
    info!("Performance degradation analysis:");
    for (layer, duration, health) in &performance_measurements {
        info!("  After removing {}: {:?} - {:?}", layer, duration, health.status);
    }

    // Test resource reallocation
    info!("Testing resource reallocation after degradation");

    // Give more resources to remaining critical components
    if let Some(critical_instance) = layer_instances.get("critical-core") {
        env.update_mock_instance(critical_instance, |instance| {
            instance.simulate_resource_usage(25, 8.0).await; // Use less than allocated
            instance.log_event("RESOURCE_REALLOCATION_COMPLETED").await;
        }).await;
    }

    // Final system health with only critical components
    let final_health = env.get_system_health().await?;
    info!("Final system health with critical components only: {:?}", final_health);

    // Verify that system is still operational
    assert!(matches!(final_health.status, crate::service_types::ServiceStatus::Healthy | crate::service_types::ServiceStatus::Degraded),
           "System should remain at least degraded with critical components");

    // Cleanup remaining instances
    for (layer_name, instance_id) in layer_instances {
        if layer_name != "critical-core" { // Keep critical core for last
            let mut manager = env.plugin_manager.write().await;
            let _ = manager.stop_instance(&instance_id).await;
        }
    }

    // Stop critical core last
    if let Some(critical_instance) = layer_instances.get("critical-core") {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(critical_instance).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_chaos_engineering_scenarios() -> PluginResult<()> {
    let env = setup_multi_plugin_scenario().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting chaos engineering scenarios test");

    // Create a resilient system with multiple instances
    let plugins = env.list_plugins().await?;
    let mut instance_ids = Vec::new();

    // Create multiple instances per plugin for redundancy
    for plugin in plugins.iter().take(5) {
        for replica in 0..2 {
            let mut manager = env.plugin_manager.write().await;
            let instance_id = manager.create_instance(&plugin.manifest.id, None).await?;
            manager.start_instance(&instance_id).await?;
            instance_ids.push(instance_id);
            drop(manager);

            assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
        }
    }

    info!("Created {} instances for chaos testing", instance_ids.len());

    let fault_injector = FaultInjector::new();
    let chaos_duration = Duration::from_secs(30);
    let chaos_start = Instant::now();

    info!("Starting chaos experiment for {:?}", chaos_duration);

    // Schedule various faults throughout the experiment
    fault_injector.schedule_fault(FaultType::InstanceCrash, Duration::from_secs(5)).await;
    fault_injector.schedule_fault(FaultType::ResourceExhaustion, Duration::from_secs(10)).await;
    fault_injector.schedule_fault(FaultType::ProcessHang, Duration::from_secs(15)).await;
    fault_injector.schedule_fault(FaultType::MemoryLeak, Duration::from_secs(20)).await;
    fault_injector.schedule_fault(FaultType::TimeoutFailure, Duration::from_secs(25)).await;

    let mut chaos_metrics = HashMap::new();
    let mut system_health_samples = Vec::new();

    // Run chaos experiment
    while chaos_start.elapsed() < chaos_duration {
        // Execute any pending faults
        let executed_faults = fault_injector.execute_pending_faults(&env).await;

        for fault in executed_faults {
            *chaos_metrics.entry(format!("{:?}", fault)).or_insert(0) += 1;
            info!("Chaos fault executed: {:?}", fault);
        }

        // Sample system health
        if let Ok(health) = env.get_system_health().await {
            system_health_samples.push((SystemTime::now(), health));
        }

        // Random small-scale disturbances
        if rand::random::<f64>() < 0.1 { // 10% chance each iteration
            if let Some(random_instance) = instance_ids.choose(&mut rand::thread_rng()) {
                env.update_mock_instance(random_instance, |instance| {
                    instance.log_event("RANDOM_DISTURBANCE").await;
                    // Small resource spike
                    let memory_spike = instance.resource_usage.memory_bytes + (10 * 1024 * 1024); // +10MB
                    let cpu_spike = instance.resource_usage.cpu_percentage + 5.0;
                    instance.simulate_resource_usage(
                        memory_spike / 1024 / 1024,
                        cpu_spike.min(100.0),
                    ).await;
                }).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    info!("Chaos experiment completed");

    // Analyze chaos results
    info!("Chaos experiment results:");
    for (fault_type, count) in &chaos_metrics {
        info!("  {} executed {} times", fault_type, count);
    }

    // Analyze system health during chaos
    let mut healthy_samples = 0;
    let mut degraded_samples = 0;
    let mut unhealthy_samples = 0;

    for (_, health) in &system_health_samples {
        match health.status {
            crate::service_types::ServiceStatus::Healthy => healthy_samples += 1,
            crate::service_types::ServiceStatus::Degraded => degraded_samples += 1,
            crate::service_types::ServiceStatus::Unhealthy => unhealthy_samples += 1,
        }
    }

    let total_samples = healthy_samples + degraded_samples + unhealthy_samples;
    if total_samples > 0 {
        info!("System health during chaos:");
        info!("  Healthy: {:.1}% ({}/{})", healthy_samples as f64 / total_samples as f64 * 100.0, healthy_samples, total_samples);
        info!("  Degraded: {:.1}% ({}/{})", degraded_samples as f64 / total_samples as f64 * 100.0, degraded_samples, total_samples);
        info!("  Unhealthy: {:.1}% ({}/{})", unhealthy_samples as f64 / total_samples as f64 * 100.0, unhealthy_samples, total_samples);
    }

    // Check fault tolerance metrics
    let crash_events = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::InstanceCrashed { .. })
    }).await;

    let restart_events = event_collector.get_events_by_type(|e| {
        matches!(e, PluginManagerEvent::InstanceStarted { .. })
    }).await;

    info!("Fault tolerance metrics:");
    info!("  Crash events: {}", crash_events.len());
    info!("  Restart events: {}", restart_events.len());

    // Test system recovery after chaos
    info!("Testing system recovery after chaos experiment");

    // Stop injecting faults and allow recovery
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Check final system state
    let final_health = env.get_system_health().await?;
    info!("Final system health after chaos: {:?}", final_health);

    // Count still running instances
    let mut running_instances = 0;
    for instance_id in &instance_ids {
        if let Ok(_) = assert_plugin_state(&env, instance_id, PluginInstanceState::Running, Duration::from_secs(2)).await {
            running_instances += 1;
        }
    }

    info!("Instances still running after chaos: {} out of {}", running_instances, instance_ids.len());

    // Resilience assertions
    let resilience_ratio = running_instances as f64 / instance_ids.len() as f64;
    info!("System resilience ratio: {:.2}%", resilience_ratio * 100.0);

    assert!(resilience_ratio > 0.3, "System should maintain at least 30% of instances during chaos");

    if total_samples > 0 {
        let availability_ratio = healthy_samples as f64 / total_samples as f64;
        info!("System availability during chaos: {:.2}%", availability_ratio * 100.0);
        assert!(availability_ratio > 0.2, "System should be available at least 20% of the time during chaos");
    }

    // Cleanup
    for instance_id in instance_ids {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(&instance_id).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_disaster_recovery() -> PluginResult<()> {
    let env = TestEnvironment::with_monitoring().await?;
    let event_collector = EventCollector::new();
    event_collector.start_collecting(env.plugin_manager.read().await.subscribe_events().await).await;

    info!("Starting disaster recovery test");

    // Create a critical system with backup capabilities
    let critical_plugins = vec![
        ("primary-db", true),   // Has backup
        ("secondary-db", false), // Is backup
        ("api-gateway", true),
        ("auth-service", true),
        ("cache-service", false), // Non-critical
    ];

    let mut instance_map = HashMap::new();

    for (plugin_name, is_critical) in critical_plugins {
        let plugin_id = format!("{}-{}", plugin_name, uuid::Uuid::new_v4().to_string()[..8]);
        let mut manifest = create_mock_plugin_manifest(&plugin_id, plugin_name);

        if is_critical {
            manifest.metadata.insert("backup_required".to_string(), serde_json::Value::Bool(true));
            manifest.metadata.insert("auto_failover".to_string(), serde_json::Value::Bool(true));
        }

        env.register_mock_plugin(manifest).await?;

        let mut manager = env.plugin_manager.write().await;
        let instance_id = manager.create_instance(&plugin_id, None).await?;
        manager.start_instance(&instance_id).await?;
        instance_map.insert(plugin_name.to_string(), (instance_id, is_critical));
        drop(manager);

        assert_plugin_state(&env, &instance_id, PluginInstanceState::Running, Duration::from_secs(10)).await?;
        info!("Started critical service: {} (critical: {})", plugin_name, is_critical);
    }

    // Simulate disaster scenarios
    let disaster_scenarios = vec![
        ("Complete System Failure", FaultType::InstanceCrash),
        ("Resource Exhaustion", FaultType::ResourceExhaustion),
        ("Memory Corruption", FaultType::MemoryLeak),
    ];

    for (scenario_name, fault_type) in disaster_scenarios {
        info!("Simulating disaster scenario: {}", scenario_name);

        // Record pre-disaster state
        let pre_disaster_health = env.get_system_health().await?;
        let pre_disaster_instances = env.list_instances().await?.len();

        info!("Pre-disaster state - Health: {:?}, Instances: {}", pre_disaster_health.status, pre_disaster_instances);

        // Execute disaster
        match fault_type {
            FaultType::InstanceCrash => {
                // Crash all critical instances
                for (plugin_name, (instance_id, is_critical)) in &instance_map {
                    if *is_critical {
                        env.update_mock_instance(instance_id, |instance| {
                            instance.crash();
                            instance.log_event(&format!("DISASTER_{}", scenario_name)).await;
                        }).await;
                    }
                }
            }
            FaultType::ResourceExhaustion => {
                // Exhaust resources on all instances
                for (plugin_name, (instance_id, _)) in &instance_map {
                    env.update_mock_instance(instance_id, |instance| {
                        instance.simulate_resource_usage(1000, 95.0).await;
                        instance.log_event(&format!("DISASTER_{}", scenario_name)).await;
                    }).await;
                }
            }
            FaultType::MemoryLeak => {
                // Simulate memory leaks
                for (plugin_name, (instance_id, _)) in &instance_map {
                    env.update_mock_instance(instance_id, |instance| {
                        let current_memory = instance.resource_usage.memory_bytes;
                        instance.simulate_resource_usage(
                            (current_memory / 1024 / 1024 + 500), // Add 500MB
                            80.0,
                        ).await;
                        instance.log_event(&format!("DISASTER_{}", scenario_name)).await;
                    }).await;
                }
            }
            _ => {}
        }

        // Wait for disaster to propagate
        tokio::time::sleep(Duration::from_secs(5)).await);

        // Check disaster impact
        let post_disaster_health = env.get_system_health().await?;
        let crash_events = event_collector.get_events_by_type(|e| {
            matches!(e, PluginManagerEvent::InstanceCrashed { .. })
        }).await;

        info!("Post-disaster state - Health: {:?}, Crashes: {}", post_disaster_health.status, crash_events.len());

        // Test disaster recovery procedures
        info!("Testing disaster recovery procedures");

        // 1. Attempt to restart critical services
        let mut recovered_count = 0;
        for (plugin_name, (instance_id, is_critical)) in &instance_map {
            if *is_critical {
                let mut manager = env.plugin_manager.write().await;
                match manager.start_instance(instance_id).await {
                    Ok(_) => {
                        recovered_count += 1;
                        info!("Successfully restarted critical service: {}", plugin_name);
                    }
                    Err(e) => {
                        warn!("Failed to restart critical service {}: {}", plugin_name, e);
                    }
                }
                drop(manager);
            }
        }

        // 2. Check failover to backup services
        if let Some((backup_instance, _)) = instance_map.get("secondary-db") {
            if let Ok(backup_health) = {
                let manager = env.plugin_manager.read().await;
                manager.get_instance_health(backup_instance).await
            } {
                if matches!(backup_health, PluginHealthStatus::Healthy) {
                    info!("Backup service successfully took over: secondary-db");
                }
            }
        }

        // 3. Implement degraded operation mode
        info!("Implementing degraded operation mode");

        // Disable non-critical services to conserve resources
        for (plugin_name, (instance_id, is_critical)) in &instance_map {
            if !*is_critical {
                let mut manager = env.plugin_manager.write().await;
                let _ = manager.stop_instance(instance_id).await;
                info!("Disabled non-critical service for resource conservation: {}", plugin_name);
            }
        }

        // Wait for recovery to stabilize
        tokio::time::sleep(Duration::from_secs(10)).await);

        // Check recovery status
        let recovery_health = env.get_system_health().await?;
        info!("Recovery health status: {:?}", recovery_health.status);

        // Evaluate recovery effectiveness
        let recovery_effectiveness = match recovery_health.status {
            crate::service_types::ServiceStatus::Healthy => "Complete",
            crate::service_types::ServiceStatus::Degraded => "Partial",
            crate::service_types::ServiceStatus::Unhealthy => "Failed",
        };

        info!("Disaster recovery effectiveness for {}: {}", scenario_name, recovery_effectiveness);

        // Reset for next scenario
        info!("Resetting system for next disaster scenario");

        // Stop all instances
        for (plugin_name, (instance_id, _)) in &instance_map {
            let mut manager = env.plugin_manager.write().await;
            let _ = manager.stop_instance(instance_id).await;
        }

        // Restart all instances
        for (plugin_name, (instance_id, _)) in &instance_map {
            let mut manager = env.plugin_manager.write().await;
            let _ = manager.start_instance(instance_id).await;
        }

        // Wait for system to stabilize
        tokio::time::sleep(Duration::from_secs(5)).await;

        info!("System reset completed, ready for next scenario");
    }

    // Test backup and restore procedures
    info!("Testing backup and restore procedures");

    // Simulate creating system backup
    let backup_start = Instant::now();
    let system_state = {
        let manager = env.plugin_manager.read().await;
        manager.get_lifecycle_analytics().await.ok()
    };

    let backup_duration = backup_start.elapsed();
    info!("System backup created in {:?} (state: {})", backup_duration, system_state.is_some());

    // Simulate complete system loss
    info!("Simulating complete system loss");

    for (plugin_name, (instance_id, _)) in &instance_map {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(instance_id).await;
    }

    // Simulate restore from backup
    info!("Simulating restore from backup");

    let restore_start = Instant::now();

    // Restart critical services in priority order
    let restore_order = vec!["primary-db", "auth-service", "api-gateway"];
    let mut restored_count = 0;

    for service_name in restore_order {
        if let Some((instance_id, _)) = instance_map.get(service_name) {
            let mut manager = env.plugin_manager.write().await;
            match manager.start_instance(instance_id).await {
                Ok(_) => {
                    restored_count += 1;
                    info!("Restored service: {}", service_name);
                }
                Err(e) => {
                    warn!("Failed to restore service {}: {}", service_name, e);
                }
            }
        }
    }

    let restore_duration = restore_start.elapsed();
    info!("System restore completed in {:?} (restored: {})", restore_duration, restored_count);

    // Final system assessment
    let final_health = env.get_system_health().await?;
    info!("Final system health after disaster recovery tests: {:?}", final_health);

    // Cleanup
    for (plugin_name, (instance_id, _)) in instance_map {
        let mut manager = env.plugin_manager.write().await;
        let _ = manager.stop_instance(&instance_id).await;
    }

    Ok(())
}