//! Event filtering tests for routing rules and filters

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::EventResult;
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

/// Test event factory for creating various types of events
struct TestEventFactory;

impl TestEventFactory {
    fn create_filesystem_event(path: &str, change_type: &str) -> DaemonEvent {
        let event_type = match change_type {
            "created" => EventType::Filesystem(FilesystemEventType::FileCreated {
                path: path.to_string(),
            }),
            "modified" => EventType::Filesystem(FilesystemEventType::FileModified {
                path: path.to_string(),
            }),
            "deleted" => EventType::Filesystem(FilesystemEventType::FileDeleted {
                path: path.to_string(),
            }),
            "moved" => EventType::Filesystem(FilesystemEventType::FileMoved {
                from: format!("{}{}", path, ".old"),
                to: path.to_string(),
            }),
            _ => EventType::Filesystem(FilesystemEventType::FileCreated {
                path: path.to_string(),
            }),
        };

        DaemonEvent::new(
            event_type,
            EventSource::filesystem("fs-watcher-1".to_string()),
            EventPayload::json(serde_json::json!({
                "path": path,
                "change_type": change_type,
                "timestamp": Utc::now().to_rfc3339()
            })),
        )
    }

    fn create_database_event(table: &str, operation: &str) -> DaemonEvent {
        let event_type = match operation {
            "created" => EventType::Database(DatabaseEventType::RecordCreated {
                table: table.to_string(),
                id: format!("id-{}", uuid::Uuid::new_v4()),
            }),
            "updated" => EventType::Database(DatabaseEventType::RecordUpdated {
                table: table.to_string(),
                id: format!("id-{}", uuid::Uuid::new_v4()),
                changes: HashMap::from([
                    ("status".to_string(), serde_json::Value::String("updated".to_string())),
                ]),
            }),
            "deleted" => EventType::Database(DatabaseEventType::RecordDeleted {
                table: table.to_string(),
                id: format!("id-{}", uuid::Uuid::new_v4()),
            }),
            _ => EventType::Database(DatabaseEventType::RecordCreated {
                table: table.to_string(),
                id: format!("id-{}", uuid::Uuid::new_v4()),
            }),
        };

        DaemonEvent::new(
            event_type,
            EventSource::database("db-trigger-1".to_string()),
            EventPayload::json(serde_json::json!({
                "table": table,
                "operation": operation,
                "record_id": uuid::Uuid::new_v4()
            })),
        )
    }

    fn create_service_event(service_id: &str, event_name: &str) -> DaemonEvent {
        let event_type = match event_name {
            "health_check" => EventType::Service(ServiceEventType::HealthCheck {
                service_id: service_id.to_string(),
                status: "healthy".to_string(),
            }),
            "registered" => EventType::Service(ServiceEventType::ServiceRegistered {
                service_id: service_id.to_string(),
                service_type: "test".to_string(),
            }),
            "unregistered" => EventType::Service(ServiceEventType::ServiceUnregistered {
                service_id: service_id.to_string(),
            }),
            _ => EventType::Service(ServiceEventType::HealthCheck {
                service_id: service_id.to_string(),
                status: "unknown".to_string(),
            }),
        };

        DaemonEvent::new(
            event_type,
            EventSource::service(service_id.to_string()),
            EventPayload::json(serde_json::json!({
                "service_id": service_id,
                "event_name": event_name,
                "timestamp": Utc::now().to_rfc3339()
            })),
        )
    }

    fn create_system_event(event_name: &str) -> DaemonEvent {
        let event_type = match event_name {
            "daemon_started" => EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            "daemon_stopped" => EventType::System(SystemEventType::DaemonStopped {
                reason: Some("shutdown".to_string()),
            }),
            "metrics" => EventType::System(SystemEventType::MetricsCollected {
                metrics: HashMap::from([
                    ("cpu_usage".to_string(), 45.2),
                    ("memory_usage".to_string(), 67.8),
                ]),
            }),
            _ => EventType::System(SystemEventType::DaemonStarted {
                version: "unknown".to_string(),
            }),
        };

        DaemonEvent::new(
            event_type,
            EventSource::system("daemon".to_string()),
            EventPayload::json(serde_json::json!({
                "system_event": event_name,
                "timestamp": Utc::now().to_rfc3339()
            })),
        )
    }

