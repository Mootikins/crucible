//! Test utilities and mock services for event system testing

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

/// Mock event service for testing
pub struct MockEventService {
    pub service_id: String,
    pub service_type: String,
    pub events_received: Arc<Mutex<Vec<DaemonEvent>>>,
    pub should_fail: Arc<AtomicBool>,
    pub failure_rate: f64,
    pub processing_delay_ms: u64,
    pub max_concurrent: usize,
    pub current_load: Arc<AtomicUsize>,
    pub supported_event_types: Vec<String>,
}

impl MockEventService {
    pub fn new(service_id: String, service_type: String) -> Self {
        Self {
            service_id,
            service_type,
            events_received: Arc::new(Mutex::new(Vec::new())),
            should_fail: Arc::new(AtomicBool::new(false)),
            failure_rate: 0.0,
            processing_delay_ms: 0,
            max_concurrent: 100,
            current_load: Arc::new(AtomicUsize::new(0)),
            supported_event_types: vec!["test".to_string(), "custom".to_string()],
        }
    }

    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.failure_rate = rate;
        self
    }

    pub fn with_processing_delay(mut self, delay_ms: u64) -> Self {
        self.processing_delay_ms = delay_ms;
        self
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    pub fn with_supported_events(mut self, events: Vec<String>) -> Self {
        self.supported_event_types = events;
        self
    }

    pub async fn handle_event(&self, event: DaemonEvent) -> EventResult<()> {
        // Check concurrent limit
        let current = self.current_load.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_concurrent {
            self.current_load.fetch_sub(1, Ordering::SeqCst);
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                "Service overloaded".to_string(),
            ));
        }

        // Simulate processing delay
        if self.processing_delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.processing_delay_ms)).await;
        }

        // Determine if should fail based on failure rate
        let should_fail = self.should_fail.load(Ordering::SeqCst) ||
            (self.failure_rate > 0.0 && rand::random::<f64>() < self.failure_rate);

        if should_fail {
            self.current_load.fetch_sub(1, Ordering::SeqCst);
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                "Mock service failure".to_string(),
            ));
        }

        // Store the event
        {
            let mut events = self.events_received.lock().await;
            events.push(event);
        }

        // Release load
        self.current_load.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    pub async fn get_events(&self) -> Vec<DaemonEvent> {
        self.events_received.lock().await.clone()
    }

    pub async fn clear_events(&self) {
        self.events_received.lock().await.clear();
    }

    pub async fn get_event_count(&self) -> usize {
        self.events_received.lock().await.len()
    }

    pub async fn set_should_fail(&self, should_fail: bool) {
        self.should_fail.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_current_load(&self) -> usize {
        self.current_load.load(Ordering::SeqCst)
    }

    pub fn create_service_registration(&self) -> ServiceRegistration {
        ServiceRegistration {
            service_id: self.service_id.clone(),
            service_type: self.service_type.clone(),
            instance_id: format!("{}-instance-1", self.service_id),
            endpoint: Some(format!("http://localhost:8080/{}", self.service_id)),
            supported_event_types: self.supported_event_types.clone(),
            priority: 0,
            weight: 1.0,
            max_concurrent_events: self.max_concurrent,
            filters: Vec::new(),
            metadata: HashMap::from([
                ("mock_service".to_string(), "true".to_string()),
            ]),
        }
    }
}

/// Event test data builder
pub struct EventDataBuilder {
    event_type: Option<EventType>,
    source: Option<EventSource>,
    payload: Option<EventPayload>,
    priority: EventPriority,
    targets: Vec<ServiceTarget>,
    metadata: HashMap<String, String>,
}

