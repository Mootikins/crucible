//! Comprehensive unit tests for the logging framework
//!
//! Tests for logging configuration, event tracing, metrics collection,
//! and performance characteristics of the logging system.

use crucible_services::logging::*;
use crucible_services::{trace_event, time_event};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::Level;


/// Test logging configuration creation and defaults
#[cfg(test)]
mod logging_config_tests {
    use super::*;

    #[test]
    fn test_logging_config_default_values() {
        let config = LoggingConfig::default();

        assert_eq!(config.default_level, Level::INFO);
        assert_eq!(config.component_levels.len(), 2);
        assert!(config.include_timestamps);
        assert!(config.include_target);
        assert!(config.use_ansi);
        assert!(config.component_filter.is_none());
    }

    #[test]
    fn test_logging_config_specific_component_levels() {
        let config = LoggingConfig::default();

        // Check specific component levels are set
        let script_engine_level = config.component_levels
            .iter()
            .find(|(comp, _)| comp == "crucible_services::script_engine")
            .map(|(_, level)| *level);
        assert_eq!(script_engine_level, Some(Level::DEBUG));

        let event_routing_level = config.component_levels
            .iter()
            .find(|(comp, _)| comp == "crucible_services::event_routing")
            .map(|(_, level)| *level);
        assert_eq!(event_routing_level, Some(Level::TRACE));
    }

    #[test]
    fn test_logging_config_custom_creation() {
        let config = LoggingConfig {
            default_level: Level::DEBUG,
            component_levels: vec![
                ("test_component".to_string(), Level::TRACE),
            ],
            include_timestamps: false,
            include_target: false,
            use_ansi: false,
            component_filter: Some("test_component".to_string()),
        };

        assert_eq!(config.default_level, Level::DEBUG);
        assert_eq!(config.component_levels.len(), 1);
        assert!(!config.include_timestamps);
        assert!(!config.include_target);
        assert!(!config.use_ansi);
        assert!(config.component_filter.is_some());
    }

    #[test]
    fn test_logging_config_clone() {
        let config = LoggingConfig::default();
        let cloned = config.clone();

        assert_eq!(config.default_level, cloned.default_level);
        assert_eq!(config.component_levels, cloned.component_levels);
        assert_eq!(config.include_timestamps, cloned.include_timestamps);
        assert_eq!(config.include_target, cloned.include_target);
        assert_eq!(config.use_ansi, cloned.use_ansi);
        assert_eq!(config.component_filter, cloned.component_filter);
    }

    #[test]
    fn test_logging_config_debug_format() {
        let config = LoggingConfig::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("LoggingConfig"));
        assert!(debug_str.contains("default_level"));
        assert!(debug_str.contains("component_levels"));
    }
}

/// Test event tracer functionality
#[cfg(test)]
mod event_tracer_tests {
    use super::*;
    use std::env;

    #[test]
    fn test_event_tracer_creation_with_env_disabled() {
        // Ensure environment variable is not set
        env::remove_var("CRUCIBLE_EVENT_TRACE");

        let tracer = EventTracer::new("test_component");
        assert_eq!(tracer.component_name, "test_component");
        assert!(!tracer.enabled);
    }

    #[test]
    fn test_event_tracer_creation_with_env_enabled() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");

