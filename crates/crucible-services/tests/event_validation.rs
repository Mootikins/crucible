//! Event Validation and Verification Utilities
//!
//! This module provides comprehensive validation and verification utilities for testing
//! event-driven service communication patterns.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use crucible_services::{
    events::{
        core::{DaemonEvent, EventType, EventPriority, EventSource},
    },
};

/// Event validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metrics: ValidationMetrics,
}

/// Validation metrics collected during validation
#[derive(Debug, Clone, Default)]
pub struct ValidationMetrics {
    pub total_events: usize,
    pub valid_events: usize,
    pub invalid_events: usize,
    pub events_by_type: HashMap<String, usize>,
    pub events_by_priority: HashMap<EventPriority, usize>,
    pub events_by_source: HashMap<String, usize>,
    pub correlation_groups: HashMap<String, usize>,
    pub average_event_size: f64,
}

/// Event validator with configurable rules
pub struct EventValidator {
    rules: Vec<ValidationRule>,
    strict_mode: bool,
}

/// Individual validation rule
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub name: String,
    pub description: String,
    pub validator: Box<dyn Fn(&DaemonEvent) -> ValidationResult + Send + Sync>,
}

impl EventValidator {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            strict_mode: false,
        }
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn add_rule(mut self, rule: ValidationRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn validate_events(&self, events: &[DaemonEvent]) -> ValidationResult {
        let mut total_errors = Vec::new();
        let mut total_warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        metrics.total_events = events.len();

        for event in events {
            let mut event_valid = true;
            let mut event_errors = Vec::new();
            let mut event_warnings = Vec::new();

            // Apply all validation rules
            for rule in &self.rules {
                let result = (rule.validator)(event);
                if !result.is_valid {
                    event_valid = false;
                    event_errors.extend(result.errors);
                }
                event_warnings.extend(result.warnings);
            }

            // Update metrics
            if event_valid {
                metrics.valid_events += 1;
            } else {
                metrics.invalid_events += 1;
                if self.strict_mode {
                    total_errors.extend(event_errors);
                } else {
                    total_warnings.extend(event_errors);
                }
            }
            total_warnings.extend(event_warnings);

            // Collect event statistics
            self.update_event_metrics(&mut metrics, event);
        }

        ValidationResult {
            is_valid: total_errors.is_empty() && (metrics.invalid_events == 0 || !self.strict_mode),
            errors: total_errors,
            warnings: total_warnings,
            metrics,
        }
    }

    fn update_event_metrics(&self, metrics: &mut ValidationMetrics, event: &DaemonEvent) {
        // Count by event type
        let event_type_str = match &event.event_type {
            EventType::Custom(s) => s.clone(),
            EventType::Mcp(_) => "mcp".to_string(),
            EventType::Database(_) => "database".to_string(),
            EventType::Service(_) => "service".to_string(),
            EventType::System(_) => "system".to_string(),
        };
        *metrics.events_by_type.entry(event_type_str).or_insert(0) += 1;

        // Count by priority
        *metrics.events_by_priority.entry(event.priority).or_insert(0) += 1;

        // Count by source
        let source_str = match &event.source {
            EventSource::Service(s) => s.clone(),
            EventSource::System => "system".to_string(),
            EventSource::External => "external".to_string(),
        };
        *metrics.events_by_source.entry(source_str).or_insert(0) += 1;

        // Count correlation groups
        if let Some(correlation_id) = &event.correlation_id {
            *metrics.correlation_groups.entry(correlation_id.clone()).or_insert(0) += 1;
        }

        // Calculate average event size (rough estimate)
        let event_size = std::mem::size_of::<DaemonEvent>() +
                       event.payload.as_ref().map_or(0, |p| p.estimated_size());
        metrics.average_event_size = (metrics.average_event_size * (metrics.total_events - 1) as f64 + event_size as f64) / metrics.total_events as f64;
    }
}

