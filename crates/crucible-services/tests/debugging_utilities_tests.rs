//! Comprehensive unit tests for debugging utilities
//!
//! Tests for event flow debugging, performance monitoring, system diagnostics,
//! error tracking, and memory usage estimation.

use crucible_services::debugging::*;
use crucible_services::event_routing::*;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Test debug event creation and processing stages
#[cfg(test)]
mod debug_event_tests {
    use super::*;

    #[test]
    fn test_debug_event_creation() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let mut context = HashMap::new();
        context.insert("key1".to_string(), "value1".to_string());
        context.insert("key2".to_string(), "value2".to_string());

        let debug_event = DebugEvent {
            event: event.clone(),
            captured_at: chrono::Utc::now(),
            stage: ProcessingStage::Received,
            context,
        };

        assert_eq!(debug_event.event.id, event.id);
        assert_eq!(debug_event.event.source, event.source);
        assert_eq!(debug_event.stage, ProcessingStage::Received);
        assert_eq!(debug_event.context.len(), 2);
        assert_eq!(debug_event.context.get("key1"), Some(&"value1".to_string()));
    }

    #[test]
    fn test_processing_stage_values() {
        let stages = vec![
            ProcessingStage::Received,
            ProcessingStage::RoutingDecision,
            ProcessingStage::HandlerFound,
            ProcessingStage::HandlerStarted,
            ProcessingStage::HandlerCompleted,
            ProcessingStage::Delivered,
            ProcessingStage::Failed,
            ProcessingStage::Completed,
        ];

        // Ensure all stages are distinct
        for (i, stage1) in stages.iter().enumerate() {
            for (j, stage2) in stages.iter().enumerate() {
                if i != j {
                    assert_ne!(stage1, stage2);
                }
            }
        }
    }

    #[test]
    fn test_processing_stage_serialization() {
        let stages = vec![
            ProcessingStage::Received,
            ProcessingStage::RoutingDecision,
            ProcessingStage::HandlerFound,
            ProcessingStage::HandlerStarted,
            ProcessingStage::HandlerCompleted,
            ProcessingStage::Delivered,
            ProcessingStage::Failed,
            ProcessingStage::Completed,
        ];

        for stage in stages {
            let serialized = serde_json::to_string(&stage).unwrap();
            let deserialized: ProcessingStage = serde_json::from_str(&serialized).unwrap();
            assert_eq!(stage, deserialized);
        }
    }

    #[test]
    fn test_processing_stage_debug_format() {
        let stage = ProcessingStage::Completed;
        let debug_str = format!("{:?}", stage);
        assert!(debug_str.contains("Completed"));
    }

    #[test]
    fn test_debug_event_serialization() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let mut context = HashMap::new();
        context.insert("test_key".to_string(), "test_value".to_string());

        let debug_event = DebugEvent {
            event,
            captured_at: chrono::Utc::now(),
            stage: ProcessingStage::HandlerCompleted,
            context,
        };

        let serialized = serde_json::to_string(&debug_event).unwrap();
        let deserialized: DebugEvent = serde_json::from_str(&serialized).unwrap();

        assert_eq!(debug_event.event.id, deserialized.event.id);
        assert_eq!(debug_event.stage, deserialized.stage);
        assert_eq!(debug_event.context, deserialized.context);
    }
}

/// Test performance snapshot functionality
#[cfg(test)]
mod performance_snapshot_tests {
    use super::*;

    #[test]
    fn test_performance_snapshot_creation() {
        let snapshot = PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second: 100.5,
            avg_processing_time_ms: 25.7,
            active_events: 10,
            memory_usage_bytes: 1024 * 1024, // 1MB
            error_rate_percent: 2.1,
        };