        let tracer = EventTracer::new("test_component");
        assert_eq!(tracer.component_name, "test_component");
        assert!(tracer.enabled);

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_event_tracer_creation_with_env_invalid() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "invalid");

        let tracer = EventTracer::new("test_component");
        assert_eq!(tracer.component_name, "test_component");
        assert!(!tracer.enabled);

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_event_tracer_trace_event_start_disabled() {
        env::remove_var("CRUCIBLE_EVENT_TRACE");
        let tracer = EventTracer::new("test_component");

        // Should not panic when disabled
        tracer.trace_event_start("test_id", "test_type", None);
        tracer.trace_event_start("test_id", "test_type", Some(&serde_json::json!({"test": "data"})));
    }

    #[test]
    fn test_event_tracer_trace_event_complete_disabled() {
        env::remove_var("CRUCIBLE_EVENT_TRACE");
        let tracer = EventTracer::new("test_component");

        // Should not panic when disabled
        tracer.trace_event_complete("test_id", 100, true);
        tracer.trace_event_complete("test_id", 200, false);
    }

    #[test]
    fn test_event_tracer_trace_event_error_disabled() {
        env::remove_var("CRUCIBLE_EVENT_TRACE");
        let tracer = EventTracer::new("test_component");

        // Should not panic when disabled
        tracer.trace_event_error("test_id", "test error");
    }

    #[test]
    fn test_event_tracer_trace_routing_disabled() {
        env::remove_var("CRUCIBLE_EVENT_TRACE");
        let tracer = EventTracer::new("test_component");

        // Should not panic when disabled
        tracer.trace_routing("test_id", "source", "target", "decision");
    }

    #[test]
    fn test_event_tracer_metadata_serialization() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        let metadata = serde_json::json!({
            "key1": "value1",
            "key2": 42,
            "nested": {"inner": "data"}
        });

        // Should not panic with valid metadata
        tracer.trace_event_start("test_id", "test_type", Some(&metadata));

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_event_tracer_invalid_metadata() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        // Create a value that might cause serialization issues
        let metadata = serde_json::json!({
            "data": std::f64::NAN // NaN might cause issues in some contexts
        });

        // Should handle gracefully
        tracer.trace_event_start("test_id", "test_type", Some(&metadata));

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_event_tracer_debug_format() {
        let tracer = EventTracer::new("test_component");
        let debug_str = format!("{:?}", tracer);

        assert!(debug_str.contains("EventTracer"));
        assert!(debug_str.contains("component_name"));
        assert!(debug_str.contains("enabled"));
    }
}

/// Test event metrics collection and calculation
#[cfg(test)]
mod event_metrics_tests {
    use super::*;

    #[test]
    fn test_event_metrics_default_values() {
        let metrics = EventMetrics::default();

        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.successful_events, 0);
        assert_eq!(metrics.failed_events, 0);
        assert_eq!(metrics.total_duration_ms, 0);
        assert_eq!(metrics.avg_duration_ms, 0.0);
        assert_eq!(metrics.max_duration_ms, 0);
        assert_eq!(metrics.min_duration_ms, 0);
    }

    #[test]
    fn test_event_metrics_single_successful_event() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(100, true);

        assert_eq!(metrics.total_events, 1);
        assert_eq!(metrics.successful_events, 1);
        assert_eq!(metrics.failed_events, 0);
        assert_eq!(metrics.total_duration_ms, 100);
        assert_eq!(metrics.avg_duration_ms, 100.0);
        assert_eq!(metrics.max_duration_ms, 100);
        assert_eq!(metrics.min_duration_ms, 100);
    }

    #[test]
    fn test_event_metrics_single_failed_event() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(200, false);

        assert_eq!(metrics.total_events, 1);
        assert_eq!(metrics.successful_events, 0);
        assert_eq!(metrics.failed_events, 1);
        assert_eq!(metrics.total_duration_ms, 200);
        assert_eq!(metrics.avg_duration_ms, 200.0);
        assert_eq!(metrics.max_duration_ms, 200);
        assert_eq!(metrics.min_duration_ms, 200);
    }

    #[test]
    fn test_event_metrics_multiple_events() {
        let mut metrics = EventMetrics::default();

        // Record multiple events with different outcomes
        metrics.record_event(100, true);
        metrics.record_event(200, false);
        metrics.record_event(50, true);
        metrics.record_event(300, true);

        assert_eq!(metrics.total_events, 4);
        assert_eq!(metrics.successful_events, 3);
        assert_eq!(metrics.failed_events, 1);
        assert_eq!(metrics.total_duration_ms, 650);
        assert_eq!(metrics.avg_duration_ms, 162.5);
        assert_eq!(metrics.max_duration_ms, 300);
        assert_eq!(metrics.min_duration_ms, 50);
    }

    #[test]
    fn test_event_metrics_min_max_updates() {
        let mut metrics = EventMetrics::default();

        metrics.record_event(100, true);
        assert_eq!(metrics.min_duration_ms, 100);
        assert_eq!(metrics.max_duration_ms, 100);

        metrics.record_event(50, true);
        assert_eq!(metrics.min_duration_ms, 50);
        assert_eq!(metrics.max_duration_ms, 100);

        metrics.record_event(200, false);
        assert_eq!(metrics.min_duration_ms, 50);
        assert_eq!(metrics.max_duration_ms, 200);
    }

    #[test]
    fn test_event_metrics_zero_duration() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(0, true);

        assert_eq!(metrics.total_events, 1);
        assert_eq!(metrics.total_duration_ms, 0);
        assert_eq!(metrics.avg_duration_ms, 0.0);
        assert_eq!(metrics.max_duration_ms, 0);
        assert_eq!(metrics.min_duration_ms, 0);
    }

    #[test]
    fn test_event_metrics_large_values() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(u64::MAX / 2, true);

        assert_eq!(metrics.total_events, 1);
        assert_eq!(metrics.total_duration_ms, u64::MAX / 2);
        assert_eq!(metrics.avg_duration_ms, (u64::MAX / 2) as f64);
    }

    #[test]
    fn test_event_metrics_clone() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(100, true);
        metrics.record_event(200, false);

        let cloned = metrics.clone();
        assert_eq!(metrics.total_events, cloned.total_events);
        assert_eq!(metrics.successful_events, cloned.successful_events);
        assert_eq!(metrics.failed_events, cloned.failed_events);
        assert_eq!(metrics.total_duration_ms, cloned.total_duration_ms);
        assert_eq!(metrics.avg_duration_ms, cloned.avg_duration_ms);
        assert_eq!(metrics.max_duration_ms, cloned.max_duration_ms);
        assert_eq!(metrics.min_duration_ms, cloned.min_duration_ms);
    }

    #[test]
    fn test_event_metrics_reset() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(100, true);
        metrics.record_event(200, false);

        metrics.reset();

        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.successful_events, 0);
        assert_eq!(metrics.failed_events, 0);
        assert_eq!(metrics.total_duration_ms, 0);
        assert_eq!(metrics.avg_duration_ms, 0.0);
        assert_eq!(metrics.max_duration_ms, 0);
        assert_eq!(metrics.min_duration_ms, 0);
    }

    #[test]
    fn test_event_metrics_debug_format() {
        let metrics = EventMetrics::default();
        let debug_str = format!("{:?}", metrics);

        assert!(debug_str.contains("EventMetrics"));
        assert!(debug_str.contains("total_events"));
        assert!(debug_str.contains("successful_events"));
        assert!(debug_str.contains("failed_events"));
    }

    #[test]
    fn test_event_metrics_average_calculation_precision() {
        let mut metrics = EventMetrics::default();

        // Test precision with fractional average
        metrics.record_event(1, true);
        metrics.record_event(2, true);

        assert_eq!(metrics.avg_duration_ms, 1.5);

        metrics.record_event(1, false);
        assert_eq!(metrics.avg_duration_ms, 4.0 / 3.0);
    }
}