    fn create_custom_event(custom_type: &str, priority: EventPriority) -> DaemonEvent {
        DaemonEvent::new(
            EventType::Custom(custom_type.to_string()),
            EventSource::external("api-gateway".to_string()),
            EventPayload::json(serde_json::json!({
                "custom_type": custom_type,
                "data": "test data",
                "timestamp": Utc::now().to_rfc3339()
            })),
        )
        .with_priority(priority)
    }

    fn create_event_with_payload_size(size_bytes: usize, event_type: &str) -> DaemonEvent {
        let data = "x".repeat(size_bytes);
        let payload = EventPayload::json(serde_json::json!({
            "large_data": data,
            "size": size_bytes
        }));

        DaemonEvent::new(
            EventType::Custom(event_type.to_string()),
            EventSource::service("size-test-service".to_string()),
            payload,
        )
    }
}

/// Event filter test scenario
struct FilterTestScenario {
    name: String,
    filter: EventFilter,
    test_events: Vec<DaemonEvent>,
    expected_matches: usize,
    description: String,
}

impl FilterTestScenario {
    fn new(name: String, filter: EventFilter, description: String) -> Self {
        Self {
            name,
            filter,
            test_events: Vec::new(),
            expected_matches: 0,
            description,
        }
    }

    fn with_events(mut self, events: Vec<DaemonEvent>) -> Self {
        self.test_events = events;
        self
    }

    fn with_expected_matches(mut self, count: usize) -> Self {
        self.expected_matches = count;
        self
    }
}

/// Filter test runner
struct FilterTestRunner;

impl FilterTestRunner {
    fn run_scenario(scenario: FilterTestScenario) -> FilterTestResults {
        let mut actual_matches = 0;
        let mut match_details = Vec::new();

        for (index, event) in scenario.test_events.iter().enumerate() {
            let matches = scenario.filter.matches(event);
            if matches {
                actual_matches += 1;
                match_details.push((index, true));
            } else {
                match_details.push((index, false));
            }
        }

        FilterTestResults {
            scenario_name: scenario.name,
            description: scenario.description,
            filter: scenario.filter,
            total_events: scenario.test_events.len(),
            expected_matches: scenario.expected_matches,
            actual_matches,
            match_details,
            success: actual_matches == scenario.expected_matches,
        }
    }

    fn run_scenarios(scenarios: Vec<FilterTestScenario>) -> Vec<FilterTestResults> {
        scenarios
            .into_iter()
            .map(|scenario| Self::run_scenario(scenario))
            .collect()
    }
}

#[derive(Debug)]
struct FilterTestResults {
    scenario_name: String,
    description: String,
    filter: EventFilter,
    total_events: usize,
    expected_matches: usize,
    actual_matches: usize,
    match_details: Vec<(usize, bool)>,
    success: bool,
}

impl FilterTestResults {
    fn print_summary(&self) {
        println!("\n=== Filter Test: {} ===", self.scenario_name);
        println!("Description: {}", self.description);
        println!("Expected matches: {}/}", self.expected_matches, self.total_events);
        println!("Actual matches: {}/}", self.actual_matches, self.total_events);
        println!("Result: {}", if self.success { "PASS" } else { "FAIL" });

        if !self.success {
            println!("\nMatch Details:");
            for (index, matched) in &self.match_details {
                println!("  Event {}: {}", index, if *matched { "MATCHED" } else { "NO MATCH" });
            }
        }

        // Print filter configuration
        println!("\nFilter Configuration:");
        if !self.filter.event_types.is_empty() {
            println!("  Event types: {:?}", self.filter.event_types);
        }
        if !self.filter.categories.is_empty() {
            println!("  Categories: {:?}", self.filter.categories);
        }
        if !self.filter.priorities.is_empty() {
            println!("  Priorities: {:?}", self.filter.priorities);
        }
        if !self.filter.sources.is_empty() {
            println!("  Sources: {:?}", self.filter.sources);
        }
        if let Some(max_size) = self.filter.max_payload_size {
            println!("  Max payload size: {} bytes", max_size);
        }
        if let Some(ref expression) = self.filter.expression {
            println!("  Expression: {}", expression);
        }
    }
}