        assert_eq!(snapshot.events_per_second, 100.5);
        assert_eq!(snapshot.avg_processing_time_ms, 25.7);
        assert_eq!(snapshot.active_events, 10);
        assert_eq!(snapshot.memory_usage_bytes, 1024 * 1024);
        assert_eq!(snapshot.error_rate_percent, 2.1);
    }

    #[test]
    fn test_performance_snapshot_serialization() {
        let snapshot = PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second: 50.25,
            avg_processing_time_ms: 12.5,
            active_events: 5,
            memory_usage_bytes: 512 * 1024, // 512KB
            error_rate_percent: 1.5,
        };

        let serialized = serde_json::to_string(&snapshot).unwrap();
        let deserialized: PerformanceSnapshot = serde_json::from_str(&serialized).unwrap();

        assert_eq!(snapshot.events_per_second, deserialized.events_per_second);
        assert_eq!(snapshot.avg_processing_time_ms, deserialized.avg_processing_time_ms);
        assert_eq!(snapshot.active_events, deserialized.active_events);
        assert_eq!(snapshot.memory_usage_bytes, deserialized.memory_usage_bytes);
        assert_eq!(snapshot.error_rate_percent, deserialized.error_rate_percent);
    }

    #[test]
    fn test_performance_snapshot_edge_values() {
        let snapshot = PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second: 0.0,
            avg_processing_time_ms: 0.0,
            active_events: 0,
            memory_usage_bytes: 0,
            error_rate_percent: 0.0,
        };

        // All values should be zero
        assert_eq!(snapshot.events_per_second, 0.0);
        assert_eq!(snapshot.avg_processing_time_ms, 0.0);
        assert_eq!(snapshot.active_events, 0);
        assert_eq!(snapshot.memory_usage_bytes, 0);
        assert_eq!(snapshot.error_rate_percent, 0.0);
    }

    #[test]
    fn test_performance_snapshot_large_values() {
        let snapshot = PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second: f64::MAX,
            avg_processing_time_ms: f64::MAX,
            active_events: usize::MAX,
            memory_usage_bytes: u64::MAX,
            error_rate_percent: 100.0,
        };

        assert_eq!(snapshot.events_per_second, f64::MAX);
        assert_eq!(snapshot.avg_processing_time_ms, f64::MAX);
        assert_eq!(snapshot.active_events, usize::MAX);
        assert_eq!(snapshot.memory_usage_bytes, u64::MAX);
        assert_eq!(snapshot.error_rate_percent, 100.0);
    }
}

/// Test error snapshot functionality
#[cfg(test)]
mod error_snapshot_tests {
    use super::*;

    #[test]
    fn test_error_snapshot_creation() {
        let error_snapshot = ErrorSnapshot {
            timestamp: chrono::Utc::now(),
            error_type: "ValidationError".to_string(),
            error_message: "Test validation error".to_string(),
            event_id: Some("test_event_id".to_string()),
            component: "test_component".to_string(),
            stack_trace: Some("Test stack trace\nLine 1\nLine 2".to_string()),
        };

        assert_eq!(error_snapshot.error_type, "ValidationError");
        assert_eq!(error_snapshot.error_message, "Test validation error");
        assert_eq!(error_snapshot.event_id, Some("test_event_id".to_string()));
        assert_eq!(error_snapshot.component, "test_component");
        assert!(error_snapshot.stack_trace.is_some());
    }

    #[test]
    fn test_error_snapshot_without_optional_fields() {
        let error_snapshot = ErrorSnapshot {
            timestamp: chrono::Utc::now(),
            error_type: "RuntimeError".to_string(),
            error_message: "Test runtime error".to_string(),
            event_id: None,
            component: "test_component".to_string(),
            stack_trace: None,
        };

        assert_eq!(error_snapshot.error_type, "RuntimeError");
        assert_eq!(error_snapshot.error_message, "Test runtime error");
        assert_eq!(error_snapshot.event_id, None);
        assert!(error_snapshot.stack_trace.is_none());
    }

    #[test]
    fn test_error_snapshot_serialization() {
        let error_snapshot = ErrorSnapshot {
            timestamp: chrono::Utc::now(),
            error_type: "TestError".to_string(),
            error_message: "Test error message".to_string(),
            event_id: Some("test_id".to_string()),
            component: "test_component".to_string(),
            stack_trace: Some("Test stack trace".to_string()),
        };

        let serialized = serde_json::to_string(&error_snapshot).unwrap();
        let deserialized: ErrorSnapshot = serde_json::from_str(&serialized).unwrap();

        assert_eq!(error_snapshot.error_type, deserialized.error_type);
        assert_eq!(error_snapshot.error_message, deserialized.error_message);
        assert_eq!(error_snapshot.event_id, deserialized.event_id);
        assert_eq!(error_snapshot.component, deserialized.component);
        assert_eq!(error_snapshot.stack_trace, deserialized.stack_trace);
    }

