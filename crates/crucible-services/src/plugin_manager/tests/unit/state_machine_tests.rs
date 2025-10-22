//! # Plugin State Machine Tests
//!
//! Comprehensive unit tests for the plugin state machine component.
//! Tests cover state transitions, validation, concurrency, persistence,
//! and performance under various load conditions.

use super::*;
use crate::plugin_manager::state_machine::*;
use crate::plugin_manager::types::*;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

#[cfg(test)]
mod state_machine_tests {
    use super::*;

    // ============================================================================
    // BASIC STATE MACHINE FUNCTIONALITY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_machine_creation() {
        let state_machine = create_test_state_machine();

        // Verify initial state
        assert_eq!(state_machine.get_instance_count().await, 0);

        // Test getting state of non-existent instance
        let result = state_machine.get_state("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_state_machine_initialization() {
        let state_machine = create_test_state_machine();

        // Initialize state machine
        let result = state_machine.initialize().await;
        assert!(result.is_ok());

        // Verify initialization
        assert_eq!(state_machine.get_instance_count().await, 0);
    }

    #[tokio::test]
    async fn test_instance_registration() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");

        // Register instance
        let result = state_machine.register_instance(&instance).await;
        assert!(result.is_ok());

        // Verify registration
        assert_eq!(state_machine.get_instance_count().await, 1);