#[cfg(test)]
mod basic_filter_tests {
    use super::*;

    #[test]
    fn test_event_type_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Filesystem Events Only".to_string(),
                EventFilter {
                    event_types: vec!["filesystem".to_string()],
                    ..Default::default()
                },
                "Should match only filesystem events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Database Events Only".to_string(),
                EventFilter {
                    event_types: vec!["database".to_string()],
                    ..Default::default()
                },
                "Should match only database events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_database_event("orders", "updated"),
                TestEventFactory::create_system_event("daemon_started"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Multiple Event Types".to_string(),
                EventFilter {
                    event_types: vec!["filesystem".to_string(), "system".to_string()],
                    ..Default::default()
                },
                "Should match filesystem and system events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_system_event("daemon_started"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_filesystem_event("/test2.txt", "deleted"),
            ])
            .with_expected_matches(3),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_event_category_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Filesystem Category".to_string(),
                EventFilter {
                    categories: vec![EventCategory::Filesystem],
                    ..Default::default()
                },
                "Should match filesystem category events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Multiple Categories".to_string(),
                EventFilter {
                    categories: vec![EventCategory::Database, EventCategory::Service],
                    ..Default::default()
                },
                "Should match database and service category events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_system_event("daemon_started"),
                TestEventFactory::create_database_event("orders", "updated"),
            ])
            .with_expected_matches(3),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Category filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_priority_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "High Priority Only".to_string(),
                EventFilter {
                    priorities: vec![EventPriority::High, EventPriority::Critical],
                    ..Default::default()
                },
                "Should match high and critical priority events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_custom_event("test1", EventPriority::Critical),
                TestEventFactory::create_custom_event("test2", EventPriority::High),
                TestEventFactory::create_custom_event("test3", EventPriority::Normal),
                TestEventFactory::create_custom_event("test4", EventPriority::Low),
                TestEventFactory::create_custom_event("test5", EventPriority::High),
            ])
            .with_expected_matches(3),

            FilterTestScenario::new(
                "Low Priority Only".to_string(),
                EventFilter {
                    priorities: vec![EventPriority::Low],
                    ..Default::default()
                },
                "Should match only low priority events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_custom_event("test1", EventPriority::Critical),
                TestEventFactory::create_custom_event("test2", EventPriority::Normal),
                TestEventFactory::create_custom_event("test3", EventPriority::Low),
                TestEventFactory::create_custom_event("test4", EventPriority::High),
            ])
            .with_expected_matches(1),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Priority filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_source_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Filesystem Sources".to_string(),
                EventFilter {
                    sources: vec!["fs-watcher-1".to_string()],
                    ..Default::default()
                },
                "Should match events from filesystem watcher".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified"),
                TestEventFactory::create_service_event("test-service", "health_check"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Multiple Sources".to_string(),
                EventFilter {
                    sources: vec!["db-trigger-1".to_string(), "daemon".to_string()],
                    ..Default::default()
                },
                "Should match events from database and daemon sources".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_system_event("daemon_started"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_database_event("orders", "updated"),
            ])
            .with_expected_matches(3),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Source filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_payload_size_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Small Payload Only".to_string(),
                EventFilter {
                    max_payload_size: Some(1000),
                    ..Default::default()
                },
                "Should match only events with small payloads".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_event_with_payload_size(500, "small"),
                TestEventFactory::create_event_with_payload_size(1500, "medium"),
                TestEventFactory::create_event_with_payload_size(200, "tiny"),
                TestEventFactory::create_event_with_payload_size(3000, "large"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Large Payload Allowed".to_string(),
                EventFilter {
                    max_payload_size: Some(5000),
                    ..Default::default()
                },
                "Should match events with payloads up to 5KB".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_event_with_payload_size(1000, "medium1"),
                TestEventFactory::create_event_with_payload_size(3000, "medium2"),
                TestEventFactory::create_event_with_payload_size(6000, "too-large"),
                TestEventFactory::create_event_with_payload_size(4000, "large"),
            ])
            .with_expected_matches(3),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Payload size filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_custom_expression_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Keyword Expression".to_string(),
                EventFilter {
                    expression: Some("test-service".to_string()),
                    ..Default::default()
                },
                "Should match events containing 'test-service'".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_service_event("other-service", "health_check"),
                TestEventFactory::create_filesystem_event("/test-service-file.txt", "created"),
                TestEventFactory::create_database_event("test_service_table", "created"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Multiple Keywords".to_string(),
                EventFilter {
                    expression: Some("users created".to_string()),
                    ..Default::default()
                },
                "Should match events containing 'users' and 'created'".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_database_event("orders", "created"),
                TestEventFactory::create_database_event("users", "updated"),
                TestEventFactory::create_service_event("users-service", "created"),
            ])
            .with_expected_matches(1),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Expression filter test '{}' failed", result.scenario_name);
        }
    }
}