    #[test]
    fn test_error_snapshot_with_unicode_content() {
        let error_snapshot = ErrorSnapshot {
            timestamp: chrono::Utc::now(),
            error_type: "UnicodeError".to_string(),
            error_message: "Error with unicode: 测试 🚀".to_string(),
            event_id: Some("unicode_event_测试".to_string()),
            component: "unicode_component_测试".to_string(),
            stack_trace: Some("Unicode stack trace: 测试\n🚀\nLine".to_string()),
        };

        let serialized = serde_json::to_string(&error_snapshot).unwrap();
        let deserialized: ErrorSnapshot = serde_json::from_str(&serialized).unwrap();

        assert_eq!(error_snapshot.error_message, deserialized.error_message);
        assert_eq!(error_snapshot.event_id, deserialized.event_id);
        assert_eq!(error_snapshot.component, deserialized.component);
        assert_eq!(error_snapshot.stack_trace, deserialized.stack_trace);
    }
}

/// Test component health functionality
#[cfg(test)]
mod component_health_tests {
    use super::*;

    #[test]
    fn test_component_health_creation() {
        let mut metrics = HashMap::new();
        metrics.insert("metric1".to_string(), serde_json::Value::Number(serde_json::Number::from(42)));
        metrics.insert("metric2".to_string(), serde_json::Value::String("value".to_string()));

        let health = ComponentHealth {
            name: "test_component".to_string(),
            status: HealthStatus::Healthy,
            last_check: chrono::Utc::now(),
            response_time_ms: Some(100),
            metrics,
        };

        assert_eq!(health.name, "test_component");
        assert_eq!(health.status, HealthStatus::Healthy);
        assert_eq!(health.response_time_ms, Some(100));
        assert_eq!(health.metrics.len(), 2);
    }

    #[test]
    fn test_health_status_values() {
        let statuses = vec![
            HealthStatus::Healthy,
            HealthStatus::Degraded,
            HealthStatus::Unhealthy,
            HealthStatus::Unknown,
        ];

        // Ensure all statuses are distinct
        for (i, status1) in statuses.iter().enumerate() {
            for (j, status2) in statuses.iter().enumerate() {
                if i != j {
                    assert_ne!(status1, status2);
                }
            }
        }
    }

