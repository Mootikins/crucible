//! Integration tests for logging and debugging framework
//!
//! This test module validates the complete logging and debugging system
//! integration with existing components.

use crucible_services::{
    config::CrucibleConfig,
    debugging::{EventFlowDebugger, ProcessingStage, SystemDiagnosticsCollector},
    event_routing::{Event, EventRouter, EventType, EventPriority, EventHandler, RoutingStrategy},
    logging::{EventTracer, EventMetrics},
    script_engine::CrucibleScriptEngine,
    service_traits::{ScriptEngine, ServiceLifecycle},
    service_types::{ScriptEngineConfig, ExecutionContext, ExecutionOptions, SecurityContext},
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn, error, trace};

#[tokio::test]
async fn test_logging_initialization() {
    // Test basic logging initialization
    let config = CrucibleConfig::default();

    // This should not panic
    let result = config.init_logging();
    assert!(result.is_ok(), "Logging initialization should succeed");

    // Test that we can log messages
    info!("Test info message");
    debug!("Test debug message");
    warn!("Test warning message");
    error!("Test error message");
    trace!("Test trace message");
}

#[tokio::test]
async fn test_event_tracer() {
    let tracer = EventTracer::new("test_component");
    let event_id = "test-event-123";

    // Test event tracing (should not panic even if tracing is disabled)
    tracer.trace_event_start(event_id, "test_event", None);
    tracer.trace_event_complete(event_id, 100, true);
    tracer.trace_event_error(event_id, "test error");
    tracer.trace_routing(event_id, "source", "target", "test_decision");

    // Test with metadata
    let metadata = serde_json::json!({
        "key1": "value1",
        "key2": 42
    });
    tracer.trace_event_start(event_id, "test_event_with_metadata", Some(&metadata));
}

#[tokio::test]
async fn test_event_metrics() {
    let mut metrics = EventMetrics::default();

    // Test recording events
    metrics.record_event(100, true);
    metrics.record_event(200, false);
    metrics.record_event(150, true);

    assert_eq!(metrics.total_events, 3);
    assert_eq!(metrics.successful_events, 2);
    assert_eq!(metrics.failed_events, 1);
    assert_eq!(metrics.total_duration_ms, 450);
    assert_eq!(metrics.avg_duration_ms, 150.0);
    assert_eq!(metrics.min_duration_ms, 100);
    assert_eq!(metrics.max_duration_ms, 200);

    // Test reset
    metrics.reset();
    assert_eq!(metrics.total_events, 0);
    assert_eq!(metrics.successful_events, 0);
    assert_eq!(metrics.failed_events, 0);
}

#[tokio::test]
async fn test_script_engine_logging() {
    let config = CrucibleConfig::default();
    config.init_logging().unwrap();

    let script_config = ScriptEngineConfig::default();
    let mut engine = CrucibleScriptEngine::new(script_config);

    // Test service lifecycle logging
    engine.start().await.unwrap();

    // Test script compilation logging
    let source = r#"
        fn main() {
            println!("Hello, World!");
        }
    "#;

    let result = engine.compile_script(source).await;
    assert!(result.is_ok(), "Script compilation should succeed");

    let compiled_script = result.unwrap();

    // Test script execution logging
    let execution_context = ExecutionContext {
        execution_id: "test-execution-123".to_string(),
        parameters: std::collections::HashMap::new(),
        security_context: SecurityContext::default(),
        options: ExecutionOptions::default(),
    };

    let execution_result = engine.execute_script(&compiled_script.script_id, execution_context).await;
    assert!(execution_result.is_ok(), "Script execution should succeed");

    // Test metrics
    let stats = engine.get_execution_stats().await;
    assert!(stats.is_ok(), "Getting execution stats should succeed");

    // Note: get_event_metrics is not available in the current API
    // This test validates that execution completes successfully with logging enabled

    engine.stop().await.unwrap();
}

#[tokio::test]
async fn test_event_router_logging() {
    let config = CrucibleConfig::default();
    config.init_logging().unwrap();

    let router_config = crucible_services::event_routing::EventRouterConfig::default();
    let router = Arc::new(EventRouter::new(router_config));

    // Register a mock handler
    let handler = Arc::new(MockEventHandler {
        name: "test_handler".to_string(),
        can_handle_types: vec![EventType::ScriptExecution],
    });

    router.register_handler(handler).await.unwrap();

    // Test event routing logging
    let event = Event::new(
        EventType::ScriptExecution,
        "test_source".to_string(),
        serde_json::json!({"test": "data"}),
    ).with_priority(EventPriority::High);

    let result = router.route_event(event).await;
    assert!(result.is_ok(), "Event routing should succeed");

    let routing_result = result.unwrap();
    assert_eq!(routing_result.decision.strategy, RoutingStrategy::TypeBased);
    assert!(!routing_result.delivery_results.is_empty());

    // Test metrics
    let metrics = router.get_metrics().await;
    assert!(metrics.total_events > 0, "Should have recorded events");
}