#[cfg(test)]
mod complex_filter_tests {
    use super::*;

    #[test]
    fn test_multi_criteria_filtering() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Filesystem + High Priority".to_string(),
                EventFilter {
                    event_types: vec!["filesystem".to_string()],
                    priorities: vec![EventPriority::High, EventPriority::Critical],
                    ..Default::default()
                },
                "Should match high priority filesystem events".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified")
                    .with_priority(EventPriority::Normal),
                TestEventFactory::create_database_event("users", "created")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_filesystem_event("/test3.txt", "deleted")
                    .with_priority(EventPriority::Critical),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Database + Specific Source".to_string(),
                EventFilter {
                    event_types: vec!["database".to_string()],
                    sources: vec!["db-trigger-1".to_string()],
                    ..Default::default()
                },
                "Should match database events from specific source".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_database_event("users", "created"),
                TestEventFactory::create_database_event("orders", "updated"),
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_service_event("db-trigger-1", "health_check"),
            ])
            .with_expected_matches(2),

            FilterTestScenario::new(
                "Complex Multi-Criteria".to_string(),
                EventFilter {
                    categories: vec![EventCategory::Service, EventCategory::System],
                    priorities: vec![EventPriority::High, EventPriority::Critical],
                    sources: vec!["test-service".to_string(), "daemon".to_string()],
                    ..Default::default()
                },
                "Should match high priority service/system events from specific sources".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_service_event("test-service", "health_check")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_service_event("other-service", "health_check")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_system_event("daemon_started")
                    .with_priority(EventPriority::Critical),
                TestEventFactory::create_filesystem_event("/test.txt", "created")
                    .with_priority(EventPriority::High),
            ])
            .with_expected_matches(2),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Complex filter test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_filter_combinations() {
        let scenarios = vec![
            FilterTestScenario::new(
                "All Criteria Must Match".to_string(),
                EventFilter {
                    event_types: vec!["filesystem".to_string()],
                    sources: vec!["fs-watcher-1".to_string()],
                    priorities: vec![EventPriority::High],
                    max_payload_size: Some(2000),
                    ..Default::default()
                },
                "All filter criteria must match".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified")
                    .with_priority(EventPriority::Normal),
                TestEventFactory::create_database_event("users", "created")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_filesystem_event("/test3.txt", "deleted")
                    .with_priority(EventPriority::High),
            ])
            .with_expected_matches(2), // fs-watcher-1 source + filesystem + high priority

            FilterTestScenario::new(
                "Relaxed Criteria".to_string(),
                EventFilter {
                    categories: vec![EventCategory::Filesystem, EventCategory::Database],
                    priorities: vec![EventPriority::Normal, EventPriority::High],
                    ..Default::default()
                },
                "Should match filesystem or database events with normal or high priority".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_database_event("users", "created")
                    .with_priority(EventPriority::Normal),
                TestEventFactory::create_service_event("test-service", "health_check")
                    .with_priority(EventPriority::High),
                TestEventFactory::create_filesystem_event("/test2.txt", "modified")
                    .with_priority(EventPriority::Low),
                TestEventFactory::create_system_event("daemon_started")
                    .with_priority(EventPriority::Normal),
            ])
            .with_expected_matches(3),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Filter combination test '{}' failed", result.scenario_name);
        }
    }
}

#[cfg(test)]
mod filter_edge_cases {
    use super::*;

