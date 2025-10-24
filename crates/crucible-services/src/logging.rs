//! Logging and debugging framework for Crucible services
//!
//! This module provides a lightweight, configurable logging system
//! focused on event routing debugging with minimal performance overhead.

use super::errors::ServiceResult;
use serde::Serialize;
use std::sync::Once;
use tracing::{debug, error, info, trace, Level};

/// Static initialization guard
static INIT: Once = Once::new();

/// Logging configuration
#[derive(Debug, Clone, Serialize)]
pub struct LoggingConfig {
    /// Default log level
    #[serde(skip_serializing)]
    pub default_level: Level,
    /// Component-specific log levels
    #[serde(skip_serializing)]
    pub component_levels: Vec<(String, Level)>,
    /// Whether to include timestamps
    pub include_timestamps: bool,
    /// Whether to include target/module
    pub include_target: bool,
    /// Whether to use ANSI colors
    pub use_ansi: bool,
    /// Filter for specific components
    pub component_filter: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            default_level: Level::INFO,
            component_levels: vec![
                ("crucible_services::script_engine".to_string(), Level::DEBUG),
                ("crucible_services::event_routing".to_string(), Level::TRACE),
            ],
            include_timestamps: true,
            include_target: true,
            use_ansi: true,
            component_filter: None,
        }
    }
}

/// Initialize the logging system
pub fn init_logging(config: LoggingConfig) -> ServiceResult<()> {
    INIT.call_once(|| {
        let filter_str = build_filter_string(&config);

        let subscriber = tracing_subscriber::fmt()
            .with_target(config.include_target)
            .with_ansi(config.use_ansi);

        // Set the filter using environment variable
        std::env::set_var("RUST_LOG", &filter_str);

        subscriber.init();
    });

    info!("Logging system initialized with level: {:?}", config.default_level);
    Ok(())
}

/// Build filter string from configuration
pub fn build_filter_string(config: &LoggingConfig) -> String {
    let mut filter_str = format!("{}=warn", env!("CARGO_PKG_NAME"));

    // Add default level
    filter_str.push_str(&format!(",crucible_services={}", config.default_level));

    // Add component-specific levels
    for (component, level) in &config.component_levels {
        filter_str.push_str(&format!(",{}={}", component, level));
    }

    // Add component filter if specified
    if let Some(filter) = &config.component_filter {
        filter_str.push_str(&format!(",{}", filter));
    }

    filter_str
}

/// Event routing tracer for debugging event flow
#[derive(Debug)]
pub struct EventTracer {
    pub component_name: String,
    pub enabled: bool,
}

impl EventTracer {
    /// Create a new event tracer
    pub fn new(component_name: &str) -> Self {
        let enabled = std::env::var("CRUCIBLE_EVENT_TRACE")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        Self {
            component_name: component_name.to_string(),
            enabled,
        }
    }

    /// Trace event start
    pub fn trace_event_start(&self, event_id: &str, event_type: &str, metadata: Option<&serde_json::Value>) {
        if !self.enabled {
            return;
        }

        let meta_str = metadata
            .map(|m| serde_json::to_string(m).unwrap_or_default())
            .unwrap_or_default();

        trace!(
            component = %self.component_name,
            event_id = %event_id,
            event_type = %event_type,
            metadata = %meta_str,
            "Event processing started"
        );
    }

    /// Trace event completion
    pub fn trace_event_complete(&self, event_id: &str, duration_ms: u64, success: bool) {
        if !self.enabled {
            return;
        }

        trace!(
            component = %self.component_name,
            event_id = %event_id,
            duration_ms = duration_ms,
            success = success,
            "Event processing completed"
        );
    }

    /// Trace event error
    pub fn trace_event_error(&self, event_id: &str, error: &str) {
        if !self.enabled {
            return;
        }

        error!(
            component = %self.component_name,
            event_id = %event_id,
            error = %error,
            "Event processing failed"
        );
    }

    /// Trace routing decision
    pub fn trace_routing(&self, event_id: &str, from: &str, to: &str, decision: &str) {
        if !self.enabled {
            return;
        }

        debug!(
            component = %self.component_name,
            event_id = %event_id,
            from = %from,
            to = %to,
            decision = %decision,
            "Event routing decision"
        );
    }
}

/// Performance metrics for event processing
#[derive(Debug, Clone, Default)]
pub struct EventMetrics {
    pub total_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u64,
    pub min_duration_ms: u64,
}

impl EventMetrics {
    /// Record a completed event
    pub fn record_event(&mut self, duration_ms: u64, success: bool) {
        self.total_events += 1;
        self.total_duration_ms += duration_ms;

        if success {
            self.successful_events += 1;
        } else {
            self.failed_events += 1;
        }

        // Update min/max
        if self.min_duration_ms == 0 || duration_ms < self.min_duration_ms {
            self.min_duration_ms = duration_ms;
        }
        if duration_ms > self.max_duration_ms {
            self.max_duration_ms = duration_ms;
        }

        // Update average
        self.avg_duration_ms = self.total_duration_ms as f64 / self.total_events as f64;
    }

    /// Log current metrics
    pub fn log_metrics(&self, component: &str) {
        info!(
            component = %component,
            total_events = %self.total_events,
            success_rate = %((self.successful_events as f64 / self.total_events as f64) * 100.0),
            avg_duration_ms = %self.avg_duration_ms,
            max_duration_ms = %self.max_duration_ms,
            min_duration_ms = %self.min_duration_ms,
            "Event processing metrics"
        );
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Utility macro for structured event logging
#[macro_export]
macro_rules! trace_event {
    ($tracer:expr, $event_id:expr, $event_type:expr $(,)?) => {
        $tracer.trace_event_start($event_id, $event_type, None);
    };
    ($tracer:expr, $event_id:expr, $event_type:expr, $metadata:expr $(,)?) => {
        $tracer.trace_event_start($event_id, $event_type, $metadata.as_ref());
    };
}

/// Utility macro for performance timing
#[macro_export]
macro_rules! time_event {
    ($tracer:expr, $event_id:expr, $block:block) => {{
        let start = std::time::Instant::now();
        let result: Result<_, Box<dyn std::error::Error + Send + Sync>> = (|| $block)();
        let duration = start.elapsed().as_millis() as u64;

        match result {
            Ok(value) => {
                $tracer.trace_event_complete($event_id, duration, true);
                Ok(value)
            }
            Err(e) => {
                $tracer.trace_event_error($event_id, &e.to_string());
                Err(e)
            }
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.default_level, Level::INFO);
        assert_eq!(config.component_levels.len(), 2);
        assert!(config.include_timestamps);
        assert!(config.include_target);
        assert!(config.use_ansi);
    }

    #[test]
    fn test_event_tracer_creation() {
        let tracer = EventTracer::new("test_component");
        assert_eq!(tracer.component_name, "test_component");
    }

    #[test]
    fn test_event_metrics() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(100, true);
        metrics.record_event(200, false);

        assert_eq!(metrics.total_events, 2);
        assert_eq!(metrics.successful_events, 1);
        assert_eq!(metrics.failed_events, 1);
        assert_eq!(metrics.total_duration_ms, 300);
        assert_eq!(metrics.avg_duration_ms, 150.0);
        assert_eq!(metrics.min_duration_ms, 100);
        assert_eq!(metrics.max_duration_ms, 200);
    }
}