/// Test logging initialization and configuration
#[cfg(test)]
mod logging_initialization_tests {
    use super::*;

    #[test]
    fn test_build_filter_string_default() {
        let config = LoggingConfig::default();
        let filter_str = build_filter_string(&config);

        assert!(filter_str.contains("warn"));
        assert!(filter_str.contains("crucible_services=info"));
        assert!(filter_str.contains("crucible_services::script_engine=debug"));
        assert!(filter_str.contains("crucible_services::event_routing=trace"));
    }

    #[test]
    fn test_build_filter_string_custom_level() {
        let config = LoggingConfig {
            default_level: Level::DEBUG,
            component_levels: vec![],
            include_timestamps: true,
            include_target: true,
            use_ansi: true,
            component_filter: None,
        };

        let filter_str = build_filter_string(&config);
        assert!(filter_str.contains("crucible_services=debug"));
    }

    #[test]
    fn test_build_filter_string_with_component_filter() {
        let config = LoggingConfig {
            default_level: Level::INFO,
            component_levels: vec![],
            include_timestamps: true,
            include_target: true,
            use_ansi: true,
            component_filter: Some("test_component=trace".to_string()),
        };

        let filter_str = build_filter_string(&config);
        assert!(filter_str.contains("test_component=trace"));
    }

    #[test]
    fn test_build_filter_string_multiple_components() {
        let config = LoggingConfig {
            default_level: Level::INFO,
            component_levels: vec![
                ("component1".to_string(), Level::DEBUG),
                ("component2".to_string(), Level::ERROR),
            ],
            include_timestamps: true,
            include_target: true,
            use_ansi: true,
            component_filter: None,
        };

        let filter_str = build_filter_string(&config);
        assert!(filter_str.contains("component1=debug"));
        assert!(filter_str.contains("component2=error"));
    }