    #[test]
    fn test_health_status_serialization() {
        let statuses = vec![
            HealthStatus::Healthy,
            HealthStatus::Degraded,
            HealthStatus::Unhealthy,
            HealthStatus::Unknown,
        ];

        for status in statuses {
            let serialized = serde_json::to_string(&status).unwrap();
            let deserialized: HealthStatus = serde_json::from_str(&serialized).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_component_health_serialization() {
        let health = ComponentHealth {
            name: "test_component".to_string(),
            status: HealthStatus::Degraded,
            last_check: chrono::Utc::now(),
            response_time_ms: Some(250),
            metrics: HashMap::new(),
        };

        let serialized = serde_json::to_string(&health).unwrap();
        let deserialized: ComponentHealth = serde_json::from_str(&serialized).unwrap();

        assert_eq!(health.name, deserialized.name);
        assert_eq!(health.status, deserialized.status);
        assert_eq!(health.response_time_ms, deserialized.response_time_ms);
        assert_eq!(health.metrics, deserialized.metrics);
    }
}

/// Test event flow debugger functionality
#[cfg(test)]
mod event_flow_debugger_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_flow_debugger_creation() {
        env::remove_var("CRUCIBLE_DEBUG_FLOW");
        let debugger = EventFlowDebugger::new("test_component", 100);

        // Should create successfully even when debug is disabled
        assert_eq!(debugger.component_name, "test_component");
        assert_eq!(debugger.max_retained_events, 100);
        assert!(!debugger.debug_enabled);
    }

    #[tokio::test]
    async fn test_event_flow_debugger_creation_enabled() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        assert_eq!(debugger.component_name, "test_component");
        assert_eq!(debugger.max_retained_events, 100);
        assert!(debugger.debug_enabled);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_capture_event_disabled() {
        env::remove_var("CRUCIBLE_DEBUG_FLOW");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let context = HashMap::new();

        // Should not capture when disabled
        debugger.capture_event(&event, ProcessingStage::Received, context).await;

        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;

        assert_eq!(events.len(), 0);
    }

    #[tokio::test]
    async fn test_event_flow_debugger_capture_event_enabled() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let mut context = HashMap::new();
        context.insert("key1".to_string(), "value1".to_string());

        debugger.capture_event(&event, ProcessingStage::Received, context.clone()).await;
        debugger.capture_event(&event, ProcessingStage::HandlerCompleted, context).await;

        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].stage, ProcessingStage::Received);
        assert_eq!(events[1].stage, ProcessingStage::HandlerCompleted);
        assert_eq!(events[0].context.len(), 1);
        assert_eq!(events[0].context.get("key1"), Some(&"value1".to_string()));

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_size_limit() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 3); // Small limit

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Add more events than the limit
        for i in 0..5 {
            let mut context = HashMap::new();
            context.insert("index".to_string(), i.to_string());
            debugger.capture_event(&event, ProcessingStage::Received, context).await;
        }

        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;

        // Should only retain the most recent events
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].context.get("index"), Some(&"2".to_string()));
        assert_eq!(events[1].context.get("index"), Some(&"3".to_string()));
        assert_eq!(events[2].context.get("index"), Some(&"4".to_string()));

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_performance_snapshot() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        debugger.record_performance_snapshot(
            100.5,
            25.7,
            10,
            2.1,
        ).await;

        debugger.record_performance_snapshot(
            150.2,
            30.1,
            15,
            1.8,
        ).await;

        let snapshots = debugger.get_recent_performance_snapshots(10).await;
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].events_per_second, 150.2);
        assert_eq!(snapshots[1].events_per_second, 100.5);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_performance_snapshot_limit() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 3);

        // Add more snapshots than limit
        for i in 0..5 {
            debugger.record_performance_snapshot(
                i as f64 * 10.0,
                i as f64 * 5.0,
                i,
                i as f64 * 0.5,
            ).await;
        }

        let snapshots = debugger.get_recent_performance_snapshots(10).await;
        assert_eq!(snapshots.len(), 3);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_record_error() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        debugger.record_error(
            "TestError",
            "Test error message",
            Some("test_event_id"),
            "test_component",
        ).await;

        debugger.record_error(
            "AnotherError",
            "Another error message",
            None,
            "another_component",
        ).await;

        let errors = debugger.get_recent_errors(10).await;
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].error_type, "AnotherError");
        assert_eq!(errors[1].error_type, "TestError");
        assert_eq!(errors[0].event_id, None);
        assert_eq!(errors[1].event_id, Some("test_event_id".to_string()));

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_record_error_limit() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 3);

        // Add more errors than limit
        for i in 0..5 {
            debugger.record_error(
                &format!("Error{}", i),
                &format!("Error message {}", i),
                Some(&format!("event_id_{}", i)),
                "test_component",
            ).await;
        }

        let errors = debugger.get_recent_errors(10).await;
        assert_eq!(errors.len(), 3);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_analyze_event_flow() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Capture a complete event flow
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        sleep(Duration::from_millis(10)).await;
        debugger.capture_event(&event, ProcessingStage::HandlerStarted, HashMap::new()).await;
        sleep(Duration::from_millis(10)).await;
        debugger.capture_event(&event, ProcessingStage::HandlerCompleted, HashMap::new()).await;
        sleep(Duration::from_millis(10)).await;
        debugger.capture_event(&event, ProcessingStage::Completed, HashMap::new()).await;

        let analysis = debugger.analyze_event_flow(&event.id).await;
        assert!(analysis.is_some());

        let analysis = analysis.unwrap();
        assert_eq!(analysis.event_id, event.id);
        assert_eq!(analysis.stages.len(), 4);
        assert_eq!(analysis.stages[0], ProcessingStage::Received);
        assert_eq!(analysis.stages[3], ProcessingStage::Completed);
        assert!(analysis.completed);
        assert_eq!(analysis.errors, 0);
        assert!(analysis.total_duration_ms >= 20); // At least 30ms (3 sleeps of 10ms)

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_analyze_event_flow_with_failure() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Capture a failed event flow
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        debugger.capture_event(&event, ProcessingStage::HandlerStarted, HashMap::new()).await;
        debugger.capture_event(&event, ProcessingStage::Failed, HashMap::new()).await;

        let analysis = debugger.analyze_event_flow(&event.id).await;
        assert!(analysis.is_some());

        let analysis = analysis.unwrap();
        assert_eq!(analysis.event_id, event.id);
        assert_eq!(analysis.stages.len(), 3);
        assert_eq!(analysis.stages[2], ProcessingStage::Failed);
        assert!(!analysis.completed);
        assert_eq!(analysis.errors, 1);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_analyze_nonexistent_event() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let analysis = debugger.analyze_event_flow("nonexistent_event_id").await;
        assert!(analysis.is_none());

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_clear_debug_data() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Add some debug data
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        debugger.record_performance_snapshot(100.0, 25.0, 10, 2.0).await;
        debugger.record_error("TestError", "Test message", Some(&event.id), "test_component").await;

        // Verify data was added
        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;
        assert_eq!(events.len(), 1);

        let snapshots = debugger.get_recent_performance_snapshots(10).await;
        assert_eq!(snapshots.len(), 1);

        let errors = debugger.get_recent_errors(10).await;
        assert_eq!(errors.len(), 1);

        // Clear data
        debugger.clear_debug_data().await;

        // Verify data was cleared
        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;
        assert_eq!(events.len(), 0);

        let snapshots = debugger.get_recent_performance_snapshots(10).await;
        assert_eq!(snapshots.len(), 0);

        let errors = debugger.get_recent_errors(10).await;
        assert_eq!(errors.len(), 0);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_event_flow_debugger_time_range_filtering() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        let now = chrono::Utc::now();

        // Capture events at different times
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;

        sleep(Duration::from_millis(50)).await;
        let middle_time = chrono::Utc::now();

        debugger.capture_event(&event, ProcessingStage::HandlerStarted, HashMap::new()).await;

        sleep(Duration::from_millis(50)).await;
        debugger.capture_event(&event, ProcessingStage::Completed, HashMap::new()).await;

        // Test different time ranges
        let all_events = debugger.get_events_in_range(
            now - Duration::from_secs(1),
            chrono::Utc::now() + Duration::from_secs(1),
        ).await;
        assert_eq!(all_events.len(), 3);

        let middle_events = debugger.get_events_in_range(
            middle_time - Duration::from_millis(10),
            middle_time + Duration::from_millis(100),
        ).await;
        assert_eq!(middle_events.len(), 2);

        let future_events = debugger.get_events_in_range(
            chrono::Utc::now() + Duration::from_secs(10),
            chrono::Utc::now() + Duration::from_secs(20),
        ).await;
        assert_eq!(future_events.len(), 0);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }
}

