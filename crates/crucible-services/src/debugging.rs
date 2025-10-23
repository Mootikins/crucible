//! Debugging utilities for event routing and service troubleshooting
//!
//! This module provides comprehensive debugging tools for event flow analysis,
//! performance monitoring, and system diagnostics with minimal overhead.

use super::event_routing::{Event, EventRouter};
use super::errors::ServiceResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

/// Debug event capture for detailed analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugEvent {
    /// Original event
    pub event: Event,
    /// Event capture timestamp
    pub captured_at: chrono::DateTime<chrono::Utc>,
    /// Processing stage when captured
    pub stage: ProcessingStage,
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Processing stages for debugging
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProcessingStage {
    /// Event received
    Received,
    /// Routing decision made
    RoutingDecision,
    /// Handler found
    HandlerFound,
    /// Handler processing started
    HandlerStarted,
    /// Handler processing completed
    HandlerCompleted,
    /// Event delivered
    Delivered,
    /// Event failed
    Failed,
    /// Event completed successfully
    Completed,
}

/// Performance snapshot for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Snapshot timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Events processed per second
    pub events_per_second: f64,
    /// Average processing time
    pub avg_processing_time_ms: f64,
    /// Current active events
    pub active_events: usize,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// Error rate percentage
    pub error_rate_percent: f64,
}

/// System diagnostics information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiagnostics {
    /// Diagnostics timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event router status
    pub router_status: RouterStatus,
    /// Performance metrics
    pub performance: PerformanceSnapshot,
    /// Recent errors
    pub recent_errors: Vec<ErrorSnapshot>,
    /// Component health status
    pub component_health: HashMap<String, ComponentHealth>,
}

/// Router status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterStatus {
    /// Total registered handlers
    pub total_handlers: usize,
    /// Active events count
    pub active_events: usize,
    /// Total events processed
    pub total_events_processed: u64,
    /// Routing history size
    pub routing_history_size: usize,
    /// Is router healthy
    pub is_healthy: bool,
}

/// Error snapshot for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSnapshot {
    /// Error timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Error type
    pub error_type: String,
    /// Error message
    pub error_message: String,
    /// Event ID if applicable
    pub event_id: Option<String>,
    /// Component where error occurred
    pub component: String,
    /// Stack trace if available
    pub stack_trace: Option<String>,
}

/// Component health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Health status
    pub status: HealthStatus,
    /// Last check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// Response time in milliseconds
    pub response_time_ms: Option<u64>,
    /// Additional metrics
    pub metrics: HashMap<String, serde_json::Value>,
}

/// Health status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Event flow debugger for detailed analysis
#[derive(Debug)]
pub struct EventFlowDebugger {
    /// Component name
    component_name: String,
    /// Captured events
    captured_events: Arc<RwLock<VecDeque<DebugEvent>>>,
    /// Performance snapshots
    performance_snapshots: Arc<RwLock<VecDeque<PerformanceSnapshot>>>,
    /// Error snapshots
    error_snapshots: Arc<RwLock<VecDeque<ErrorSnapshot>>>,
    /// Maximum events to retain
    max_retained_events: usize,
    /// Debug enabled flag
    debug_enabled: bool,
}

impl EventFlowDebugger {
    /// Create a new event flow debugger
    pub fn new(component_name: &str, max_retained_events: usize) -> Self {
        Self {
            component_name: component_name.to_string(),
            captured_events: Arc::new(RwLock::new(VecDeque::new())),
            performance_snapshots: Arc::new(RwLock::new(VecDeque::new())),
            error_snapshots: Arc::new(RwLock::new(VecDeque::new())),
            max_retained_events,
            debug_enabled: std::env::var("CRUCIBLE_DEBUG_FLOW")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
        }
    }

    /// Capture an event at a specific processing stage
    pub async fn capture_event(
        &self,
        event: &Event,
        stage: ProcessingStage,
        context: HashMap<String, String>,
    ) {
        if !self.debug_enabled {
            return;
        }

        let debug_event = DebugEvent {
            event: event.clone(),
            captured_at: chrono::Utc::now(),
            stage: stage.clone(),
            context,
        };

        let mut events = self.captured_events.write().await;
        events.push_back(debug_event);

        // Maintain size limit
        while events.len() > self.max_retained_events {
            events.pop_front();
        }

        trace!(
            component = %self.component_name,
            event_id = %event.id,
            stage = ?stage,
            "Event captured for debugging"
        );
    }