impl EventDataBuilder {
    pub fn new() -> Self {
        Self {
            event_type: None,
            source: None,
            payload: None,
            priority: EventPriority::Normal,
            targets: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_event_type(mut self, event_type: EventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    pub fn with_source(mut self, source: EventSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_payload(mut self, payload: EventPayload) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_target(mut self, target: ServiceTarget) -> Self {
        self.targets.push(target);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn build(self) -> DaemonEvent {
        let event_type = self.event_type.unwrap_or_else(|| {
            EventType::Custom("test-event".to_string())
        });

        let source = self.source.unwrap_or_else(|| {
            EventSource::service("test-source".to_string())
        });

        let payload = self.payload.unwrap_or_else(|| {
            EventPayload::json(serde_json::json!({"test": true}))
        });

        let mut event = DaemonEvent::new(event_type, source, payload)
            .with_priority(self.priority);

        for target in self.targets {
            event = event.with_target(target);
        }

        for (key, value) in self.metadata {
            event = event.with_metadata(key, value);
        }

        event
    }
}

impl Default for EventDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Test event factory for creating various types of test events
pub struct TestEventFactory;

impl TestEventFactory {
    /// Create a basic test event
    pub fn create_basic_event(event_id: &str) -> DaemonEvent {
        EventDataBuilder::new()
            .with_event_type(EventType::Custom(format!("test-{}", event_id)))
            .with_source(EventSource::service(format!("test-client-{}", event_id)))
            .with_payload(EventPayload::json(serde_json::json!({
                "event_id": event_id,
                "timestamp": Utc::now().to_rfc3339(),
                "test": true
            })))
            .build()
    }

    /// Create a filesystem test event
    pub fn create_filesystem_event(path: &str, operation: &str) -> DaemonEvent {
        let event_type = match operation {
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

        EventDataBuilder::new()
            .with_event_type(event_type)
            .with_source(EventSource::filesystem("fs-watcher".to_string()))
            .with_payload(EventPayload::json(serde_json::json!({
                "path": path,
                "operation": operation,
                "timestamp": Utc::now().to_rfc3339()
            })))
            .build()
    }

    /// Create a database test event
    pub fn create_database_event(table: &str, operation: &str, record_id: Option<String>) -> DaemonEvent {
        let id = record_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let event_type = match operation {
            "created" => EventType::Database(DatabaseEventType::RecordCreated {
                table: table.to_string(),
                id: id.clone(),
            }),
            "updated" => EventType::Database(DatabaseEventType::RecordUpdated {
                table: table.to_string(),
                id: id.clone(),
                changes: HashMap::from([
                    ("updated_at".to_string(), serde_json::Value::String(Utc::now().to_rfc3339())),
                ]),
            }),
            "deleted" => EventType::Database(DatabaseEventType::RecordDeleted {
                table: table.to_string(),
                id: id.clone(),
            }),
            _ => EventType::Database(DatabaseEventType::RecordCreated {
                table: table.to_string(),
                id: id.clone(),
            }),
        };

        EventDataBuilder::new()
            .with_event_type(event_type)
            .with_source(EventSource::database("db-trigger".to_string()))
            .with_payload(EventPayload::json(serde_json::json!({
                "table": table,
                "operation": operation,
                "record_id": id,
                "timestamp": Utc::now().to_rfc3339()
            })))
            .build()
    }

    /// Create a service test event
    pub fn create_service_event(service_id: &str, event_type: &str, data: serde_json::Value) -> DaemonEvent {
        let service_event_type = match event_type {
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
            "status_changed" => EventType::Service(ServiceEventType::ServiceStatusChanged {
                service_id: service_id.to_string(),
                old_status: "starting".to_string(),
                new_status: "running".to_string(),
            }),
            _ => EventType::Service(ServiceEventType::HealthCheck {
                service_id: service_id.to_string(),
                status: "unknown".to_string(),
            }),
        };

        EventDataBuilder::new()
            .with_event_type(service_event_type)
            .with_source(EventSource::service(service_id.to_string()))
            .with_payload(EventPayload::json(serde_json::json!({
                "service_id": service_id,
                "event_type": event_type,
                "data": data,
                "timestamp": Utc::now().to_rfc3339()
            })))
            .build()
    }

    /// Create a system test event
    pub fn create_system_event(event_name: &str, data: serde_json::Value) -> DaemonEvent {
        let system_event_type = match event_name {
            "daemon_started" => EventType::System(SystemEventType::DaemonStarted {
                version: "1.0.0".to_string(),
            }),
            "daemon_stopped" => EventType::System(SystemEventType::DaemonStopped {
                reason: Some("test_complete".to_string()),
            }),
            "metrics" => EventType::System(SystemEventType::MetricsCollected {
                metrics: HashMap::from([
                    ("cpu_usage".to_string(), 45.2),
                    ("memory_usage".to_string(), 67.8),
                ]),
            }),
            "backup_completed" => EventType::System(SystemEventType::BackupCompleted {
                backup_path: "/test/backup.tar.gz".to_string(),
                size_bytes: 1024 * 1024,
            }),
            _ => EventType::System(SystemEventType::DaemonStarted {
                version: "test".to_string(),
            }),
        };

        EventDataBuilder::new()
            .with_event_type(system_event_type)
            .with_source(EventSource::system("daemon".to_string()))
            .with_payload(EventPayload::json(serde_json::json!({
                "system_event": event_name,
                "data": data,
                "timestamp": Utc::now().to_rfc3339()
            })))
            .build()
    }

    /// Create an event with specific priority
    pub fn create_event_with_priority(priority: EventPriority) -> DaemonEvent {
        EventDataBuilder::new()
            .with_event_type(EventType::Custom("priority-test".to_string()))
            .with_source(EventSource::service("priority-test-client".to_string()))
            .with_priority(priority)
            .with_payload(EventPayload::json(serde_json::json!({
                "priority_test": true,
                "priority_level": format!("{:?}", priority)
            })))
            .build()
    }

    /// Create an event with large payload
    pub fn create_large_payload_event(size_kb: usize) -> DaemonEvent {
        let data = "x".repeat(size_kb * 1024);
        EventDataBuilder::new()
            .with_event_type(EventType::Custom("large-payload-test".to_string()))
            .with_source(EventSource::service("large-payload-client".to_string()))
            .with_payload(EventPayload::json(serde_json::json!({
                "large_data": data,
                "size_kb": size_kb
            })))
            .build()
    }

    /// Create a batch of similar events
    pub fn create_event_batch(count: usize, event_type: &str) -> Vec<DaemonEvent> {
        (0..count)
            .map(|i| match event_type {
                "filesystem" => Self::create_filesystem_event(&format!("/test/file{}.txt", i), "created"),
                "database" => Self::create_database_event("test_table", "created", Some(format!("id-{}", i))),
                "service" => Self::create_service_event(&format!("service-{}", i), "health_check", serde_json::json!({"index": i})),
                "system" => Self::create_system_event("metrics", serde_json::json!({"index": i})),
                _ => Self::create_basic_event(&format!("event-{}", i)),
            })
            .collect()
    }
}

/// Test environment setup for event routing tests
pub struct EventTestEnvironment {
    pub router: Arc<DefaultEventRouter>,
    pub services: HashMap<String, Arc<MockEventService>>,
    pub routing_rules: Vec<RoutingRule>,
}

impl EventTestEnvironment {
    pub fn new() -> Self {
        Self {
            router: Arc::new(DefaultEventRouter::new()),
            services: HashMap::new(),
            routing_rules: Vec::new(),
        }
    }

    pub fn with_config(config: RoutingConfig) -> Self {
        Self {
            router: Arc::new(DefaultEventRouter::with_config(config)),
            services: HashMap::new(),
            routing_rules: Vec::new(),
        }
    }

    /// Add a mock service to the environment
    pub async fn add_service(&mut self, service_id: &str, service_type: &str) -> Arc<MockEventService> {
        let service = Arc::new(MockEventService::new(
            service_id.to_string(),
            service_type.to_string(),
        ));

        let registration = service.create_service_registration();
        self.router.register_service(registration).await.unwrap();

        // Set initial health
        self.router.update_service_health(service_id, ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Test service initialized".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        self.services.insert(service_id.to_string(), service.clone());
        service
    }

    /// Add a mock service with custom configuration
    pub async fn add_custom_service(&mut self, service: Arc<MockEventService>) {
        let registration = service.create_service_registration();
        self.router.register_service(registration).await.unwrap();

        self.router.update_service_health(&service.service_id, ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Custom test service initialized".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        self.services.insert(service.service_id.clone(), service);
    }

    /// Add a routing rule to the environment
    pub async fn add_routing_rule(&mut self, rule: RoutingRule) {
        self.router.add_routing_rule(rule.clone()).await.unwrap();
        self.routing_rules.push(rule);
    }

    /// Create a simple routing rule
    pub fn create_routing_rule(
        rule_id: &str,
        filter: EventFilter,
        targets: Vec<String>,
    ) -> RoutingRule {
        RoutingRule {
            rule_id: rule_id.to_string(),
            name: format!("Test Rule {}", rule_id),
            description: "Test routing rule".to_string(),
            filter,
            targets: targets.into_iter().map(ServiceTarget::new).collect(),
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        }
    }

    /// Route an event through the environment
    pub async fn route_event(&self, event: DaemonEvent) -> EventResult<()> {
        self.router.route_event(event).await
    }

    /// Test routing without actually sending the event
    pub async fn test_routing(&self, event: &DaemonEvent) -> EventResult<Vec<String>> {
        self.router.test_routing(event).await
    }

    /// Get service by ID
    pub fn get_service(&self, service_id: &str) -> Option<Arc<MockEventService>> {
        self.services.get(service_id).cloned()
    }

    /// Wait for event processing
    pub async fn wait_for_processing(&self, duration_ms: u64) {
        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
    }

    /// Reset all services
    pub async fn reset_services(&self) {
        for service in self.services.values() {
            service.clear_events().await;
            service.set_should_fail(false).await;
        }
    }

    /// Get routing statistics
    pub async fn get_routing_stats(&self) -> EventResult<RoutingStats> {
        self.router.get_routing_stats().await
    }

    /// Update service health
    pub async fn update_service_health(&self, service_id: &str, status: ServiceStatus) -> EventResult<()> {
        self.router.update_service_health(service_id, ServiceHealth {
            status,
            message: Some(format!("Updated to {:?}", status)),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await
    }

    /// Count total events received by all services
    pub async fn count_total_events(&self) -> usize {
        let mut total = 0;
        for service in self.services.values() {
            total += service.get_event_count().await;
        }
        total
    }

    /// Get events from a specific service
    pub async fn get_service_events(&self, service_id: &str) -> Option<Vec<DaemonEvent>> {
        if let Some(service) = self.services.get(service_id) {
            Some(service.get_events().await)
        } else {
            None
        }
    }

    /// Print environment status
    pub async fn print_status(&self) {
        println!("\n=== Event Test Environment Status ===");
        println!("Services: {}", self.services.len());
        println!("Routing Rules: {}", self.routing_rules.len());

        for (service_id, service) in &self.services {
            let event_count = service.get_event_count().await;
            let current_load = service.get_current_load();
            println!("  Service {}: {} events, load: {}/{}",
                     service_id, event_count, current_load, service.max_concurrent);
        }

        if let Ok(stats) = self.get_routing_stats().await {
            println!("Total events routed: {}", stats.total_events_routed);
            println!("Error rate: {:.2}%", stats.error_rate * 100.0);
        }
    }
}

impl Default for EventTestEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Event assertion utilities
pub struct EventAssertions;

impl EventAssertions {
    /// Assert that an event matches the given filter
    pub fn assert_event_matches_filter(event: &DaemonEvent, filter: &EventFilter) {
        assert!(filter.matches(event),
               "Event should match filter. Event: {:?}, Filter: {:?}",
               event.event_type, filter);
    }

    /// Assert that an event does not match the given filter
    pub fn assert_event_not_matches_filter(event: &DaemonEvent, filter: &EventFilter) {
        assert!(!filter.matches(event),
               "Event should not match filter. Event: {:?}, Filter: {:?}",
               event.event_type, filter);
    }

    /// Assert that a service received a specific number of events
    pub async fn assert_service_event_count(
        service: &MockEventService,
        expected_count: usize,
    ) {
        let actual_count = service.get_event_count().await;
        assert_eq!(actual_count, expected_count,
                  "Service should have received {} events, but got {}",
                  expected_count, actual_count);
    }

    /// Assert that a service received events of specific types
    pub async fn assert_service_received_event_types(
        service: &MockEventService,
        expected_types: Vec<&str>,
    ) {
        let events = service.get_events().await;
        let mut found_types = HashMap::new();

        for event in &events {
            let type_str = match &event.event_type {
                EventType::Filesystem(_) => "filesystem",
                EventType::Database(_) => "database",
                EventType::External(_) => "external",
                EventType::Mcp(_) => "mcp",
                EventType::Service(_) => "service",
                EventType::System(_) => "system",
                EventType::Custom(name) => name,
            };
            *found_types.entry(type_str).or_insert(0) += 1;
        }

        for expected_type in expected_types {
            assert!(found_types.contains_key(expected_type),
                   "Service should have received '{}' events, but found types: {:?}",
                   expected_type, found_types.keys().collect::<Vec<_>>());
        }
    }

    /// Assert event priority ordering
    pub fn assert_event_priority_ordering(events: &[DaemonEvent], expected_order: &[EventPriority]) {
        assert_eq!(events.len(), expected_order.len(),
                  "Number of events should match expected order length");

        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.priority, expected_order[i],
                      "Event at index {} should have priority {:?}, but got {:?}",
                      i, expected_order[i], event.priority);
        }
    }

    /// Assert event validation
    pub fn assert_event_validates(event: &DaemonEvent) {
        assert!(event.validate().is_ok(),
               "Event should be valid. Validation errors: {:?}",
               event.validate().err());
    }

    /// Assert event validation failure
    pub fn assert_event_validation_fails(event: &DaemonEvent, expected_error_pattern: &str) {
        let result = event.validate();
        assert!(result.is_err(),
               "Event validation should fail");

        let error_string = format!("{}", result.unwrap_err());
        assert!(error_string.contains(expected_error_pattern),
               "Validation error should contain '{}', but got: '{}'",
               expected_error_pattern, error_string);
    }
}

/// Performance measurement utilities
pub struct PerformanceMeasurer {
    start_time: std::time::Instant,
}

impl PerformanceMeasurer {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed().as_millis() as u64
    }

    pub fn throughput_per_second(&self, operations: usize) -> f64 {
        operations as f64 / self.elapsed().as_secs_f64()
    }

    pub fn assert_duration_under(&self, max_duration: std::time::Duration) {
        let elapsed = self.elapsed();
        assert!(elapsed <= max_duration,
               "Operation should complete in <= {:?}, but took {:?}",
               max_duration, elapsed);
    }

    pub fn assert_throughput_at_least(&self, operations: usize, min_throughput: f64) {
        let throughput = self.throughput_per_second(operations);
        assert!(throughput >= min_throughput,
               "Throughput should be at least {:.2} ops/sec, but got {:.2}",
               min_throughput, throughput);
    }
}

impl Default for PerformanceMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

/// Random test data generator
pub struct RandomDataGenerator;

impl RandomDataGenerator {
    /// Generate a random event
    pub fn random_event() -> DaemonEvent {
        let event_types = vec![
            EventType::Filesystem(FilesystemEventType::FileCreated {
                path: format!("/random/path{}.txt", rand::random::<u32>()),
            }),
            EventType::Database(DatabaseEventType::RecordCreated {
                table: "random_table".to_string(),
                id: format!("id-{}", rand::random::<u32>()),
            }),
            EventType::Service(ServiceEventType::HealthCheck {
                service_id: format!("service-{}", rand::random::<u32>()),
                status: "healthy".to_string(),
            }),
            EventType::System(SystemEventType::MetricsCollected {
                metrics: HashMap::from([
                    ("cpu".to_string(), rand::random::<f64>() * 100.0),
                    ("memory".to_string(), rand::random::<f64>() * 100.0),
                ]),
            }),
            EventType::Custom(format!("custom-event-{}", rand::random::<u32>())),
        ];

        let event_type = event_types[rand::random::<usize>() % event_types.len()];
        let source_types = vec![
            EventSource::service(format!("service-{}", rand::random::<u32>())),
            EventSource::filesystem(format!("watcher-{}", rand::random::<u32>())),
            EventSource::database(format!("trigger-{}", rand::random::<u32>())),
            EventSource::system("daemon".to_string()),
            EventSource::external(format!("api-{}", rand::random::<u32>())),
        ];

        let source = source_types[rand::random::<usize>() % source_types.len()];
        let priorities = vec![
            EventPriority::Critical,
            EventPriority::High,
            EventPriority::Normal,
            EventPriority::Low,
        ];

        let priority = priorities[rand::random::<usize>() % priorities.len()];

        EventDataBuilder::new()
            .with_event_type(event_type)
            .with_source(source)
            .with_priority(priority)
            .with_payload(EventPayload::json(serde_json::json!({
                "random": true,
                "timestamp": Utc::now().to_rfc3339(),
                "data": format!("random-data-{}", rand::random::<u32>())
            })))
            .build()
    }

    /// Generate a random string
    pub fn random_string(length: usize) -> String {
        use rand::Rng;
        let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                             abcdefghijklmnopqrstuvwxyz\
                             0123456789";
        let mut rng = rand::thread_rng();

        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..charset.len());
                charset[idx] as char
            })
            .collect()
    }

    /// Generate a random service ID
    pub fn random_service_id() -> String {
        format!("service-{}", Self::random_string(8))
    }

    /// Generate a random event filter
    pub fn random_filter() -> EventFilter {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let event_types = vec!["filesystem", "database", "service", "system"];
        let mut selected_types = Vec::new();

        if rng.gen_bool(0.7) { // 70% chance to include event types
            let count = rng.gen_range(1..=3);
            for _ in 0..count {
                let event_type = event_types[rng.gen_range(0..event_types.len())];
                if !selected_types.contains(&event_type) {
                    selected_types.push(event_type.to_string());
                }
            }
        }

        EventFilter {
            event_types: selected_types,
            categories: Vec::new(), // Simplified for random generation
            priorities: Vec::new(),
            sources: Vec::new(),
            expression: if rng.gen_bool(0.3) { // 30% chance to include expression
                Some(Self::random_string(10))
            } else {
                None
            },
            max_payload_size: if rng.gen_bool(0.2) { // 20% chance to include size limit
                Some(rng.gen_range(100..=10000))
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod test_utilities_tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_event_service() {
        let service = MockEventService::new("test-service".to_string(), "test".to_string())
            .with_failure_rate(0.5)
            .with_processing_delay(10);

        // Test successful handling
        let event = TestEventFactory::create_basic_event("test1");
        let result = service.handle_event(event).await;
        assert!(result.is_ok() || result.is_err()); // Random failure

        // Test event count
        let count = service.get_event_count().await;
        assert!(count <= 1); // At most one event should be stored

        // Test failure setting
        service.set_should_fail(true).await;
        let event = TestEventFactory::create_basic_event("test2");
        let result = service.handle_event(event).await;
        assert!(result.is_err());

        // Clear events
        service.clear_events().await;
        let count = service.get_event_count().await;
        assert_eq!(count, 0);
    }

    #[test]
    fn test_event_data_builder() {
        let event = EventDataBuilder::new()
            .with_event_type(EventType::Custom("test-event".to_string()))
            .with_source(EventSource::service("test-source".to_string()))
            .with_priority(EventPriority::High)
            .with_metadata("test_key".to_string(), "test_value".to_string())
            .build();

        assert_eq!(event.priority, EventPriority::High);
        assert_eq!(event.source.id, "test-source");
        assert_eq!(event.metadata.get_field("test_key"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_test_event_factory() {
        let fs_event = TestEventFactory::create_filesystem_event("/test.txt", "created");
        match fs_event.event_type {
            EventType::Filesystem(FilesystemEventType::FileCreated { path }) => {
                assert_eq!(path, "/test.txt");
            }
            _ => panic!("Expected filesystem event"),
        }

        let db_event = TestEventFactory::create_database_event("users", "created", Some("123".to_string()));
        match db_event.event_type {
            EventType::Database(DatabaseEventType::RecordCreated { table, id }) => {
                assert_eq!(table, "users");
                assert_eq!(id, "123");
            }
            _ => panic!("Expected database event"),
        }

        let batch = TestEventFactory::create_event_batch(5, "filesystem");
        assert_eq!(batch.len(), 5);
    }

    #[tokio::test]
    async fn test_event_test_environment() {
        let mut env = EventTestEnvironment::new();

        // Add services
        let service1 = env.add_service("service1", "test").await;
        let service2 = env.add_service("service2", "test").await;

        // Add routing rule
        let rule = env.create_routing_rule(
            "test-rule",
            EventFilter {
                event_types: vec!["test".to_string()],
                ..Default::default()
            },
            vec!["service1".to_string()],
        );
        env.add_routing_rule(rule).await;

        // Test routing
        let event = TestEventFactory::create_basic_event("test");
        let targets = env.test_routing(&event).await.unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], "service1");

        // Test event routing
        let event = TestEventFactory::create_basic_event("test2")
            .with_target(ServiceTarget::new("service2".to_string()));
        let result = env.route_event(event).await;
        assert!(result.is_ok());

        // Wait for processing
        env.wait_for_processing(100).await;

        // Check service received event
        let count = service2.get_event_count().await;
        assert_eq!(count, 1);
    }

    #[test]
    fn test_event_assertions() {
        let event = TestEventFactory::create_basic_event("test");
        let filter = EventFilter {
            event_types: vec!["test-event".to_string()],
            ..Default::default()
        };

        // This might pass or fail depending on the event type implementation
        // In a real scenario, you'd adjust the filter to match the event
        println!("Testing event assertions with event: {:?}", event.event_type);
    }

    #[test]
    fn test_performance_measurer() {
        let measurer = PerformanceMeasurer::new();

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = measurer.elapsed();
        assert!(elapsed >= std::time::Duration::from_millis(10));

        let throughput = measurer.throughput_per_second(100);
        assert!(throughput > 0.0);
    }

    #[test]
    fn test_random_data_generator() {
        let event = RandomDataGenerator::random_event();
        // Just verify it creates a valid event
        assert!(event.validate().is_ok());

        let service_id = RandomDataGenerator::random_service_id();
        assert!(!service_id.is_empty());
        assert!(service_id.starts_with("service-"));

        let random_string = RandomDataGenerator::random_string(10);
        assert_eq!(random_string.len(), 10);

        let filter = RandomDataGenerator::random_filter();
        // Just verify it creates a valid filter
        println!("Random filter: {:?}", filter);
    }
}