    #[test]
    fn test_empty_filter() {
        let scenario = FilterTestScenario::new(
            "Empty Filter".to_string(),
            EventFilter::default(),
            "Empty filter should match all events".to_string(),
        )
        .with_events(vec![
            TestEventFactory::create_filesystem_event("/test.txt", "created"),
            TestEventFactory::create_database_event("users", "created"),
            TestEventFactory::create_service_event("test-service", "health_check"),
            TestEventFactory::create_system_event("daemon_started"),
            TestEventFactory::create_custom_event("test", EventPriority::Normal),
        ])
        .with_expected_matches(5);

        let result = FilterTestRunner::run_scenario(scenario);
        result.print_summary();
        assert!(result.success, "Empty filter should match all events");
    }

    #[test]
    fn test_no_matches_filter() {
        let scenario = FilterTestScenario::new(
            "No Matches Filter".to_string(),
            EventFilter {
                event_types: vec!["nonexistent".to_string()],
                sources: vec!["nonexistent-source".to_string()],
                priorities: vec![EventPriority::Critical],
                ..Default::default()
            },
            "Filter with impossible criteria should match no events".to_string(),
        )
        .with_events(vec![
            TestEventFactory::create_filesystem_event("/test.txt", "created"),
            TestEventFactory::create_database_event("users", "created"),
            TestEventFactory::create_service_event("test-service", "health_check"),
        ])
        .with_expected_matches(0);

        let result = FilterTestRunner::run_scenario(scenario);
        result.print_summary();
        assert!(result.success, "Impossible filter should match no events");
    }

    #[test]
    fn test_case_sensitivity() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Case Sensitive Event Types".to_string(),
                EventFilter {
                    event_types: vec!["FILESYSTEM".to_string()], // Uppercase
                    ..Default::default()
                },
                "Event type matching should be case sensitive".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
            ])
            .with_expected_matches(0),

            FilterTestScenario::new(
                "Case Sensitive Sources".to_string(),
                EventFilter {
                    sources: vec!["FS-WATCHER-1".to_string()], // Uppercase
                    ..Default::default()
                },
                "Source matching should be case sensitive".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_filesystem_event("/test.txt", "created"),
                TestEventFactory::create_database_event("users", "created"),
            ])
            .with_expected_matches(0),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Case sensitivity test '{}' failed", result.scenario_name);
        }
    }

    #[test]
    fn test_filter_with_special_characters() {
        let scenarios = vec![
            FilterTestScenario::new(
                "Special Characters in Expression".to_string(),
                EventFilter {
                    expression: Some("test-service_v2".to_string()),
                    ..Default::default()
                },
                "Should handle special characters in expressions".to_string(),
            )
            .with_events(vec![
                TestEventFactory::create_service_event("test-service_v2", "health_check"),
                TestEventFactory::create_service_event("test-service", "health_check"),
                TestEventFactory::create_service_event("test_service_v2", "health_check"),
            ])
            .with_expected_matches(1),

            FilterTestScenario::new(
                "Unicode Characters".to_string(),
                EventFilter {
                    expression: Some("测试".to_string()), // Chinese characters
                    ..Default::default()
                },
                "Should handle unicode characters in expressions".to_string(),
            )
            .with_events(vec![
                DaemonEvent::new(
                    EventType::Custom("test-event".to_string()),
                    EventSource::service("测试服务".to_string()),
                    EventPayload::json(serde_json::json!({"message": "测试消息"})),
                ),
                TestEventFactory::create_service_event("test-service", "health_check"),
            ])
            .with_expected_matches(1),
        ];

        let results = FilterTestRunner::run_scenarios(scenarios);

        for result in &results {
            result.print_summary();
            assert!(result.success, "Special characters test '{}' failed", result.scenario_name);
        }
    }
}

#[cfg(test)]
mod routing_rule_filter_tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_routing_rule_with_filters() {
        let router = Arc::new(DefaultEventRouter::new());

        // Register test services
        let services = vec![
            ("fs-processor", "filesystem"),
            ("db-processor", "database"),
            ("general-processor", "all"),
        ];