    /// Record a performance snapshot
    pub async fn record_performance_snapshot(
        &self,
        events_per_second: f64,
        avg_processing_time_ms: f64,
        active_events: usize,
        error_rate_percent: f64,
    ) {
        if !self.debug_enabled {
            return;
        }

        let snapshot = PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second,
            avg_processing_time_ms,
            active_events,
            memory_usage_bytes: self.estimate_memory_usage().await,
            error_rate_percent,
        };

        let mut snapshots = self.performance_snapshots.write().await;
        snapshots.push_back(snapshot);

        // Maintain size limit
        while snapshots.len() > self.max_retained_events {
            snapshots.pop_front();
        }

        debug!(
            component = %self.component_name,
            events_per_second = events_per_second,
            avg_processing_time_ms = avg_processing_time_ms,
            active_events = active_events,
            "Performance snapshot recorded"
        );
    }

    /// Record an error for debugging
    pub async fn record_error(
        &self,
        error_type: &str,
        error_message: &str,
        event_id: Option<&str>,
        component: &str,
    ) {
        if !self.debug_enabled {
            return;
        }

        let error_snapshot = ErrorSnapshot {
            timestamp: chrono::Utc::now(),
            error_type: error_type.to_string(),
            error_message: error_message.to_string(),
            event_id: event_id.map(|id| id.to_string()),
            component: component.to_string(),
            stack_trace: std::backtrace::Backtrace::capture()
                .to_string()
                .lines()
                .take(10)
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        };

        let mut errors = self.error_snapshots.write().await;
        errors.push_back(error_snapshot);

        // Maintain size limit
        while errors.len() > self.max_retained_events {
            errors.pop_front();
        }

        error!(
            component = %self.component_name,
            error_type = %error_type,
            error_message = %error_message,
            event_id = ?event_id,
            "Error recorded for debugging"
        );
    }

    /// Get captured events for a specific time range
    pub async fn get_events_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<DebugEvent> {
        let events = self.captured_events.read().await;
        events
            .iter()
            .filter(|e| e.captured_at >= start && e.captured_at <= end)
            .cloned()
            .collect()
    }

    /// Get recent performance snapshots
    pub async fn get_recent_performance_snapshots(&self, count: usize) -> Vec<PerformanceSnapshot> {
        let snapshots = self.performance_snapshots.read().await;
        snapshots
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    /// Get recent errors
    pub async fn get_recent_errors(&self, count: usize) -> Vec<ErrorSnapshot> {
        let errors = self.error_snapshots.read().await;
        errors
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    /// Estimate memory usage
    pub async fn estimate_memory_usage(&self) -> u64 {
        let events = self.captured_events.read().await;
        let snapshots = self.performance_snapshots.read().await;
        let errors = self.error_snapshots.read().await;

        // Rough estimation
        (events.len() + snapshots.len() + errors.len()) as u64 * 1024 // 1KB per item estimate
    }

    /// Get event flow analysis
    pub async fn analyze_event_flow(&self, event_id: &str) -> Option<EventFlowAnalysis> {
        let events = self.captured_events.read().await;
        let related_events: Vec<DebugEvent> = events
            .iter()
            .filter(|e| e.event.id == event_id)
            .cloned()
            .collect();

        if related_events.is_empty() {
            return None;
        }

        let mut stages = Vec::new();
        let mut total_duration = Duration::from_millis(0);

        for (i, event) in related_events.iter().enumerate() {
            stages.push(event.stage.clone());
            if i > 0 {
                total_duration += event.captured_at.signed_duration_since(related_events[i-1].captured_at).to_std().unwrap_or_default();
            }
        }

        Some(EventFlowAnalysis {
            event_id: event_id.to_string(),
            stages,
            total_duration_ms: total_duration.as_millis() as u64,
            completed: related_events.last()
                .map(|e| matches!(e.stage, ProcessingStage::Completed))
                .unwrap_or(false),
            errors: related_events.iter()
                .filter(|e| matches!(e.stage, ProcessingStage::Failed))
                .count(),
        })
    }

    /// Clear all debug data
    pub async fn clear_debug_data(&self) {
        let mut events = self.captured_events.write().await;
        let mut snapshots = self.performance_snapshots.write().await;
        let mut errors = self.error_snapshots.write().await;

        events.clear();
        snapshots.clear();
        errors.clear();

        info!(component = %self.component_name, "Debug data cleared");
    }
}

/// Event flow analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFlowAnalysis {
    /// Event ID
    pub event_id: String,
    /// Processing stages
    pub stages: Vec<ProcessingStage>,
    /// Total processing duration
    pub total_duration_ms: u64,
    /// Whether processing completed
    pub completed: bool,
    /// Number of errors encountered
    pub errors: usize,
}