impl Default for EventValidator {
    fn default() -> Self {
        let validator = Self::new();
        validator
            .add_rule(ValidationRule {
                name: "event_id_required".to_string(),
                description: "Events must have a valid UUID".to_string(),
                validator: Box::new(|event| {
                    if event.id == Uuid::nil() {
                        ValidationResult {
                            is_valid: false,
                            errors: vec!["Event ID cannot be nil UUID".to_string()],
                            warnings: vec![],
                            metrics: ValidationMetrics::default(),
                        }
                    } else {
                        ValidationResult {
                            is_valid: true,
                            errors: vec![],
                            warnings: vec![],
                            metrics: ValidationMetrics::default(),
                        }
                    }
                }),
            })
            .add_rule(ValidationRule {
                name: "timestamp_required".to_string(),
                description: "Events must have a valid timestamp".to_string(),
                validator: Box::new(|event| {
                    let now = chrono::Utc::now();
                    let event_time = event.created_at;
                    let time_diff = (now - event_time).num_seconds();

                    if time_diff.abs() > 300 { // 5 minutes tolerance
                        ValidationResult {
                            is_valid: false,
                            errors: vec![format!("Event timestamp is too far from current time: {} seconds", time_diff)],
                            warnings: vec![],
                            metrics: ValidationMetrics::default(),
                        }
                    } else {
                        ValidationResult {
                            is_valid: true,
                            errors: vec![],
                            warnings: vec![],
                            metrics: ValidationMetrics::default(),
                        }
                    }
                }),
            })
            .add_rule(ValidationRule {
                name: "priority_valid".to_string(),
                description: "Events must have valid priority".to_string(),
                validator: Box::new(|event| {
                    match event.priority {
                        EventPriority::Low | EventPriority::Normal | EventPriority::High | EventPriority::Critical => {
                            ValidationResult {
                                is_valid: true,
                                errors: vec![],
                                warnings: vec![],
                                metrics: ValidationMetrics::default(),
                            }
                        }
                    }
                }),
            })
            .add_rule(ValidationRule {
                name: "payload_not_empty".to_string(),
                description: "Events should have non-empty payload".to_string(),
                validator: Box::new(|event| {
                    let payload_empty = match &event.payload {
                        crucible_services::events::core::EventPayload::Json(value) => {
                            value.as_object().map_or(true, |obj| obj.is_empty())
                        }
                        crucible_services::events::core::EventPayload::Binary(data) => data.is_empty(),
                        crucible_services::events::core::EventPayload::Text(text) => text.is_empty(),
                        crucible_services::events::core::EventPayload::Empty => true,
                    };

                    if payload_empty {
                        ValidationResult {
                            is_valid: true, // Warning only, not an error
                            errors: vec![],
                            warnings: vec!["Event payload is empty".to_string()],
                            metrics: ValidationMetrics::default(),
                        }
                    } else {
                        ValidationResult {
                            is_valid: true,
                            errors: vec![],
                            warnings: vec![],
                            metrics: ValidationMetrics::default(),
                        }
                    }
                }),
            })
    }
}

/// Event flow validator for checking sequences and patterns
pub struct EventFlowValidator {
    expected_patterns: Vec<EventPattern>,
}

/// Pattern definition for event sequences
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub name: String,
    pub description: String,
    pub event_types: Vec<String>,
    pub optional_events: HashSet<String>,
    pub allowed_sources: HashSet<String>,
    pub correlation_required: bool,
    pub timeout_ms: u64,
}

impl EventFlowValidator {
    pub fn new() -> Self {
        Self {
            expected_patterns: Vec::new(),
        }
    }

    pub fn add_pattern(mut self, pattern: EventPattern) -> Self {
        self.expected_patterns.push(pattern);
        self
    }

    pub fn validate_flow(&self, events: &[DaemonEvent]) -> FlowValidationResult {
        let mut results = Vec::new();

        for pattern in &self.expected_patterns {
            let result = self.validate_pattern(events, pattern);
            results.push(result);
        }

        FlowValidationResult {
            patterns: results,
            overall_success: results.iter().all(|r| r.success),
        }
    }

    fn validate_pattern(&self, events: &[DaemonEvent], pattern: &EventPattern) -> PatternValidationResult {
        let mut matched_events = Vec::new();
        let mut current_index = 0;

        // Filter events by allowed sources
        let filtered_events: Vec<_> = events.iter()
            .filter(|e| {
                pattern.allowed_sources.is_empty() ||
                pattern.allowed_sources.contains(&e.source.to_string())
            })
            .collect();

        for expected_type in &pattern.event_types {
            let mut found = false;

            for (i, event) in filtered_events.iter().enumerate().skip(current_index) {
                if self.event_matches_type(event, expected_type) {
                    matched_events.push((i, event.clone()));
                    current_index = i + 1;
                    found = true;
                    break;
                }
            }

            if !found && !pattern.optional_events.contains(expected_type) {
                return PatternValidationResult {
                    pattern_name: pattern.name.clone(),
                    success: false,
                    matched_events: matched_events.clone(),
                    missing_events: vec![expected_type.clone()],
                    unexpected_events: vec![],
                    timing_violations: vec![],
                };
            }
        }

        PatternValidationResult {
            pattern_name: pattern.name.clone(),
            success: true,
            matched_events,
            missing_events: vec![],
            unexpected_events: vec![],
            timing_violations: vec![],
        }
    }