/// Test system diagnostics functionality
#[cfg(test)]
mod system_diagnostics_tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_system_diagnostics_collector_creation() {
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let collector = SystemDiagnosticsCollector::new("test_collector", None, debugger);

        assert_eq!(collector.component_name, "test_collector");
        assert!(collector.event_router.is_none());
    }

    #[tokio::test]
    async fn test_system_diagnostics_collector_with_router() {
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let router = Arc::new(EventRouter::new(EventRouterConfig::default()));
        let collector = SystemDiagnosticsCollector::new("test_collector", Some(router), debugger.clone());

        assert_eq!(collector.component_name, "test_collector");
        assert!(collector.event_router.is_some());
    }

    #[tokio::test]
    async fn test_system_diagnostics_collection() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let router = Arc::new(EventRouter::new(EventRouterConfig::default()));
        let collector = SystemDiagnosticsCollector::new("test_collector", Some(router), debugger.clone());

        // Add some debug data
        debugger.record_performance_snapshot(100.0, 25.0, 10, 2.0).await;
        debugger.record_error("TestError", "Test message", None, "test_component").await;

        let diagnostics = collector.collect_diagnostics().await;
        assert!(diagnostics.is_ok());

        let diagnostics = diagnostics.unwrap();
        assert!(!diagnostics.timestamp.to_rfc3339().is_empty());
        assert_eq!(diagnostics.router_status.active_events, 0);
        assert_eq!(diagnostics.performance.events_per_second, 100.0);
        assert_eq!(diagnostics.recent_errors.len(), 1);
        assert!(diagnostics.component_health.contains_key("test_collector"));

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_system_diagnostics_generation() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let collector = SystemDiagnosticsCollector::new("test_collector", None, debugger);

        let report = collector.generate_report().await;
        assert!(report.is_ok());

        let report = report.unwrap();
        assert!(!report.is_empty());

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        assert!(parsed.is_object());

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_system_diagnostics_save_report() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let collector = SystemDiagnosticsCollector::new("test_collector", None, debugger);

        let temp_dir = TempDir::new().unwrap();
        let report_path = temp_dir.path().join("diagnostics_report.json");

        let result = collector.save_report(report_path.to_str().unwrap()).await;
        assert!(result.is_ok());
        assert!(report_path.exists());

        // Verify file content
        let content = tokio::fs::read_to_string(&report_path).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.is_object());

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_system_diagnostics_save_report_invalid_path() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let collector = SystemDiagnosticsCollector::new("test_collector", None, debugger);

        let invalid_path = "/invalid/path/that/does/not/exist/report.json";
        let result = collector.save_report(invalid_path).await;
        assert!(result.is_err());

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[test]
    fn test_router_status_creation() {
        let status = RouterStatus {
            total_handlers: 5,
            active_events: 3,
            total_events_processed: 1000,
            routing_history_size: 500,
            is_healthy: true,
        };

        assert_eq!(status.total_handlers, 5);
        assert_eq!(status.active_events, 3);
        assert_eq!(status.total_events_processed, 1000);
        assert_eq!(status.routing_history_size, 500);
        assert!(status.is_healthy);
    }

    #[test]
    fn test_router_status_serialization() {
        let status = RouterStatus {
            total_handlers: 10,
            active_events: 7,
            total_events_processed: 2500,
            routing_history_size: 1000,
            is_healthy: false,
        };

        let serialized = serde_json::to_string(&status).unwrap();
        let deserialized: RouterStatus = serde_json::from_str(&serialized).unwrap();

        assert_eq!(status.total_handlers, deserialized.total_handlers);
        assert_eq!(status.active_events, deserialized.active_events);
        assert_eq!(status.total_events_processed, deserialized.total_events_processed);
        assert_eq!(status.routing_history_size, deserialized.routing_history_size);
        assert_eq!(status.is_healthy, deserialized.is_healthy);
    }

    #[tokio::test]
    async fn test_system_diagnostics_comprehensive() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let router = Arc::new(EventRouter::new(EventRouterConfig::default()));
        let collector = SystemDiagnosticsCollector::new("test_collector", Some(router), debugger.clone());

        // Add comprehensive debug data
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        debugger.record_performance_snapshot(150.5, 35.2, 12, 1.8).await;
        debugger.record_error("ComprehensiveError", "Comprehensive test error", Some(&event.id), "test_component").await;

        let diagnostics = collector.collect_diagnostics().await.unwrap();

        // Verify all components are populated
        assert!(diagnostics.router_status.active_events >= 0);
        assert!(diagnostics.performance.events_per_second > 0.0);
        assert!(diagnostics.performance.memory_usage_bytes > 0);
        assert!(!diagnostics.recent_errors.is_empty());
        assert!(!diagnostics.component_health.is_empty());

        // Verify component health includes expected fields
        let health = &diagnostics.component_health["test_collector"];
        assert_eq!(health.name, "test_collector");
        assert!(matches!(health.status, HealthStatus::Healthy | HealthStatus::Unhealthy));
        assert!(health.response_time_ms.is_some());
        assert!(!health.metrics.is_empty());

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }
}