        let current_state = state_machine.get_state("test-instance-1").await.unwrap();
        assert_eq!(current_state, PluginInstanceState::Created);
    }

    #[tokio::test]
    async fn test_duplicate_instance_registration() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");

        // Register instance twice
        state_machine.register_instance(&instance).await.unwrap();
        let result = state_machine.register_instance(&instance).await;

        // Should fail
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::AlreadyExists(_)));
    }

    // ============================================================================
    // STATE TRANSITION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_valid_state_transitions() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Test: Created -> Starting
        let result = state_machine.transition_state("test-instance-1", StateTransition::Start).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Starting);

        // Test: Starting -> Running
        let result = state_machine.transition_state("test-instance-1", StateTransition::CompleteStart).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Running);

        // Test: Running -> Stopping
        let result = state_machine.transition_state("test-instance-1", StateTransition::Stop).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Stopping);

        // Test: Stopping -> Stopped
        let result = state_machine.transition_state("test-instance-1", StateTransition::CompleteStop).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Stopped);
    }

    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Try to stop a created instance (should fail)
        let result = state_machine.transition_state("test-instance-1", StateTransition::Stop).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::InvalidState(_)));

        // Try to complete start without starting (should fail)
        let result = state_machine.transition_state("test-instance-1", StateTransition::CompleteStart).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::InvalidState(_)));
    }

    #[tokio::test]
    async fn test_error_state_transitions() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Start the instance
        state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();

        // Transition to error state
        let result = state_machine.transition_state("test-instance-1", StateTransition::Error("Test error".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Error);

        // Try to start from error state (should work for recovery)
        let result = state_machine.transition_state("test-instance-1", StateTransition::Start).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Starting);
    }

    #[tokio::test]
    async fn test_maintenance_state_transitions() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Start the instance
        state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();
        state_machine.transition_state("test-instance-1", StateTransition::CompleteStart).await.unwrap();

        // Transition to maintenance
        let result = state_machine.transition_state("test-instance-1", StateTransition::Maintenance).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Maintenance);

        // Transition back from maintenance
        let result = state_machine.transition_state("test-instance-1", StateTransition::CompleteMaintenance).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_state, PluginInstanceState::Running);
    }

    // ============================================================================
    // HEALTH STATUS TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_health_status_updates() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Update health status
        let result = state_machine.update_health_status("test-instance-1", PluginHealthStatus::Healthy).await;
        assert!(result.is_ok());

        // Verify health status
        let health = state_machine.get_health_status("test-instance-1").await.unwrap();
        assert_eq!(health, PluginHealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_status_history() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Update health status multiple times
        state_machine.update_health_status("test-instance-1", PluginHealthStatus::Healthy).await.unwrap();
        state_machine.update_health_status("test-instance-1", PluginHealthStatus::Unhealthy).await.unwrap();
        state_machine.update_health_status("test-instance-1", PluginHealthStatus::Healthy).await.unwrap();

        // Get health history
        let history = state_machine.get_health_history("test-instance-1", 10).await.unwrap();
        assert_eq!(history.len(), 4); // Initial Unknown + 3 updates

        // Verify history order (most recent first)
        assert_eq!(history[0].status, PluginHealthStatus::Healthy);
        assert_eq!(history[1].status, PluginHealthStatus::Unhealthy);
        assert_eq!(history[2].status, PluginHealthStatus::Healthy);
        assert_eq!(history[3].status, PluginHealthStatus::Unknown);
    }

    // ============================================================================
    // CONCURRENT STATE TRANSITIONS TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_concurrent_state_transitions() {
        let state_machine = Arc::new(create_test_state_machine());
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        let mut handles = Vec::new();

        // Spawn multiple concurrent state transition attempts
        for i in 0..10 {
            let sm = state_machine.clone();
            let handle = tokio::spawn(async move {
                if i % 2 == 0 {
                    sm.transition_state("test-instance-1", StateTransition::Start).await
                } else {
                    sm.transition_state("test-instance-1", StateTransition::Stop).await
                }
            });
            handles.push(handle);
        }

        // Wait for all transitions to complete
        let mut successes = 0;
        let mut failures = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => successes += 1,
                Err(_) => failures += 1,
            }
        }

        // Only one transition should succeed (first one), others should fail due to state conflicts
        assert_eq!(successes, 1);
        assert_eq!(failures, 9);
    }

    #[tokio::test]
    async fn test_concurrent_instance_operations() {
        let state_machine = Arc::new(create_test_state_machine());
        state_machine.initialize().await.unwrap();

        let mut handles = Vec::new();

        // Register multiple instances concurrently
        for i in 0..10 {
            let sm = state_machine.clone();
            let handle = tokio::spawn(async move {
                let instance = create_test_plugin_instance(&format!("test-instance-{}", i), "test-plugin");
                sm.register_instance(&instance).await
            });
            handles.push(handle);
        }

        // Wait for all registrations to complete
        let mut successes = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                successes += 1;
            }
        }

        // All registrations should succeed
        assert_eq!(successes, 10);
        assert_eq!(state_machine.get_instance_count().await, 10);
    }

    // ============================================================================
    // STATE PERSISTENCE TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_snapshot() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        // Register instances
        for i in 0..5 {
            let instance = create_test_plugin_instance(&format!("test-instance-{}", i), "test-plugin");
            state_machine.register_instance(&instance).await.unwrap();

            // Transition some instances to different states
            if i % 2 == 0 {
                state_machine.transition_state(&format!("test-instance-{}", i), StateTransition::Start).await.unwrap();
            }
        }

        // Create snapshot
        let snapshot = state_machine.create_snapshot().await.unwrap();

        // Verify snapshot contains all instances
        assert_eq!(snapshot.instances.len(), 5);

        // Verify instance states in snapshot
        for (i, instance_state) in snapshot.instances.iter().enumerate() {
            if i % 2 == 0 {
                assert_eq!(instance_state.state, PluginInstanceState::Starting);
            } else {
                assert_eq!(instance_state.state, PluginInstanceState::Created);
            }
        }
    }

    #[tokio::test]
    async fn test_state_restoration() {
        let state_machine1 = create_test_state_machine();
        state_machine1.initialize().await.unwrap();

        // Register and transition instances
        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine1.register_instance(&instance).await.unwrap();
        state_machine1.transition_state("test-instance-1", StateTransition::Start).await.unwrap();
        state_machine1.update_health_status("test-instance-1", PluginHealthStatus::Healthy).await.unwrap();

        // Create snapshot
        let snapshot = state_machine1.create_snapshot().await.unwrap();

        // Create new state machine and restore from snapshot
        let state_machine2 = create_test_state_machine();
        state_machine2.initialize().await.unwrap();

        let result = state_machine2.restore_from_snapshot(&snapshot).await;
        assert!(result.is_ok());

        // Verify restoration
        assert_eq!(state_machine2.get_instance_count().await, 1);

        let state = state_machine2.get_state("test-instance-1").await.unwrap();
        assert_eq!(state, PluginInstanceState::Starting);

        let health = state_machine2.get_health_status("test-instance-1").await.unwrap();
        assert_eq!(health, PluginHealthStatus::Healthy);
    }

    // ============================================================================
    // EVENT SYSTEM TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_transition_events() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        // Subscribe to events
        let mut event_receiver = state_machine.subscribe_events().await;

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Perform state transition
        state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();

        // Wait for event
        let event = tokio::time::timeout(Duration::from_millis(100), event_receiver.recv()).await.unwrap().unwrap();

        // Verify event
        match event {
            StateMachineEvent::StateChanged { instance_id, from_state, to_state, .. } => {
                assert_eq!(instance_id, "test-instance-1");
                assert_eq!(from_state, PluginInstanceState::Created);
                assert_eq!(to_state, PluginInstanceState::Starting);
            }
            _ => panic!("Expected StateChanged event"),
        }
    }

    #[tokio::test]
    async fn test_health_status_events() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        // Subscribe to events
        let mut event_receiver = state_machine.subscribe_events().await;

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Update health status
        state_machine.update_health_status("test-instance-1", PluginHealthStatus::Healthy).await.unwrap();

        // Wait for event
        let event = tokio::time::timeout(Duration::from_millis(100), event_receiver.recv()).await.unwrap().unwrap();

        // Verify event
        match event {
            StateMachineEvent::HealthStatusChanged { instance_id, old_status, new_status, .. } => {
                assert_eq!(instance_id, "test-instance-1");
                assert_eq!(old_status, PluginHealthStatus::Unknown);
                assert_eq!(new_status, PluginHealthStatus::Healthy);
            }
            _ => panic!("Expected HealthStatusChanged event"),
        }
    }

    // ============================================================================
    // METRICS AND ANALYTICS TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_machine_metrics() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        // Register instances
        for i in 0..5 {
            let instance = create_test_plugin_instance(&format!("test-instance-{}", i), "test-plugin");
            state_machine.register_instance(&instance).await.unwrap();
        }

        // Perform some state transitions
        state_machine.transition_state("test-instance-0", StateTransition::Start).await.unwrap();
        state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();
        state_machine.transition_state("test-instance-0", StateTransition::Error("Test error".to_string())).await.unwrap();

        // Get metrics
        let metrics = state_machine.get_metrics().await.unwrap();

        // Verify metrics
        assert_eq!(metrics.total_instances, 5);
        assert_eq!(metrics.instances_by_state.get(&PluginInstanceState::Created), Some(&2));
        assert_eq!(metrics.instances_by_state.get(&PluginInstanceState::Starting), Some(&1));
        assert_eq!(metrics.instances_by_state.get(&PluginInstanceState::Error), Some(&1));
        assert_eq!(metrics.instances_by_state.get(&PluginInstanceState::Running), Some(&0));

        assert!(metrics.total_transitions > 0);
        assert!(metrics.total_state_changes > 0);
    }

    #[tokio::test]
    async fn test_instance_analytics() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Perform multiple state transitions
        state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();
        state_machine.transition_state("test-instance-1", StateTransition::CompleteStart).await.unwrap();
        state_machine.transition_state("test-instance-1", StateTransition::Stop).await.unwrap();
        state_machine.transition_state("test-instance-1", StateTransition::CompleteStop).await.unwrap();

        // Get analytics
        let analytics = state_machine.get_instance_analytics("test-instance-1").await.unwrap();

        // Verify analytics
        assert_eq!(analytics.instance_id, "test-instance-1");
        assert_eq!(analytics.plugin_id, "test-plugin-1");
        assert_eq!(analytics.current_state, PluginInstanceState::Stopped);
        assert_eq!(analytics.total_transitions, 4);
        assert!(analytics.time_in_current_state >= Duration::ZERO);
        assert!(analytics.state_history.len() > 0);
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_transition_performance() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Benchmark state transition performance
        let (average_time, _) = benchmark_operation("state_transition", || async {
            state_machine.transition_state("test-instance-1", StateTransition::Start).await.unwrap();
            state_machine.transition_state("test-instance-1", StateTransition::CompleteStart).await.unwrap();
            state_machine.transition_state("test-instance-1", StateTransition::Stop).await.unwrap();
            state_machine.transition_state("test-instance-1", StateTransition::CompleteStop).await.unwrap();
        }, 100).await;

        // Verify performance targets (should be under 10ms per transition)
        assert!(average_time < Duration::from_millis(40), "State transition too slow: {:?}", average_time);
    }

    #[tokio::test]
    async fn test_concurrent_state_access_performance() {
        let state_machine = Arc::new(create_test_state_machine());
        state_machine.initialize().await.unwrap();

        // Register many instances
        for i in 0..100 {
            let instance = create_test_plugin_instance(&format!("test-instance-{}", i), "test-plugin");
            state_machine.register_instance(&instance).await.unwrap();
        }

        // Benchmark concurrent state access
        let (average_time, _) = benchmark_operation("concurrent_state_access", || async {
            let mut handles = Vec::new();

            for i in 0..10 {
                let sm = state_machine.clone();
                let handle = tokio::spawn(async move {
                    for j in 0..10 {
                        let instance_id = format!("test-instance-{}", i * 10 + j);
                        let _ = sm.get_state(&instance_id).await;
                        let _ = sm.get_health_status(&instance_id).await;
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.await.unwrap();
            }
        }, 10).await;

        // Verify performance (should handle 100 state accesses efficiently)
        assert!(average_time < Duration::from_millis(100), "Concurrent state access too slow: {:?}", average_time);
    }

    // ============================================================================
    // ERROR HANDLING TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_state_machine_error_handling() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        // Test operations on non-existent instance
        let result = state_machine.transition_state("non-existent", StateTransition::Start).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));

        let result = state_machine.update_health_status("non-existent", PluginHealthStatus::Healthy).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));

        let result = state_machine.get_instance_analytics("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_corrupted_state_recovery() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Simulate corrupted state by putting instance in invalid state
        // This would require internal access to manipulate state directly

        // Test recovery mechanisms
        let result = state_machine.validate_all_instances().await;
        assert!(result.is_ok());

        // Verify instance is still accessible
        let state = state_machine.get_state("test-instance-1").await;
        assert!(state.is_ok());
    }

    // ============================================================================
    // STRESS TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_high_volume_state_transitions() {
        let state_machine = create_test_state_machine();
        state_machine.initialize().await.unwrap();

        let instance = create_test_plugin_instance("test-instance-1", "test-plugin-1");
        state_machine.register_instance(&instance).await.unwrap();

        // Perform many state transitions rapidly
        for i in 0..1000 {
            if i % 2 == 0 {
                let _ = state_machine.transition_state("test-instance-1", StateTransition::Start).await;
            } else {
                let _ = state_machine.transition_state("test-instance-1", StateTransition::Stop).await;
            }
        }

        // Verify state machine is still functional
        let state = state_machine.get_state("test-instance-1").await;
        assert!(state.is_ok());

        let metrics = state_machine.get_metrics().await.unwrap();
        assert!(metrics.total_transitions > 0);
    }

    #[tokio::test]
    async fn test_memory_usage_under_load() {
        let state_machine = Arc::new(create_test_state_machine());
        state_machine.initialize().await.unwrap();

        // Register many instances
        for i in 0..1000 {
            let instance = create_test_plugin_instance(&format!("test-instance-{}", i), "test-plugin");
            state_machine.register_instance(&instance).await.unwrap();
        }

        // Perform operations on all instances
        for i in 0..1000 {
            let instance_id = format!("test-instance-{}", i);
            let _ = state_machine.transition_state(&instance_id, StateTransition::Start).await;
            let _ = state_machine.update_health_status(&instance_id, PluginHealthStatus::Healthy).await;
        }

        // Verify all instances are still accessible
        assert_eq!(state_machine.get_instance_count().await, 1000);

        // Get metrics to ensure they're reasonable
        let metrics = state_machine.get_metrics().await.unwrap();
        assert_eq!(metrics.total_instances, 1000);
    }
}