/// System diagnostics collector
pub struct SystemDiagnosticsCollector {
    /// Component name
    pub component_name: String,
    /// Event router reference
    pub event_router: Option<Arc<EventRouter>>,
    /// Event flow debugger
    pub debugger: Arc<EventFlowDebugger>,
}

impl SystemDiagnosticsCollector {
    /// Create a new diagnostics collector
    pub fn new(
        component_name: &str,
        event_router: Option<Arc<EventRouter>>,
        debugger: Arc<EventFlowDebugger>,
    ) -> Self {
        Self {
            component_name: component_name.to_string(),
            event_router,
            debugger,
        }
    }

    /// Collect comprehensive system diagnostics
    pub async fn collect_diagnostics(&self) -> ServiceResult<SystemDiagnostics> {
        info!(component = %self.component_name, "Collecting system diagnostics");

        let router_status = if let Some(router) = &self.event_router {
            RouterStatus {
                total_handlers: 0, // TODO: Get from router
                active_events: router.get_active_events_count().await,
                total_events_processed: 0, // TODO: Get from router metrics
                routing_history_size: router.get_routing_history(None).await.len(),
                is_healthy: true, // TODO: Implement health check
            }
        } else {
            RouterStatus {
                total_handlers: 0,
                active_events: 0,
                total_events_processed: 0,
                routing_history_size: 0,
                is_healthy: false,
            }
        };

        let recent_snapshots = self.debugger.get_recent_performance_snapshots(10).await;
        let performance = recent_snapshots.first().unwrap_or(&PerformanceSnapshot {
            timestamp: chrono::Utc::now(),
            events_per_second: 0.0,
            avg_processing_time_ms: 0.0,
            active_events: 0,
            memory_usage_bytes: 0,
            error_rate_percent: 0.0,
        }).clone();

        let recent_errors = self.debugger.get_recent_errors(50).await;

        let mut component_health = HashMap::new();
        component_health.insert(
            self.component_name.clone(),
            ComponentHealth {
                name: self.component_name.clone(),
                status: if router_status.is_healthy { HealthStatus::Healthy } else { HealthStatus::Unhealthy },
                last_check: chrono::Utc::now(),
                response_time_ms: Some(100), // Mock response time
                metrics: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("active_events".to_string(), serde_json::Value::Number(serde_json::Number::from(router_status.active_events as i64)));
                    map.insert("total_processed".to_string(), serde_json::Value::Number(serde_json::Number::from(router_status.total_events_processed as i64)));
                    map
                },
            },
        );

        Ok(SystemDiagnostics {
            timestamp: chrono::Utc::now(),
            router_status,
            performance,
            recent_errors,
            component_health,
        })
    }

    /// Generate diagnostics report
    pub async fn generate_report(&self) -> ServiceResult<String> {
        let diagnostics = self.collect_diagnostics().await?;
        let report = serde_json::to_string_pretty(&diagnostics)
            .map_err(|e| super::errors::ServiceError::SerializationError(e))?;

        info!(component = %self.component_name, "Diagnostics report generated");
        Ok(report)
    }

    /// Save diagnostics report to file
    pub async fn save_report(&self, file_path: &str) -> ServiceResult<()> {
        let report = self.generate_report().await?;
        tokio::fs::write(file_path, report).await
            .map_err(|e| super::errors::ServiceError::IoError(e))?;

        info!(component = %self.component_name, file_path = %file_path, "Diagnostics report saved");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_routing::{EventType, EventPriority};

    #[tokio::test]
    async fn test_event_flow_debugger() {
        std::env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        debugger.capture_event(
            &event,
            ProcessingStage::Received,
            HashMap::new(),
        ).await;

        let analysis = debugger.analyze_event_flow(&event.id).await;
        assert!(analysis.is_some());
        assert_eq!(analysis.unwrap().event_id, event.id);

        std::env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[tokio::test]
    async fn test_performance_snapshot() {
        std::env::set_var("CRUCIBLE_DEBUG_FLOW", "true");
        let debugger = EventFlowDebugger::new("test_component", 100);

        debugger.record_performance_snapshot(
            10.5,
            25.7,
            5,
            2.1,
        ).await;

        let snapshots = debugger.get_recent_performance_snapshots(1).await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].events_per_second, 10.5);

        std::env::remove_var("CRUCIBLE_DEBUG_FLOW");
    }

    #[test]
    fn test_processing_stage_serialization() {
        let stage = ProcessingStage::Completed;
        let serialized = serde_json::to_string(&stage).unwrap();
        let deserialized: ProcessingStage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(stage, deserialized);
    }
}