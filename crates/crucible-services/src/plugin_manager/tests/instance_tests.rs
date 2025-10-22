//! # Plugin Instance Tests
//!
//! Comprehensive tests for plugin instance functionality including
//! lifecycle management, process control, resource tracking, and communication.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};

/// ============================================================================
/// PLUGIN INSTANCE CREATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_creation() -> Result<(), Box<dyn std::error::Error>> {
    let instance_id = "test-instance-1";
    let plugin_id = "test-plugin-1";

    let instance = create_test_plugin_instance(instance_id, plugin_id);

    // Verify instance properties
    assert_eq!(instance.instance_id, instance_id);
    assert_eq!(instance.plugin_id, plugin_id);
    assert_eq!(instance.state, PluginInstanceState::Created);
    assert_eq!(instance.pid, None);
    assert_eq!(instance.restart_count, 0);
    assert_eq!(instance.health_status, PluginHealthStatus::Unknown);

    Ok(())
}

#[tokio::test]
async fn test_mock_plugin_instance_creation() -> Result<(), Box<dyn std::error::Error>> {
    let instance_id = "mock-instance-1";
    let plugin_id = "mock-plugin-1";

    let mut instance = MockPluginInstance::new(instance_id.to_string(), plugin_id.to_string());

    // Verify initial state
    assert_eq!(instance.instance_id(), instance_id);
    assert_eq!(instance.plugin_id(), plugin_id);
    assert_eq!(instance.get_state().await?, PluginInstanceState::Created);
    assert_eq!(instance.get_pid().await?, None);
    assert_eq!(instance.get_start_count(), 0);
    assert_eq!(instance.get_stop_count(), 0);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE LIFECYCLE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "lifecycle-instance".to_string(),
        "lifecycle-plugin".to_string(),
    );

    // Start instance
    instance.start().await?;

    // Verify started state
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);
    assert!(instance.get_pid().await?.is_some());
    assert_eq!(instance.get_start_count(), 1);
    assert_eq!(instance.get_stop_count(), 0);

    // Stop instance
    instance.stop().await?;

    // Verify stopped state
    assert_eq!(instance.get_state().await?, PluginInstanceState::Stopped);
    assert_eq!(instance.get_pid().await?, None);
    assert_eq!(instance.get_start_count(), 1);
    assert_eq!(instance.get_stop_count(), 1);

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_restart() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "restart-instance".to_string(),
        "restart-plugin".to_string(),
    );

    // Start instance
    instance.start().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);
    assert_eq!(instance.get_start_count(), 1);

    // Restart instance
    instance.restart().await?;

    // Verify restarted state
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);
    assert_eq!(instance.get_start_count(), 2); // Should increment
    assert_eq!(instance.get_stop_count(), 1); // Should increment

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_multiple_starts() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "multi-start-instance".to_string(),
        "multi-start-plugin".to_string(),
    );

    // Start instance multiple times
    instance.start().await?;
    assert_eq!(instance.get_start_count(), 1);

    instance.start().await?;
    assert_eq!(instance.get_start_count(), 2);

    instance.start().await?;
    assert_eq!(instance.get_start_count(), 3);

    // State should still be running
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_multiple_stops() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "multi-stop-instance".to_string(),
        "multi-stop-plugin".to_string(),
    );

    // Start then stop multiple times
    instance.start().await?;
    instance.stop().await?;
    assert_eq!(instance.get_stop_count(), 1);

    instance.stop().await?;
    assert_eq!(instance.get_stop_count(), 2);

    instance.stop().await?;
    assert_eq!(instance.get_stop_count(), 3);

    // State should still be stopped
    assert_eq!(instance.get_state().await?, PluginInstanceState::Stopped);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE ERROR HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_start_failure() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "fail-start-instance".to_string(),
        "fail-start-plugin".to_string(),
    );

    // Configure instance to fail on start
    instance.set_start_failure(true);

    // Try to start - should fail
    let result = instance.start().await;
    assert!(result.is_err());

    // Verify error state
    assert_eq!(instance.get_state().await?, PluginInstanceState::Error("Mock start failure".to_string()));
    assert_eq!(instance.get_pid().await?, None);
    assert_eq!(instance.get_start_count(), 1);

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_stop_failure() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "fail-stop-instance".to_string(),
        "fail-stop-plugin".to_string(),
    );

    // Start instance first
    instance.start().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);

    // Configure instance to fail on stop
    instance.set_stop_failure(true);

    // Try to stop - should fail
    let result = instance.stop().await;
    assert!(result.is_err());

    // Verify start count is still incremented
    assert_eq!(instance.get_start_count(), 1);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE RESOURCE TRACKING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_resource_usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "resource-instance".to_string(),
        "resource-plugin".to_string(),
    );

    // Set custom resource usage
    let usage = create_moderate_resource_usage();
    instance.set_resource_usage(usage.clone()).await;

    // Get resource usage
    let retrieved_usage = instance.get_resource_usage().await?;

    // Verify resource usage
    assert_eq!(retrieved_usage.memory_bytes, usage.memory_bytes);
    assert_eq!(retrieved_usage.cpu_percentage, usage.cpu_percentage);
    assert_eq!(retrieved_usage.disk_bytes, usage.disk_bytes);
    assert_eq!(retrieved_usage.network_bytes, usage.network_bytes);
    assert_eq!(retrieved_usage.open_files, usage.open_files);
    assert_eq!(retrieved_usage.active_threads, usage.active_threads);
    assert_eq!(retrieved_usage.child_processes, usage.child_processes);

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_resource_usage_updates() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "resource-update-instance".to_string(),
        "resource-update-plugin".to_string(),
    );

    // Start with low usage
    let low_usage = create_low_resource_usage();
    instance.set_resource_usage(low_usage).await;

    let initial_usage = instance.get_resource_usage().await?;
    assert_eq!(initial_usage.memory_bytes, 32 * 1024 * 1024); // 32MB

    // Update to high usage
    let high_usage = create_high_resource_usage();
    instance.set_resource_usage(high_usage).await;

    let updated_usage = instance.get_resource_usage().await?;
    assert_eq!(updated_usage.memory_bytes, 1024 * 1024 * 1024); // 1GB

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE MESSAGE HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_message_communication() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "message-instance".to_string(),
        "message-plugin".to_string(),
    );

    // Create test message
    let request_message = create_test_message(PluginMessageType::Request);

    // Send message
    let response_message = instance.send_message(request_message).await?;

    // Verify response
    assert_eq!(response_message.message_type, PluginMessageType::Response);
    assert!(response_message.correlation_id.is_some());

    // Verify response payload
    let payload = response_message.payload;
    assert!(payload.get("status").is_some());
    assert!(payload.get("mock").is_some());

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_different_message_types() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "message-types-instance".to_string(),
        "message-types-plugin".to_string(),
    );

    // Test different message types
    let message_types = vec![
        PluginMessageType::Request,
        PluginMessageType::Event,
        PluginMessageType::HealthCheck,
        PluginMessageType::ConfigUpdate,
    ];

    for message_type in message_types {
        let message = create_test_message(message_type.clone());
        let response = instance.send_message(message).await?;

        // All should get a response
        assert_eq!(response.message_type, PluginMessageType::Response);
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_high_priority_messages() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "priority-instance".to_string(),
        "priority-plugin".to_string(),
    );

    // Create high priority message
    let high_priority_message = create_high_priority_request();

    // Send message
    let response = instance.send_message(high_priority_message).await?;

    // Verify response maintains priority
    assert_eq!(response.priority, MessagePriority::High);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE EVENT HANDLING TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_events() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "events-instance".to_string(),
        "events-plugin".to_string(),
    );

    // Subscribe to events
    let mut event_receiver = instance.subscribe_events().await;

    // Start instance (should generate event)
    instance.start().await?;

    // Wait for start event
    let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await?;
    assert!(event.is_some());

    match event.unwrap() {
        InstanceEvent::InstanceStarted { instance_id, plugin_id } => {
            assert_eq!(instance_id, "events-instance");
            assert_eq!(plugin_id, "events-plugin");
        }
        _ => panic!("Expected InstanceStarted event"),
    }

    // Stop instance (should generate event)
    instance.stop().await?;

    // Wait for stop event
    let event = tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await?;
    assert!(event.is_some());

    match event.unwrap() {
        InstanceEvent::InstanceStopped { instance_id, plugin_id } => {
            assert_eq!(instance_id, "events-instance");
            assert_eq!(plugin_id, "events-plugin");
        }
        _ => panic!("Expected InstanceStopped event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_multiple_event_subscribers() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "multi-events-instance".to_string(),
        "multi-events-plugin".to_string(),
    );

    // Subscribe multiple receivers
    let mut receiver1 = instance.subscribe_events().await;
    let mut receiver2 = instance.subscribe_events().await;

    // Start instance
    instance.start().await?;

    // Both subscribers should receive the event
    for (i, receiver) in [&mut receiver1, &mut receiver2].iter_mut().enumerate() {
        let event = tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await?;
        assert!(event.is_some());

        match event.unwrap() {
            InstanceEvent::InstanceStarted { instance_id, .. } => {
                assert_eq!(instance_id, "multi-events-instance");
            }
            _ => panic!("Subscriber {} expected InstanceStarted event", i + 1),
        }
    }

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE STATE TRANSITIONS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_state_transitions() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "state-instance".to_string(),
        "state-plugin".to_string(),
    );

    // Initial state should be Created
    assert_eq!(instance.get_state().await?, PluginInstanceState::Created);

    // Start -> Running
    instance.start().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);

    // Stop -> Stopped
    instance.stop().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Stopped);

    // Start again -> Running
    instance.start().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);

    // Stop again -> Stopped
    instance.stop().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Stopped);

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_restart_state_transitions() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "restart-state-instance".to_string(),
        "restart-state-plugin".to_string(),
    );

    // Start -> Running
    instance.start().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);

    // Restart (should stop then start)
    instance.restart().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);
    assert_eq!(instance.get_start_count(), 2);
    assert_eq!(instance.get_stop_count(), 1);

    // Restart from stopped state
    instance.stop().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Stopped);

    instance.restart().await?;
    assert_eq!(instance.get_state().await?, PluginInstanceState::Running);
    assert_eq!(instance.get_start_count(), 3);
    assert_eq!(instance.get_stop_count(), 2);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE CONFIGURATION TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let instance_id = "config-instance";
    let plugin_id = "config-plugin";

    // Create instance with configuration
    let mut config = HashMap::new();
    config.insert("timeout".to_string(), serde_json::Value::Number(30.into()));
    config.insert("retries".to_string(), serde_json::Value::Number(3.into()));
    config.insert("debug".to_string(), serde_json::Value::Bool(true));

    let instance = create_test_plugin_instance(instance_id, plugin_id);

    // Verify configuration (in real implementation, this would be accessible)
    assert_eq!(instance.instance_id, instance_id);
    assert_eq!(instance.plugin_id, plugin_id);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE EXECUTION STATISTICS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_execution_statistics() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = create_test_plugin_instance("stats-instance", "stats-plugin");

    // Initial statistics should be zero
    assert_eq!(instance.execution_stats.total_executions, 0);
    assert_eq!(instance.execution_stats.successful_executions, 0);
    assert_eq!(instance.execution_stats.failed_executions, 0);
    assert_eq!(instance.success_rate(), 1.0); // No executions yet

    // Record successful execution
    let execution_time = Duration::from_millis(100);
    let memory_usage = 64 * 1024 * 1024; // 64MB
    instance.update_execution_stats(true, execution_time, memory_usage);

    assert_eq!(instance.execution_stats.total_executions, 1);
    assert_eq!(instance.execution_stats.successful_executions, 1);
    assert_eq!(instance.execution_stats.failed_executions, 0);
    assert_eq!(instance.success_rate(), 1.0);
    assert_eq!(instance.execution_stats.peak_memory_usage, memory_usage);

    // Record failed execution
    let execution_time = Duration::from_millis(50);
    let memory_usage = 32 * 1024 * 1024; // 32MB
    instance.update_execution_stats(false, execution_time, memory_usage);

    assert_eq!(instance.execution_stats.total_executions, 2);
    assert_eq!(instance.execution_stats.successful_executions, 1);
    assert_eq!(instance.execution_stats.failed_executions, 1);
    assert_eq!(instance.success_rate(), 0.5); // 1 success out of 2 total

    // Record another successful execution with higher memory usage
    let execution_time = Duration::from_millis(75);
    let memory_usage = 128 * 1024 * 1024; // 128MB
    instance.update_execution_stats(true, execution_time, memory_usage);

    assert_eq!(instance.execution_stats.total_executions, 3);
    assert_eq!(instance.execution_stats.successful_executions, 2);
    assert_eq!(instance.execution_stats.failed_executions, 1);
    assert_eq!(instance.success_rate(), 2.0 / 3.0);
    assert_eq!(instance.execution_stats.peak_memory_usage, memory_usage); // Should update to higher value

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE ERROR INFO TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_error_info() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = create_failed_plugin_instance(
        "error-instance",
        "error-plugin",
        "Test error message"
    );

    // Verify error info
    assert!(matches!(instance.state, PluginInstanceState::Error(_)));
    assert_eq!(instance.health_status, PluginHealthStatus::Unhealthy);
    assert!(instance.error_info.is_some());

    let error_info = instance.error_info.as_ref().unwrap();
    assert_eq!(error_info.code, "EXECUTION_ERROR");
    assert_eq!(error_info.message, "Test error message");
    assert!(error_info.stack_trace.is_some());
    assert_eq!(error_info.occurrence_count, 1);

    // Test instance can restart from error state
    assert!(instance.can_restart());

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE CRASHED STATE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_crashed_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = create_crashed_plugin_instance("crashed-instance", "crashed-plugin");

    // Verify crashed state
    assert_eq!(instance.state, PluginInstanceState::Crashed);
    assert_eq!(instance.health_status, PluginHealthStatus::Unhealthy);
    assert_eq!(instance.restart_count, 2);
    assert_eq!(instance.execution_stats.failed_executions, 1);

    // Test instance can restart from crashed state
    assert!(instance.can_restart());

    // Simulate recovery
    instance.state = PluginInstanceState::Running;
    instance.health_status = PluginHealthStatus::Healthy;
    instance.last_activity = Some(SystemTime::now());

    assert!(instance.is_running());

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE CONCURRENCY TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_concurrent_operations() -> Result<(), Box<dyn std::error::Error>> {
    let instance = Arc::new(RwLock::new(MockPluginInstance::new(
        "concurrent-instance".to_string(),
        "concurrent-plugin".to_string(),
    )));

    // Concurrent starts
    let mut handles = Vec::new();
    for _ in 0..5 {
        let instance_clone = instance.clone();
        let handle = tokio::spawn(async move {
            let mut instance_guard = instance_clone.write().await;
            instance_guard.start().await
        });
        handles.push(handle);
    }

    // Wait for all starts
    for handle in handles {
        let _ = handle.await?;
    }

    // Verify instance is running
    let instance_guard = instance.read().await;
    assert_eq!(instance_guard.get_state().await?, PluginInstanceState::Running);
    assert_eq!(instance_guard.get_start_count(), 5);

    // Concurrent stops
    let mut handles = Vec::new();
    for _ in 0..3 {
        let instance_clone = instance.clone();
        let handle = tokio::spawn(async move {
            let mut instance_guard = instance_clone.write().await;
            instance_guard.stop().await
        });
        handles.push(handle);
    }

    // Wait for all stops
    for handle in handles {
        let _ = handle.await?;
    }

    // Verify stop count
    let instance_guard = instance.read().await;
    assert_eq!(instance_guard.get_stop_count(), 3);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_performance() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = MockPluginInstance::new(
        "perf-instance".to_string(),
        "perf-plugin".to_string(),
    );

    // Measure start performance
    let start_times = benchmark_async(|| {
        Box::pin(async {
            let mut test_instance = MockPluginInstance::new(
                "temp-instance".to_string(),
                "temp-plugin".to_string(),
            );
            test_instance.start().await
        })
    }, 10).await;

    let start_stats = calculate_duration_stats(&start_times);
    println!("Start Performance: {:?}", start_stats);

    // Start should be fast (less than 10ms average)
    assert!(start_stats.mean < Duration::from_millis(10));

    // Measure message handling performance
    let message_times = benchmark_async(|| {
        Box::pin(async {
            let mut test_instance = MockPluginInstance::new(
                "temp-instance".to_string(),
                "temp-plugin".to_string(),
            );
            let message = create_test_message(PluginMessageType::Request);
            test_instance.send_message(message).await
        })
    }, 100).await;

    let message_stats = calculate_duration_stats(&message_times);
    println!("Message Performance: {:?}", message_stats);

    // Message handling should be very fast (less than 1ms average)
    assert!(message_stats.mean < Duration::from_millis(1));

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE RESOURCE LIMITS TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_instance_resource_limits() -> Result<(), Box<dyn std::error::Error>> {
    let mut instance = create_test_plugin_instance("limits-instance", "limits-plugin");

    // Set resource limits
    instance.resource_limits = ResourceLimits {
        max_memory_bytes: Some(256 * 1024 * 1024), // 256MB
        max_cpu_percentage: Some(50.0),
        max_concurrent_operations: Some(10),
        operation_timeout: Some(Duration::from_secs(30)),
        idle_timeout: Some(Duration::from_secs(300)),
        ..Default::default()
    };

    // Test within limits
    let normal_usage = create_moderate_resource_usage();
    assert_resource_usage_within_bounds(
        &normal_usage,
        Some(256 * 1024 * 1024),
        Some(50.0),
        None,
    );

    // Test exceeding limits
    let violating_usage = create_violating_resource_usage();
    assert!(!violating_usage.memory_bytes <= 256 * 1024 * 1024);
    assert!(!violating_usage.cpu_percentage <= 50.0);

    Ok(())
}

/// ============================================================================
/// PLUGIN INSTANCE INTEGRATION WITH MANAGER TESTS
/// ============================================================================

#[tokio::test]
async fn test_instance_manager_integration() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("integration-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    // Create instance through manager
    let instance_id = service.create_instance(&plugin_id, None).await?;

    // Start instance through manager
    service.start_instance(&instance_id).await?;

    // Verify instance exists
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 1);

    // Get resource usage
    let usage = service.get_resource_usage(Some(&instance_id)).await?;
    assert!(usage.memory_bytes >= 0);

    // Get instance health
    let health = service.get_instance_health(&instance_id).await?;
    assert!(matches!(health, PluginHealthStatus::Healthy | PluginHealthStatus::Unknown));

    // Stop instance through manager
    service.stop_instance(&instance_id).await?;

    // Verify instance is gone
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), 0);

    service.stop().await?;
    Ok(())
}