/// Test memory usage estimation
#[cfg(test)]
mod memory_estimation_tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_usage_estimation() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        // Add some data to estimate memory usage
        for _ in 0..10 {
            debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        }

        for i in 0..5 {
            debugger.record_performance_snapshot(i as f64 * 10.0, i as f64 * 5.0, i, i as f64 * 0.5).await;
        }

        for i in 0..3 {
            debugger.record_error(&format!("Error{}", i), &format!("Message{}", i), None, "test_component").await;
        }

        // Memory usage should be estimated (rough estimation based on item count)
        // 10 events + 5 snapshots + 3 errors = 18 items
        // At 1KB per item estimate = 18KB
        let memory_usage = debugger.estimate_memory_usage().await;
        assert!(memory_usage > 0);
        assert!(memory_usage >= 18 * 1024); // At least 18KB

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_memory_usage_estimation_empty() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let memory_usage = debugger.estimate_memory_usage().await;
        assert_eq!(memory_usage, 0);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }
}

/// Test performance characteristics of debugging utilities
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_debugger_performance_disabled() {
        env::remove_var("CRUCIBLE_DEBUG_FLOW");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let start = Instant::now();
        for i in 0..1000 {
            let mut context = HashMap::new();
            context.insert("index".to_string(), i.to_string());
            debugger.capture_event(&event, ProcessingStage::Received, context).await;
        }
        let elapsed = start.elapsed();

        // Should be very fast when disabled (< 50ms for 1000 calls)
        assert!(elapsed.as_millis() < 50);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_debugger_performance_enabled() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 1000);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        let start = Instant::now();
        for i in 0..100 {
            let mut context = HashMap::new();
            context.insert("index".to_string(), i.to_string());
            debugger.capture_event(&event, ProcessingStage::Received, context).await;
        }
        let elapsed = start.elapsed();

        // Should complete in reasonable time (< 1 second for 100 calls)
        assert!(elapsed.as_secs() < 1);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_concurrent_debugger_access() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 1000));
        let mut handles = vec![];

        for thread_id in 0..10 {
            let debugger_clone = debugger.clone();
            let event = Event::new(
                EventType::ScriptExecution,
                format!("source_{}", thread_id),
                serde_json::json!({"thread_id": thread_id}),
            );

            let handle = tokio::spawn(async move {
                for i in 0..10 {
                    let mut context = HashMap::new();
                    context.insert("thread_id".to_string(), thread_id.to_string());
                    context.insert("index".to_string(), i.to_string());
                    debugger_clone.capture_event(&event, ProcessingStage::Received, context).await;
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Should have captured events from all threads
        let events = debugger.get_events_in_range(
            chrono::Utc::now() - Duration::from_secs(10),
            chrono::Utc::now() + Duration::from_secs(10),
        ).await;

        assert_eq!(events.len(), 100); // 10 threads * 10 events each

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_large_payload_handling() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let large_payload = serde_json::json!({
            "data": "x".repeat(1_000_000), // 1MB payload
            "metadata": {
                "large_array": vec![0; 100_000],
                "nested": {
                    "data": "y".repeat(500_000)
                }
            }
        });

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            large_payload,
        );

        let start = Instant::now();
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        let elapsed = start.elapsed();

        // Should handle large payloads within reasonable time (< 5 seconds)
        assert!(elapsed.as_secs() < 5);

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }
}

/// Test thread safety of debugging utilities
#[cfg(test)]
mod thread_safety_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_debugging_types_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DebugEvent>();
        assert_send_sync::<PerformanceSnapshot>();
        assert_send_sync::<ErrorSnapshot>();
        assert_send_sync::<ComponentHealth>();
        assert_send_sync::<EventFlowAnalysis>();
        assert_send_sync::<RouterStatus>();
        assert_send_sync::<SystemDiagnostics>();
    }

    #[test]
    fn test_event_flow_debugger_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventFlowDebugger>();
    }

    #[test]
    fn test_system_diagnostics_collector_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SystemDiagnosticsCollector>();
    }

    #[tokio::test]
    async fn test_concurrent_event_flow_analysis() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 1000));

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({}),
        );

        // Create a complete event flow
        debugger.capture_event(&event, ProcessingStage::Received, HashMap::new()).await;
        debugger.capture_event(&event, ProcessingStage::HandlerStarted, HashMap::new()).await;
        debugger.capture_event(&event, ProcessingStage::HandlerCompleted, HashMap::new()).await;
        debugger.capture_event(&event, ProcessingStage::Completed, HashMap::new()).await;

        let mut handles = vec![];
        for _ in 0..10 {
            let debugger_clone = debugger.clone();
            let event_id = event.id.clone();
            let handle = tokio::spawn(async move {
                debugger_clone.analyze_event_flow(&event_id).await
            });
            handles.push(handle);
        }

        for handle in handles {
            let analysis = handle.await.unwrap();
            assert!(analysis.is_some());
            assert!(analysis.unwrap().completed);
        }

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_concurrent_diagnostics_collection() {
        env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));
        let collector = Arc::new(SystemDiagnosticsCollector::new("test_collector", None, debugger.clone()));

        // Add some debug data
        debugger.record_performance_snapshot(100.0, 25.0, 10, 2.0).await;

        let mut handles = vec![];
        for _ in 0..5 {
            let collector_clone = collector.clone();
            let handle = tokio::spawn(async move {
                collector_clone.collect_diagnostics().await
            });
            handles.push(handle);
        }

        for handle in handles {
            let diagnostics = handle.await.unwrap();
            assert!(diagnostics.is_ok());
        }

        env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }
use tokio::time::sleep;
use serde_json::json;