        for (service_id, supported_type) in services {
            let registration = ServiceRegistration {
                service_id: service_id.to_string(),
                service_type: "test".to_string(),
                instance_id: "instance-1".to_string(),
                endpoint: None,
                supported_event_types: vec![supported_type.to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: 10,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            router.register_service(registration).await.unwrap();
        }

        // Create routing rule with filter
        let filter = EventFilter {
            event_types: vec!["filesystem".to_string()],
            ..Default::default()
        };

        let rule = RoutingRule {
            rule_id: "fs-filter-rule".to_string(),
            name: "Filesystem Filter Rule".to_string(),
            description: "Route filesystem events to fs-processor".to_string(),
            filter,
            targets: vec![ServiceTarget::new("fs-processor".to_string())],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();

        // Test routing
        let test_events = vec![
            TestEventFactory::create_filesystem_event("/test.txt", "created"),
            TestEventFactory::create_database_event("users", "created"),
            TestEventFactory::create_filesystem_event("/test2.txt", "modified"),
            TestEventFactory::create_service_event("test-service", "health_check"),
        ];

        let mut filesystem_event_routed = 0;
        let mut other_event_routed = 0;

        for event in test_events {
            let targets = router.test_routing(&event).await.unwrap();

            match event.event_type.category() {
                EventCategory::Filesystem => {
                    if !targets.is_empty() {
                        filesystem_event_routed += 1;
                        assert_eq!(targets[0], "fs-processor");
                    }
                }
                _ => {
                    if !targets.is_empty() {
                        other_event_routed += 1;
                    }
                }
            }
        }

        // Verify filtering worked
        assert_eq!(filesystem_event_routed, 2, "Filesystem events should be routed");
        assert_eq!(other_event_routed, 0, "Other events should not be routed by this rule");
    }

    #[tokio::test]
    async fn test_routing_rule_priority_with_filters() {
        let router = Arc::new(DefaultEventRouter::new());

        // Register service
        let registration = ServiceRegistration {
            service_id: "priority-test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["test".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create high priority rule with narrow filter
        let high_priority_filter = EventFilter {
            event_types: vec!["custom".to_string()],
            sources: vec!["critical-source".to_string()],
            ..Default::default()
        };

        let high_priority_rule = RoutingRule {
            rule_id: "high-priority-rule".to_string(),
            name: "High Priority Rule".to_string(),
            description: "High priority rule with narrow filter".to_string(),
            filter: high_priority_filter,
            targets: vec![ServiceTarget::new("priority-test-service".to_string())],
            priority: 10,
            enabled: true,
            conditions: Vec::new(),
        };

        // Create low priority rule with broad filter
        let low_priority_filter = EventFilter {
            event_types: vec!["custom".to_string()],
            ..Default::default()
        };

        let low_priority_rule = RoutingRule {
            rule_id: "low-priority-rule".to_string(),
            name: "Low Priority Rule".to_string(),
            description: "Low priority rule with broad filter".to_string(),
            filter: low_priority_filter,
            targets: vec![ServiceTarget::new("priority-test-service".to_string())],
            priority: 1,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(low_priority_rule).await.unwrap();
        router.add_routing_rule(high_priority_rule).await.unwrap();

        // Test events
        let matching_high_priority = DaemonEvent::new(
            EventType::Custom("test".to_string()),
            EventSource::service("critical-source".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let matching_low_priority = DaemonEvent::new(
            EventType::Custom("test".to_string()),
            EventSource::service("normal-source".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        // Both should match and be routed (to the same service in this case)
        let targets1 = router.test_routing(&matching_high_priority).await.unwrap();
        let targets2 = router.test_routing(&matching_low_priority).await.unwrap();

        assert!(!targets1.is_empty(), "High priority event should be routed");
        assert!(!targets2.is_empty(), "Low priority event should be routed");
    }

    #[tokio::test]
    async fn test_disabled_routing_rule_with_filters() {
        let router = Arc::new(DefaultEventRouter::new());

        // Register service
        let registration = ServiceRegistration {
            service_id: "disabled-rule-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["test".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Create disabled rule
        let filter = EventFilter {
            event_types: vec!["custom".to_string()],
            ..Default::default()
        };

        let disabled_rule = RoutingRule {
            rule_id: "disabled-rule".to_string(),
            name: "Disabled Rule".to_string(),
            description: "A disabled routing rule".to_string(),
            filter,
            targets: vec![ServiceTarget::new("disabled-rule-service".to_string())],
            priority: 0,
            enabled: false, // Disabled
            conditions: Vec::new(),
        };

        router.add_routing_rule(disabled_rule).await.unwrap();

        // Test event that would match the filter
        let test_event = DaemonEvent::new(
            EventType::Custom("test".to_string()),
            EventSource::service("test-source".to_string()),
            EventPayload::json(serde_json::json!({})),
        );

        let targets = router.test_routing(&test_event).await.unwrap();
        assert!(targets.is_empty(), "Disabled rule should not route events");
    }
}

#[cfg(test)]
mod filter_performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_filter_performance() {
        let scenarios = vec![
            ("Simple Type Filter", EventFilter {
                event_types: vec!["filesystem".to_string()],
                ..Default::default()
            }),
            ("Complex Multi-Criteria Filter", EventFilter {
                event_types: vec!["filesystem".to_string(), "database".to_string()],
                categories: vec![EventCategory::Filesystem, EventCategory::Database],
                priorities: vec![EventPriority::High, EventPriority::Normal],
                sources: vec!["fs-watcher-1".to_string(), "db-trigger-1".to_string()],
                max_payload_size: Some(1000),
                expression: Some("test".to_string()),
            }),
            ("Expression Only Filter", EventFilter {
                expression: Some("filesystem database test".to_string()),
                ..Default::default()
            }),
        ];

        let test_events = vec![
            TestEventFactory::create_filesystem_event("/test.txt", "created"),
            TestEventFactory::create_database_event("users", "created"),
            TestEventFactory::create_service_event("test-service", "health_check"),
            TestEventFactory::create_system_event("daemon_started"),
            TestEventFactory::create_custom_event("test", EventPriority::Normal),
        ];

        for (scenario_name, filter) in scenarios {
            let iterations = 10000;
            let start_time = Instant::now();

            for _ in 0..iterations {
                for event in &test_events {
                    filter.matches(event);
                }
            }

            let duration = start_time.elapsed();
            let total_operations = iterations * test_events.len();
            let ops_per_second = total_operations as f64 / duration.as_secs_f64();

            println!("\n=== Filter Performance: {} ===", scenario_name);
            println!("Total operations: {}", total_operations);
            println!("Duration: {:?}", duration);
            println!("Operations per second: {:.2}", ops_per_second);

            // Performance assertions
            assert!(ops_per_second > 100000.0, "Filter should be fast (>100k ops/sec)");
        }
    }

    #[test]
    fn test_large_dataset_filtering() {
        let filter = EventFilter {
            event_types: vec!["filesystem".to_string(), "database".to_string()],
            priorities: vec![EventPriority::High, EventPriority::Critical],
            ..Default::default()
        };

        // Create large dataset
        let mut events = Vec::new();
        for i in 0..10000 {
            let event = match i % 5 {
                0 => TestEventFactory::create_filesystem_event(&format!("/test{}.txt", i), "created")
                    .with_priority(if i % 3 == 0 { EventPriority::High } else { EventPriority::Normal }),
                1 => TestEventFactory::create_database_event("users", "created")
                    .with_priority(if i % 4 == 0 { EventPriority::Critical } else { EventPriority::Normal }),
                2 => TestEventFactory::create_service_event("test-service", "health_check")
                    .with_priority(EventPriority::Normal),
                3 => TestEventFactory::create_system_event("daemon_started")
                    .with_priority(EventPriority::Normal),
                _ => TestEventFactory::create_custom_event(&format!("test{}", i), EventPriority::Low),
            };
            events.push(event);
        }

        let start_time = Instant::now();
        let mut matches = 0;

        for event in &events {
            if filter.matches(event) {
                matches += 1;
            }
        }

        let duration = start_time.elapsed();

        println!("\n=== Large Dataset Filter Performance ===");
        println!("Events processed: {}", events.len());
        println!("Matches found: {}", matches);
        println!("Duration: {:?}", duration);
        println!("Events per second: {:.2}", events.len() as f64 / duration.as_secs_f64());

        // Should process large datasets efficiently
        assert!(duration.as_millis() < 1000, "Should process 10k events in less than 1 second");
        assert!(matches > 0, "Should find some matches");
    }
}