    #[test]
    fn test_init_logging_multiple_calls() {
        let config = LoggingConfig::default();

        // Should not panic on multiple calls due to Once guard
        let result1 = init_logging(config.clone());
        let result2 = init_logging(config.clone());

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}

/// Test logging macros and utilities
#[cfg(test)]
mod logging_macros_tests {
    use super::*;
    use std::env;

    #[test]
    fn test_trace_event_macro_without_metadata() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        // Should not panic
        trace_event!(tracer, "test_id", "test_type");

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_trace_event_macro_with_metadata() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");
        let metadata = serde_json::json!({"key": "value"});

        // Should not panic
        trace_event!(tracer, "test_id", "test_type", Some(metadata));

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_time_event_macro_success() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        let result: Result<String, Box<dyn std::error::Error + Send + Sync>> = time_event!(tracer, "test_id", {
            Ok("success".to_string())
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_time_event_macro_error() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        let result: Result<String, Box<dyn std::error::Error + Send + Sync>> = time_event!(tracer, "test_id", {
            Err("test error".into())
        });

        assert!(result.is_err());

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_time_event_macro_timing_accuracy() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = EventTracer::new("test_component");

        let start = std::time::Instant::now();
        let _result: Result<(), Box<dyn std::error::Error + Send + Sync>> = time_event!(tracer, "test_id", {
            std::thread::sleep(Duration::from_millis(10));
            Ok(())
        });
        let elapsed = start.elapsed();

        // Should have taken at least 10ms (with some tolerance)
        assert!(elapsed.as_millis() >= 10);

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }
}

/// Test performance characteristics
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_event_tracer_performance_disabled() {
        env::remove_var("CRUCIBLE_EVENT_TRACE");
        let tracer = EventTracer::new("test_component");

        let start = Instant::now();
        for i in 0..10000 {
            tracer.trace_event_start(&i.to_string(), "test_type", None);
            tracer.trace_event_complete(&i.to_string(), 100, true);
        }
        let elapsed = start.elapsed();

        // Should be very fast when disabled (< 10ms for 20k calls)
        assert!(elapsed.as_millis() < 10);
    }

    #[test]
    fn test_event_metrics_performance() {
        let mut metrics = EventMetrics::default();

        let start = Instant::now();
        for i in 0..10000 {
            metrics.record_event(i % 1000, i % 2 == 0);
        }
        let elapsed = start.elapsed();

        // Should be fast (< 50ms for 10k calls)
        assert!(elapsed.as_millis() < 50);

        // Verify correctness
        assert_eq!(metrics.total_events, 10000);
        assert_eq!(metrics.successful_events, 5000);
        assert_eq!(metrics.failed_events, 5000);
    }

    #[test]
    fn test_concurrent_event_metrics() {
        use std::sync::Arc;
        use std::thread;

        let metrics = Arc::new(std::sync::Mutex::new(EventMetrics::default()));
        let mut handles = vec![];

        for thread_id in 0..10 {
            let metrics_clone = metrics.clone();
            let handle = thread::spawn(move || {
                for i in 0..1000 {
                    let mut m = metrics_clone.lock().unwrap();
                    m.record_event((thread_id * 1000 + i) % 100, i % 3 != 0);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let final_metrics = metrics.lock().unwrap();
        assert_eq!(final_metrics.total_events, 10000);
    }
}

/// Test thread safety
#[cfg(test)]
mod thread_safety_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_event_tracer_thread_safety() {
        env::set_var("CRUCIBLE_EVENT_TRACE", "true");
        let tracer = Arc::new(EventTracer::new("test_component"));
        let mut handles = vec![];

        for i in 0..10 {
            let tracer_clone = tracer.clone();
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let event_id = format!("event_{}_{}", i, j);
                    tracer_clone.trace_event_start(&event_id, "test_type", None);
                    tracer_clone.trace_event_complete(&event_id, 100, true);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        env::remove_var("CRUCIBLE_EVENT_TRACE");
    }

    #[test]
    fn test_event_metrics_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventMetrics>();
    }

    #[test]
    fn test_logging_config_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LoggingConfig>();
    }
}