#[tokio::test]
async fn test_event_flow_debugger() {
    let debugger = EventFlowDebugger::new("test_component", 100);

    // Test event capture
    let event = Event::new(
        EventType::ScriptExecution,
        "test_source".to_string(),
        serde_json::json!({"test": "data"}),
    );

    let mut context = std::collections::HashMap::new();
    context.insert("key1".to_string(), "value1".to_string());

    debugger.capture_event(&event, ProcessingStage::Received, context.clone()).await;
    debugger.capture_event(&event, ProcessingStage::RoutingDecision, context.clone()).await;
    debugger.capture_event(&event, ProcessingStage::HandlerStarted, context.clone()).await;
    debugger.capture_event(&event, ProcessingStage::Completed, context).await;

    // Test event flow analysis
    let analysis = debugger.analyze_event_flow(&event.id).await;
    assert!(analysis.is_some(), "Should have event flow analysis");

    let analysis = analysis.unwrap();
    assert_eq!(analysis.event_id, event.id);
    assert!(analysis.completed);
    assert_eq!(analysis.errors, 0);

    // Test performance snapshots
    debugger.record_performance_snapshot(10.0, 25.0, 5, 1.0).await;

    let snapshots = debugger.get_recent_performance_snapshots(1).await;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].events_per_second, 10.0);

    // Test error recording
    debugger.record_error("TestError", "Test error message", Some(&event.id), "test_component").await;

    let errors = debugger.get_recent_errors(1).await;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error_type, "TestError");
}

#[tokio::test]
async fn test_system_diagnostics() {
    let config = CrucibleConfig::default();
    config.init_logging().unwrap();

    let router_config = crucible_services::event_routing::EventRouterConfig::default();
    let router = Arc::new(EventRouter::new(router_config));

    let debugger = Arc::new(EventFlowDebugger::new("test_component", 100));

    let collector = SystemDiagnosticsCollector::new(
        "test_system",
        Some(router.clone()),
        debugger,
    );

    // Test diagnostics collection
    let diagnostics = collector.collect_diagnostics().await;
    assert!(diagnostics.is_ok(), "Diagnostics collection should succeed");

    let diagnostics = diagnostics.unwrap();
    assert_eq!(diagnostics.router_status.active_events, 0);
    assert!(diagnostics.component_health.contains_key("test_system"));

    // Test report generation
    let report = collector.generate_report().await;
    assert!(report.is_ok(), "Report generation should succeed");

    let report_str = report.unwrap();
    assert!(report_str.contains("router_status"));
    assert!(report_str.contains("component_health"));
}

#[tokio::test]
async fn test_configuration_integration() {
    // Test loading configuration
    let config = CrucibleConfig::load().unwrap();

    // Test configuration validation
    assert!(config.event_routing.max_event_age_seconds > 0);
    assert!(config.event_routing.max_concurrent_events > 0);

    // Test logging initialization with custom config
    let result = config.init_logging();
    assert!(result.is_ok(), "Custom config logging should initialize");

    // Test configuration summary
    let summary = config.get_summary();
    assert!(summary.contains("logging"));
    assert!(summary.contains("routing"));
    assert!(summary.contains("debug"));

    // Test environment variable handling
    std::env::set_var("CRUCIBLE_LOG_LEVEL", "debug");
    std::env::set_var("CRUCIBLE_DEBUG_EVENTS", "true");

    let env_config = CrucibleConfig::load().unwrap();
    assert_eq!(format!("{:?}", env_config.logging.default_level), "DEBUG");
    assert!(env_config.debugging.enable_event_flow_debug);

    // Clean up environment variables
    std::env::remove_var("CRUCIBLE_LOG_LEVEL");
    std::env::remove_var("CRUCIBLE_DEBUG_EVENTS");
}

#[tokio::test]
async fn test_logging_performance_impact() {
    let config = CrucibleConfig::default();
    config.init_logging().unwrap();

    let tracer = EventTracer::new("performance_test");
    let mut metrics = EventMetrics::default();

    // Measure logging overhead
    let start = std::time::Instant::now();

    for i in 0..1000 {
        let event_id = format!("event-{}", i);
        tracer.trace_event_start(&event_id, "performance_test", None);
        tracer.trace_event_complete(&event_id, 10, true);
        metrics.record_event(10, true);
    }

    let duration = start.elapsed();
    let avg_time_per_event = duration.as_micros() as f64 / 1000.0;

    // Log performance should be reasonable (less than 100 microseconds per event)
    assert!(avg_time_per_event < 100.0, "Logging overhead should be minimal: {}μs per event", avg_time_per_event);

    info!(
        total_events = 1000,
        total_duration_ms = duration.as_millis(),
        avg_time_per_event_us = avg_time_per_event,
        "Logging performance test completed"
    );
}

#[tokio::test]
async fn test_concurrent_logging() {
    let config = CrucibleConfig::default();
    config.init_logging().unwrap();

    let tracer = Arc::new(EventTracer::new("concurrent_test"));
    let mut handles = Vec::new();

    // Test concurrent logging from multiple tasks
    for i in 0..10 {
        let tracer_clone = tracer.clone();
        let handle = tokio::spawn(async move {
            for j in 0..100 {
                let event_id = format!("concurrent-event-{}-{}", i, j);
                tracer_clone.trace_event_start(&event_id, "concurrent_test", None);

                // Small delay to simulate real work
                sleep(Duration::from_micros(10)).await;

                tracer_clone.trace_event_complete(&event_id, 10, true);
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    info!("Concurrent logging test completed successfully");
}

// Mock event handler for testing
struct MockEventHandler {
    name: String,
    can_handle_types: Vec<EventType>,
}

#[async_trait]
impl EventHandler for MockEventHandler {
    fn handler_name(&self) -> &str {
        &self.name
    }

    async fn can_handle(&self, event: &Event) -> bool {
        self.can_handle_types.contains(&event.event_type)
    }

    async fn handle_event(&self, event: Event) -> Result<Event, crucible_services::errors::ServiceError> {
        // Simulate some processing time
        sleep(Duration::from_millis(10)).await;
        Ok(event)
    }

    fn handler_priority(&self) -> EventPriority {
        EventPriority::Normal
    }
}