    fn event_matches_type(&self, event: &DaemonEvent, expected_type: &str) -> bool {
        match &event.event_type {
            EventType::Custom(event_type) => event_type.contains(expected_type),
            EventType::Mcp(_) => expected_type.contains("mcp"),
            EventType::Database(_) => expected_type.contains("database"),
            EventType::Service(_) => expected_type.contains("service"),
            EventType::System(_) => expected_type.contains("system"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlowValidationResult {
    pub patterns: Vec<PatternValidationResult>,
    pub overall_success: bool,
}

#[derive(Debug, Clone)]
pub struct PatternValidationResult {
    pub pattern_name: String,
    pub success: bool,
    pub matched_events: Vec<(usize, DaemonEvent)>,
    pub missing_events: Vec<String>,
    pub unexpected_events: Vec<String>,
    pub timing_violations: Vec<String>,
}

/// Predefined event patterns for common workflows
pub struct EventPatterns;

impl EventPatterns {
    pub fn script_execution_pattern() -> EventPattern {
        EventPattern {
            name: "script_execution_workflow".to_string(),
            description: "Complete script execution workflow".to_string(),
            event_types: vec![
                "script_execution_request".to_string(),
                "script_execution_started".to_string(),
                "script_execution_completed".to_string(),
            ],
            optional_events: vec![
                "script_execution_progress".to_string(),
                "script_execution_error".to_string(),
            ].into_iter().collect(),
            allowed_sources: vec!["script-engine".to_string()].into_iter().collect(),
            correlation_required: true,
            timeout_ms: 30000, // 30 seconds
        }
    }

    pub fn document_crud_pattern() -> EventPattern {
        EventPattern {
            name: "document_crud_workflow".to_string(),
            description: "Document CRUD operations workflow".to_string(),
            event_types: vec![
                "document_creation_request".to_string(),
                "document_created".to_string(),
            ],
            optional_events: vec![
                "document_updated".to_string(),
                "document_deleted".to_string(),
                "document_read".to_string(),
            ].into_iter().collect(),
            allowed_sources: vec!["datastore".to_string()].into_iter().collect(),
            correlation_required: true,
            timeout_ms: 10000, // 10 seconds
        }
    }

    pub fn inference_pattern() -> EventPattern {
        EventPattern {
            name: "inference_workflow".to_string(),
            description: "Inference request workflow".to_string(),
            event_types: vec![
                "inference_request".to_string(),
                "inference_started".to_string(),
                "inference_completed".to_string(),
            ],
            optional_events: vec![
                "cache_hit".to_string(),
                "cache_miss".to_string(),
                "inference_error".to_string(),
            ].into_iter().collect(),
            allowed_sources: vec!["inference-engine".to_string()].into_iter().collect(),
            correlation_required: true,
            timeout_ms: 60000, // 60 seconds
        }
    }

    pub fn mcp_session_pattern() -> EventPattern {
        EventPattern {
            name: "mcp_session_workflow".to_string(),
            description: "MCP session lifecycle workflow".to_string(),
            event_types: vec![
                "session_created".to_string(),
                "tool_registered".to_string(),
                "tool_executed".to_string(),
                "session_closed".to_string(),
            ],
            optional_events: vec![
                "tool_error".to_string(),
                "protocol_error".to_string(),
            ].into_iter().collect(),
            allowed_sources: vec!["mcp-gateway".to_string()].into_iter().collect(),
            correlation_required: true,
            timeout_ms: 45000, // 45 seconds
        }
    }

    pub fn cross_service_pattern() -> EventPattern {
        EventPattern {
            name: "cross_service_workflow".to_string(),
            description: "Cross-service communication workflow".to_string(),
            event_types: vec![
                "cross_service_request".to_string(),
                "service_response".to_string(),
            ],
            optional_events: vec![
                "service_error".to_string(),
                "retry_attempt".to_string(),
            ].into_iter().collect(),
            allowed_sources: HashSet::new(), // Any source allowed
            correlation_required: true,
            timeout_ms: 15000, // 15 seconds
        }
    }
}

/// Event correlation validator for tracking related events
pub struct EventCorrelationValidator {
    correlation_groups: HashMap<String, Vec<DaemonEvent>>,
}

impl EventCorrelationValidator {
    pub fn new() -> Self {
        Self {
            correlation_groups: HashMap::new(),
        }
    }

    pub fn analyze_correlations(&mut self, events: &[DaemonEvent]) -> CorrelationAnalysis {
        // Group events by correlation ID
        self.correlation_groups.clear();

        for event in events {
            if let Some(correlation_id) = &event.correlation_id {
                self.correlation_groups
                    .entry(correlation_id.clone())
                    .or_insert_with(Vec::new)
                    .push(event.clone());
            }
        }

        let total_events = events.len();
        let correlated_events = self.correlation_groups.values().map(|group| group.len()).sum();
        let uncorrelated_events = total_events - correlated_events;

        let group_sizes: Vec<usize> = self.correlation_groups.values().map(|group| group.len()).collect();
        let average_group_size = if group_sizes.is_empty() {
            0.0
        } else {
            group_sizes.iter().sum::<usize>() as f64 / group_sizes.len() as f64
        };

        let max_group_size = group_sizes.iter().max().copied().unwrap_or(0);
        let min_group_size = group_sizes.iter().min().copied().unwrap_or(0);

        // Analyze temporal patterns
        let temporal_violations = self.analyze_temporal_patterns();

        CorrelationAnalysis {
            total_events,
            correlated_events,
            uncorrelated_events,
            correlation_groups: self.correlation_groups.clone(),
            average_group_size,
            max_group_size,
            min_group_size,
            temporal_violations,
        }
    }

    fn analyze_temporal_patterns(&self) -> Vec<TemporalViolation> {
        let mut violations = Vec::new();

        for (correlation_id, events) in &self.correlation_groups {
            if events.len() < 2 {
                continue;
            }

            // Sort events by timestamp
            let mut sorted_events = events.clone();
            sorted_events.sort_by_key(|e| e.created_at);

            // Check for unusual gaps
            for i in 1..sorted_events.len() {
                let time_diff = sorted_events[i].created_at - sorted_events[i-1].created_at;

                // Flag gaps larger than 5 seconds as potential issues
                if time_diff.num_seconds() > 5 {
                    violations.push(TemporalViolation {
                        correlation_id: correlation_id.clone(),
                        violation_type: TemporalViolationType::LargeTimeGap,
                        description: format!("Large time gap of {} seconds between events", time_diff.num_seconds()),
                        events_involved: vec![sorted_events[i-1].id, sorted_events[i].id],
                    });
                }

                // Flag events that appear to be out of order
                if time_diff.num_seconds() < 0 {
                    violations.push(TemporalViolation {
                        correlation_id: correlation_id.clone(),
                        violation_type: TemporalViolationType::OutOfOrder,
                        description: "Events appear to be out of chronological order".to_string(),
                        events_involved: vec![sorted_events[i-1].id, sorted_events[i].id],
                    });
                }
            }
        }

        violations
    }

    pub fn get_correlation_group(&self, correlation_id: &str) -> Option<&Vec<DaemonEvent>> {
        self.correlation_groups.get(correlation_id)
    }

    pub fn find_related_events(&self, event_id: &uuid::Uuid) -> Vec<&DaemonEvent> {
        let mut related = Vec::new();

        for group in self.correlation_groups.values() {
            if let Some(target_event) = group.iter().find(|e| e.id == *event_id) {
                related.extend(group.iter().filter(|e| e.id != *event_id));
                break;
            }
        }

        related
    }
}

#[derive(Debug, Clone)]
pub struct CorrelationAnalysis {
    pub total_events: usize,
    pub correlated_events: usize,
    pub uncorrelated_events: usize,
    pub correlation_groups: HashMap<String, Vec<DaemonEvent>>,
    pub average_group_size: f64,
    pub max_group_size: usize,
    pub min_group_size: usize,
    pub temporal_violations: Vec<TemporalViolation>,
}

#[derive(Debug, Clone)]
pub struct TemporalViolation {
    pub correlation_id: String,
    pub violation_type: TemporalViolationType,
    pub description: String,
    pub events_involved: Vec<uuid::Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemporalViolationType {
    LargeTimeGap,
    OutOfOrder,
    CircularDependency,
    MissingIntermediateEvent,
}

/// Performance validator for checking event system performance
pub struct EventPerformanceValidator {
    performance_thresholds: PerformanceThresholds,
}

#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub max_event_processing_time_ms: u64,
    pub max_event_size_bytes: usize,
    pub max_throughput_violation_rate: f64, // Percentage of events that can exceed threshold
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_event_processing_time_ms: 1000, // 1 second
            max_event_size_bytes: 1024 * 1024,   // 1MB
            max_throughput_violation_rate: 0.05, // 5%
        }
    }
}

impl EventPerformanceValidator {
    pub fn new(thresholds: PerformanceThresholds) -> Self {
        Self {
            performance_thresholds: thresholds,
        }
    }

    pub fn validate_performance(&self, events: &[DaemonEvent]) -> PerformanceValidationResult {
        let mut violations = Vec::new();
        let mut total_processing_time = Duration::ZERO;
        let mut total_size = 0;
        let mut large_events = 0;
        let mut slow_events = 0;

        for event in events {
            let event_size = self.estimate_event_size(event);
            total_size += event_size;

            if event_size > self.performance_thresholds.max_event_size_bytes {
                violations.push(PerformanceViolation {
                    event_id: event.id,
                    violation_type: PerformanceViolationType::EventSizeExceeded,
                    actual_value: event_size as f64,
                    threshold: self.performance_thresholds.max_event_size_bytes as f64,
                    description: format!("Event size {} bytes exceeds threshold {}", event_size, self.performance_thresholds.max_event_size_bytes),
                });
                large_events += 1;
            }

            // Note: In a real implementation, we'd need processing time information
            // For now, we'll simulate this based on event complexity
            let estimated_processing_time = self.estimate_processing_time(event);
            total_processing_time += estimated_processing_time;

            if estimated_processing_time > Duration::from_millis(self.performance_thresholds.max_event_processing_time_ms) {
                violations.push(PerformanceViolation {
                    event_id: event.id,
                    violation_type: PerformanceViolationType::ProcessingTimeExceeded,
                    actual_value: estimated_processing_time.as_millis() as f64,
                    threshold: self.performance_thresholds.max_event_processing_time_ms as f64,
                    description: format!("Estimated processing time {}ms exceeds threshold {}ms",
                                       estimated_processing_time.as_millis(),
                                       self.performance_thresholds.max_event_processing_time_ms),
                });
                slow_events += 1;
            }
        }

        let average_event_size = if events.is_empty() { 0.0 } else { total_size as f64 / events.len() as f64 };
        let average_processing_time = if events.is_empty() { Duration::ZERO } else { total_processing_time / events.len() as u32 };

        let violation_rate = violations.len() as f64 / events.len() as f64;
        let within_threshold = violation_rate <= self.performance_thresholds.max_throughput_violation_rate;

        PerformanceValidationResult {
            within_threshold,
            violations,
            average_event_size,
            average_processing_time,
            violation_rate,
            total_events: events.len(),
        }
    }

    fn estimate_event_size(&self, event: &DaemonEvent) -> usize {
        let base_size = std::mem::size_of::<DaemonEvent>();
        let payload_size = match &event.payload {
            crucible_services::events::core::EventPayload::Json(value) => {
                serde_json::to_string(value).map(|s| s.len()).unwrap_or(0)
            }
            crucible_services::events::core::EventPayload::Binary(data) => data.len(),
            crucible_services::events::core::EventPayload::Text(text) => text.len(),
            crucible_services::events::core::EventPayload::Empty => 0,
        };

        base_size + payload_size
    }

    fn estimate_processing_time(&self, event: &DaemonEvent) -> Duration {
        // Simple heuristic based on event complexity
        let base_time = Duration::from_millis(10);
        let payload_complexity = match &event.payload {
            crucible_services::events::core::EventPayload::Json(value) => {
                // Estimate complexity based on JSON structure
                let string_len = serde_json::to_string(value).map(|s| s.len()).unwrap_or(0);
                string_len / 100 // 1ms per 100 characters
            }
            crucible_services::events::core::EventPayload::Binary(data) => data.len() / 1000,
            crucible_services::events::core::EventPayload::Text(text) => text.len() / 200,
            crucible_services::events::core::EventPayload::Empty => 0,
        };

        let target_count = event.targets.len() * 5; // 5ms per target
        base_time + Duration::from_millis((payload_complexity + target_count) as u64)
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceValidationResult {
    pub within_threshold: bool,
    pub violations: Vec<PerformanceViolation>,
    pub average_event_size: f64,
    pub average_processing_time: Duration,
    pub violation_rate: f64,
    pub total_events: usize,
}

#[derive(Debug, Clone)]
pub struct PerformanceViolation {
    pub event_id: uuid::Uuid,
    pub violation_type: PerformanceViolationType,
    pub actual_value: f64,
    pub threshold: f64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceViolationType {
    EventSizeExceeded,
    ProcessingTimeExceeded,
    ThroughputExceeded,
    MemoryUsageExceeded,
}

impl Default for EventFlowValidator {
    fn default() -> Self {
        Self::new()
    }
}