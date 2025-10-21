//! Event routing logic for daemon coordination

use super::core::{DaemonEvent, EventFilter, EventPriority, ServiceTarget};
use super::errors::{EventError, EventResult};
use crate::types::ServiceHealth;
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Event routing configuration
#[derive(Debug, Clone)]
pub struct RoutingConfig {
    /// Maximum number of events in queue per service
    pub max_queue_size: usize,

    /// Default retry attempts
    pub default_max_retries: u32,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker recovery timeout in milliseconds
    pub circuit_breaker_timeout_ms: u64,

    /// Event timeout in milliseconds
    pub event_timeout_ms: u64,

    /// Maximum concurrent events per service
    pub max_concurrent_events: usize,

    /// Load balancing strategy
    pub load_balancing_strategy: LoadBalancingStrategy,

    /// Enable event deduplication
    pub enable_deduplication: bool,

    /// Deduplication window in seconds
    pub deduplication_window_s: u64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            default_max_retries: 3,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 30000,
            event_timeout_ms: 30000,
            max_concurrent_events: 100,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            enable_deduplication: true,
            deduplication_window_s: 60,
        }
    }
}

/// Load balancing strategies for event routing
#[derive(Debug, Clone, PartialEq)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution
    RoundRobin,

    /// Least connections
    LeastConnections,

    /// Weighted random
    WeightedRandom,

    /// Service health-based routing
    HealthBased,

    /// Priority-based routing
    PriorityBased,
}

/// Event router interface
#[async_trait]
pub trait EventRouter: Send + Sync {
    /// Route an event to appropriate services
    async fn route_event(&self, event: DaemonEvent) -> EventResult<()>;

    /// Register a service for routing
    async fn register_service(&self, service: ServiceRegistration) -> EventResult<()>;

    /// Unregister a service
    async fn unregister_service(&self, service_id: &str) -> EventResult<()>;

    /// Update service health status
    async fn update_service_health(&self, service_id: &str, health: ServiceHealth) -> EventResult<()>;

    /// Get routing statistics
    async fn get_routing_stats(&self) -> EventResult<RoutingStats>;

    /// Add routing rule
    async fn add_routing_rule(&self, rule: RoutingRule) -> EventResult<()>;

    /// Remove routing rule
    async fn remove_routing_rule(&self, rule_id: &str) -> EventResult<()>;

    /// Test routing configuration
    async fn test_routing(&self, event: &DaemonEvent) -> EventResult<Vec<String>>;
}

/// Service registration information
#[derive(Debug, Clone)]
pub struct ServiceRegistration {
    pub service_id: String,
    pub service_type: String,
    pub instance_id: String,
    pub endpoint: Option<String>,
    pub supported_event_types: Vec<String>,
    pub priority: u8,
    pub weight: f64,
    pub max_concurrent_events: usize,
    pub filters: Vec<EventFilter>,
    pub metadata: HashMap<String, String>,
}

/// Routing rule for directing events
#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub rule_id: String,
    pub name: String,
    pub description: String,
    pub filter: EventFilter,
    pub targets: Vec<ServiceTarget>,
    pub priority: u8,
    pub enabled: bool,
    pub conditions: Vec<RoutingCondition>,
}

/// Routing condition
#[derive(Debug, Clone)]
pub enum RoutingCondition {
    /// Time-based condition
    TimeWindow {
        start_hour: u8,
        end_hour: u8,
        timezone: Option<String>,
    },

    /// Load-based condition
    ServiceLoad {
        service_id: String,
        max_load: f64,
    },

    /// Health-based condition
    ServiceHealth {
        service_id: String,
        required_status: String,
    },

    /// Custom condition
    Custom {
        expression: String,
        parameters: HashMap<String, serde_json::Value>,
    },
}

/// Routing statistics
#[derive(Debug, Clone)]
pub struct RoutingStats {
    pub total_events_routed: u64,
    pub events_routed_last_minute: u64,
    pub events_routed_last_hour: u64,
    pub service_stats: HashMap<String, ServiceRoutingStats>,
    pub rule_stats: HashMap<String, RuleRoutingStats>,
    pub error_rate: f64,
    pub average_routing_time_ms: f64,
}

/// Service-specific routing statistics
#[derive(Debug, Clone)]
pub struct ServiceRoutingStats {
    pub service_id: String,
    pub events_received: u64,
    pub events_processed: u64,
    pub events_failed: u64,
    pub average_processing_time_ms: f64,
    pub current_queue_size: usize,
    pub circuit_breaker_open: bool,
    pub last_event_processed: Option<chrono::DateTime<Utc>>,
}

/// Rule-specific routing statistics
#[derive(Debug, Clone)]
pub struct RuleRoutingStats {
    pub rule_id: String,
    pub events_matched: u64,
    pub events_routed: u64,
    pub average_routing_time_ms: f64,
    pub last_matched: Option<chrono::DateTime<Utc>>,
}

/// Default implementation of event router
pub struct DefaultEventRouter {
    config: RoutingConfig,
    services: Arc<DashMap<String, ServiceInfo>>,
    routing_rules: Arc<RwLock<Vec<RoutingRule>>>,
    event_sender: mpsc::UnboundedSender<QueuedEvent>,
    deduplication_cache: Arc<DashMap<String, chrono::DateTime<Utc>>>,
    statistics: Arc<RwLock<RoutingStats>>,
}

/// Internal service information
#[derive(Debug, Clone)]
struct ServiceInfo {
    registration: ServiceRegistration,
    health: ServiceHealth,
    circuit_breaker: CircuitBreaker,
    current_connections: usize,
    last_used: chrono::DateTime<Utc>,
    round_robin_counter: u64,
}

/// Circuit breaker for service resilience
#[derive(Debug, Clone)]
struct CircuitBreaker {
    failure_count: u32,
    last_failure_time: chrono::DateTime<Utc>,
    state: CircuitBreakerState,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Queued event with routing metadata
#[derive(Debug, Clone)]
struct QueuedEvent {
    event: DaemonEvent,
    target_services: Vec<String>,
    queued_at: chrono::DateTime<Utc>,
    retry_count: u32,
}

impl DefaultEventRouter {
    /// Create a new event router with default configuration
    pub fn new() -> Self {
        Self::with_config(RoutingConfig::default())
    }

    /// Create a new event router with custom configuration
    pub fn with_config(config: RoutingConfig) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel::<QueuedEvent>();
        let services = Arc::new(DashMap::new());
        let routing_rules = Arc::new(RwLock::new(Vec::new()));
        let deduplication_cache = Arc::new(DashMap::new());
        let statistics = Arc::new(RwLock::new(RoutingStats {
            total_events_routed: 0,
            events_routed_last_minute: 0,
            events_routed_last_hour: 0,
            service_stats: HashMap::new(),
            rule_stats: HashMap::new(),
            error_rate: 0.0,
            average_routing_time_ms: 0.0,
        }));

        let router = Self {
            config,
            services,
            routing_rules,
            event_sender,
            deduplication_cache,
            statistics,
        };

        // Start event processing loop
        router.start_event_processor(event_receiver);

        router
    }

    /// Start the background event processor
    fn start_event_processor(&self, mut receiver: mpsc::UnboundedReceiver<QueuedEvent>) {
        let services = self.services.clone();
        let config = self.config.clone();
        let statistics = self.statistics.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Cleanup expired deduplication entries
                        Self::cleanup_deduplication_cache(&services).await;
                    }
                    Some(queued_event) = receiver.recv() => {
                        if let Err(e) = Self::process_queued_event(queued_event, &services, &config, &statistics).await {
                            tracing::error!("Failed to process queued event: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Process a queued event
    async fn process_queued_event(
        queued_event: QueuedEvent,
        services: &Arc<DashMap<String, ServiceInfo>>,
        config: &RoutingConfig,
        statistics: &Arc<RwLock<RoutingStats>>,
    ) -> EventResult<()> {
        let event = queued_event.event;
        let target_services = queued_event.target_services;

        for service_id in target_services {
            if let Some(service_info) = services.get(&service_id) {
                // Check circuit breaker
                if service_info.circuit_breaker.state == CircuitBreakerState::Open {
                    continue;
                }

                // Check service health
                if service_info.health.status != crate::types::ServiceStatus::Healthy {
                    continue;
                }

                // Check concurrent event limit
                if service_info.current_connections >= config.max_concurrent_events {
                    continue;
                }

                // Route event to service
                if let Err(e) = Self::deliver_event(&event, &service_id).await {
                    tracing::error!("Failed to deliver event to service {}: {}", service_id, e);

                    // Update circuit breaker
                    Self::handle_service_failure(&service_id, services).await;
                } else {
                    // Update success statistics
                    Self::update_service_stats(service_id, true, Some(event.payload.size_bytes), statistics).await;
                }
            }
        }

        Ok(())
    }

    /// Deliver event to a specific service
    async fn deliver_event(event: &DaemonEvent, service_id: &str) -> EventResult<()> {
        // In a real implementation, this would send the event to the service
        // via IPC, HTTP, or other communication mechanism

        tracing::debug!("Delivering event {} to service {}", event.id, service_id);

        // Simulate event delivery
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(())
    }

    /// Handle service failure for circuit breaker
    async fn handle_service_failure(
        service_id: &str,
        services: &Arc<DashMap<String, ServiceInfo>>,
    ) {
        if let Some(mut service_info) = services.get_mut(service_id) {
            service_info.circuit_breaker.failure_count += 1;
            service_info.circuit_breaker.last_failure_time = Utc::now();

            if service_info.circuit_breaker.failure_count >= 5 {
                service_info.circuit_breaker.state = CircuitBreakerState::Open;
            }
        }
    }

    /// Update service statistics
    async fn update_service_stats(
        service_id: String,
        success: bool,
        _event_size: Option<usize>,
        statistics: &Arc<RwLock<RoutingStats>>,
    ) {
        let mut stats = statistics.write().await;

        let service_stats = stats.service_stats.entry(service_id.clone()).or_insert_with(|| {
            ServiceRoutingStats {
                service_id,
                events_received: 0,
                events_processed: 0,
                events_failed: 0,
                average_processing_time_ms: 0.0,
                current_queue_size: 0,
                circuit_breaker_open: false,
                last_event_processed: None,
            }
        });

        service_stats.events_received += 1;
        if success {
            service_stats.events_processed += 1;
        } else {
            service_stats.events_failed += 1;
        }

        service_stats.last_event_processed = Some(Utc::now());
    }

    /// Cleanup expired deduplication cache entries
    async fn cleanup_deduplication_cache(_services: &Arc<DashMap<String, ServiceInfo>>) {
        // Implementation would remove old entries from deduplication cache
        // This is a placeholder for the actual cleanup logic
    }

    /// Determine target services for an event
    async fn determine_targets(&self, event: &DaemonEvent) -> EventResult<Vec<String>> {
        let mut targets = Vec::new();

        // Check explicit targets in the event
        if !event.targets.is_empty() {
            for target in &event.targets {
                if let Some(service_info) = self.services.get(&target.service_id) {
                    if self.should_route_to_service(event, &service_info, &target.filters) {
                        targets.push(target.service_id.clone());
                    }
                }
            }
        } else {
            // Apply routing rules
            let rules = self.routing_rules.read().await;
            for rule in rules.iter().filter(|r| r.enabled) {
                if rule.filter.matches(event) {
                    for target in &rule.targets {
                        if let Some(service_info) = self.services.get(&target.service_id) {
                            if self.should_route_to_service(event, &service_info, &target.filters) {
                                targets.push(target.service_id.clone());
                            }
                        }
                    }
                }
            }

            // If no explicit targets or rules match, use event type routing
            if targets.is_empty() {
                targets.extend(self.route_by_event_type(event).await?);
            }
        }

        // Apply load balancing if multiple targets
        if targets.len() > 1 {
            targets = self.apply_load_balancing(event, targets).await?;
        }

        Ok(targets)
    }

    /// Check if event should be routed to a service
    fn should_route_to_service(
        &self,
        event: &DaemonEvent,
        service_info: &ServiceInfo,
        filters: &[EventFilter],
    ) -> bool {
        // Check service health
        if service_info.health.status != crate::types::ServiceStatus::Healthy {
            return false;
        }

        // Check circuit breaker
        if service_info.circuit_breaker.state == CircuitBreakerState::Open {
            return false;
        }

        // Check concurrent event limit
        if service_info.current_connections >= self.config.max_concurrent_events {
            return false;
        }

        // Check if service supports this event type
        let event_type_str = match &event.event_type {
            super::core::EventType::Filesystem(_) => "filesystem",
            super::core::EventType::Database(_) => "database",
            super::core::EventType::External(_) => "external",
            super::core::EventType::Mcp(_) => "mcp",
            super::core::EventType::Service(_) => "service",
            super::core::EventType::System(_) => "system",
            super::core::EventType::Custom(name) => name,
        };

        if !service_info.registration.supported_event_types.contains(&event_type_str.to_string()) {
            return false;
        }

        // Apply filters
        for filter in filters {
            if !filter.matches(event) {
                return false;
            }
        }

        true
    }

    /// Route events based on event type to default services
    async fn route_by_event_type(&self, event: &DaemonEvent) -> EventResult<Vec<String>> {
        let mut targets = Vec::new();

        match &event.event_type {
            super::core::EventType::Filesystem(_) => {
                // Route to McpGateway for file-based MCP tools
                if let Some(_) = self.services.get("mcp-gateway") {
                    targets.push("mcp-gateway".to_string());
                }
            }
            super::core::EventType::Database(_) => {
                // Route to DataStore for database events
                if let Some(_) = self.services.get("datastore") {
                    targets.push("datastore".to_string());
                }
            }
            super::core::EventType::Mcp(_) => {
                // Route to McpGateway for MCP events
                if let Some(_) = self.services.get("mcp-gateway") {
                    targets.push("mcp-gateway".to_string());
                }
            }
            super::core::EventType::Service(_) => {
                // Route to appropriate service based on service event type
                // This would need more sophisticated logic based on the specific service event
            }
            super::core::EventType::System(_) => {
                // System events often need to be broadcast to all services
                for service_entry in self.services.iter() {
                    targets.push(service_entry.key().clone());
                }
            }
            _ => {}
        }

        Ok(targets)
    }

    /// Apply load balancing strategy
    async fn apply_load_balancing(&self, event: &DaemonEvent, targets: Vec<String>) -> EventResult<Vec<String>> {
        if targets.len() <= 1 {
            return Ok(targets);
        }

        match self.config.load_balancing_strategy {
            LoadBalancingStrategy::RoundRobin => {
                self.round_robin_balancing(targets).await
            }
            LoadBalancingStrategy::LeastConnections => {
                self.least_connections_balancing(targets).await
            }
            LoadBalancingStrategy::WeightedRandom => {
                self.weighted_random_balancing(targets).await
            }
            LoadBalancingStrategy::HealthBased => {
                self.health_based_balancing(targets).await
            }
            LoadBalancingStrategy::PriorityBased => {
                self.priority_based_balancing(event, targets).await
            }
        }
    }

    async fn round_robin_balancing(&self, targets: Vec<String>) -> EventResult<Vec<String>> {
        // Find service with lowest round-robin counter
        let mut selected_service = None;
        let mut min_counter = u64::MAX;

        for target in &targets {
            if let Some(service_info) = self.services.get(target) {
                if service_info.round_robin_counter < min_counter {
                    min_counter = service_info.round_robin_counter;
                    selected_service = Some(target.clone());
                }
            }
        }

        if let Some(service) = selected_service {
            // Update counter
            if let Some(mut service_info) = self.services.get_mut(&service) {
                service_info.round_robin_counter += 1;
            }
            Ok(vec![service])
        } else {
            Ok(targets)
        }
    }

    async fn least_connections_balancing(&self, targets: Vec<String>) -> EventResult<Vec<String>> {
        // Find service with least connections
        let mut selected_service = None;
        let mut min_connections = usize::MAX;

        for target in &targets {
            if let Some(service_info) = self.services.get(target) {
                if service_info.current_connections < min_connections {
                    min_connections = service_info.current_connections;
                    selected_service = Some(target.clone());
                }
            }
        }

        if let Some(service) = selected_service {
            Ok(vec![service])
        } else {
            Ok(targets)
        }
    }

    async fn weighted_random_balancing(&self, targets: Vec<String>) -> EventResult<Vec<String>> {
        use rand::seq::SliceRandom;

        let mut weighted_targets = Vec::new();
        for target in &targets {
            if let Some(service_info) = self.services.get(target) {
                let weight = service_info.registration.weight;
                for _ in 0..(weight * 100.0) as usize {
                    weighted_targets.push(target.clone());
                }
            }
        }

        if let Some(selected) = weighted_targets.choose(&mut rand::thread_rng()) {
            Ok(vec![selected.clone()])
        } else {
            Ok(targets)
        }
    }

    async fn health_based_balancing(&self, targets: Vec<String>) -> EventResult<Vec<String>> {
        // Prefer healthy services
        let mut healthy_targets = Vec::new();
        let mut degraded_targets = Vec::new();

        for target in &targets {
            if let Some(service_info) = self.services.get(target) {
                match service_info.health.status {
                    crate::types::ServiceStatus::Healthy => healthy_targets.push(target.clone()),
                    crate::types::ServiceStatus::Degraded => degraded_targets.push(target.clone()),
                    _ => {}
                }
            }
        }

        if !healthy_targets.is_empty() {
            Ok(healthy_targets)
        } else if !degraded_targets.is_empty() {
            Ok(degraded_targets)
        } else {
            Ok(targets)
        }
    }

    async fn priority_based_balancing(&self, event: &DaemonEvent, targets: Vec<String>) -> EventResult<Vec<String>> {
        // Sort by service priority and event priority
        let mut prioritized_targets = targets;
        prioritized_targets.sort_by(|a, b| {
            let priority_a = self.services.get(a)
                .map(|s| s.registration.priority)
                .unwrap_or(u8::MAX);
            let priority_b = self.services.get(b)
                .map(|s| s.registration.priority)
                .unwrap_or(u8::MAX);

            priority_a.cmp(&priority_b)
        });

        // For high priority events, try higher priority services first
        if event.priority == EventPriority::Critical || event.priority == EventPriority::High {
            Ok(prioritized_targets.into_iter().take(1).collect())
        } else {
            Ok(prioritized_targets)
        }
    }
}

#[async_trait]
impl EventRouter for DefaultEventRouter {
    async fn route_event(&self, event: DaemonEvent) -> EventResult<()> {
        // Validate event
        event.validate()?;

        // Check for duplicate events if deduplication is enabled
        if self.config.enable_deduplication {
            let event_key = format!("{}-{:?}", event.id, event.event_type);

            if let Some(last_seen) = self.deduplication_cache.get(&event_key) {
                let time_diff = Utc::now() - *last_seen;
                if time_diff.num_seconds() < self.config.deduplication_window_s as i64 {
                    return Err(EventError::ValidationError("Duplicate event detected".to_string()));
                }
            }

            self.deduplication_cache.insert(event_key, Utc::now());
        }

        // Determine target services
        let target_services = self.determine_targets(&event).await?;

        if target_services.is_empty() {
            return Err(EventError::RoutingError("No target services found for event".to_string()));
        }

        // Queue event for processing
        let queued_event = QueuedEvent {
            event,
            target_services,
            queued_at: Utc::now(),
            retry_count: 0,
        };

        self.event_sender.send(queued_event)
            .map_err(|_| EventError::QueueFull { capacity: self.config.max_queue_size })?;

        Ok(())
    }

    async fn register_service(&self, registration: ServiceRegistration) -> EventResult<()> {
        let service_info = ServiceInfo {
            registration,
            health: ServiceHealth {
                status: crate::types::ServiceStatus::Healthy,
                message: Some("Service registered".to_string()),
                last_check: Utc::now(),
                details: HashMap::new(),
            },
            circuit_breaker: CircuitBreaker {
                failure_count: 0,
                last_failure_time: Utc::now(),
                state: CircuitBreakerState::Closed,
            },
            current_connections: 0,
            last_used: Utc::now(),
            round_robin_counter: 0,
        };

        self.services.insert(service_info.registration.service_id.clone(), service_info);
        Ok(())
    }

    async fn unregister_service(&self, service_id: &str) -> EventResult<()> {
        self.services.remove(service_id);
        Ok(())
    }

    async fn update_service_health(&self, service_id: &str, health: ServiceHealth) -> EventResult<()> {
        let service_status = health.status.clone();
        if let Some(mut service_info) = self.services.get_mut(service_id) {
            service_info.health = health;

            // Reset circuit breaker if service is healthy
            if service_status == crate::types::ServiceStatus::Healthy {
                service_info.circuit_breaker.state = CircuitBreakerState::Closed;
                service_info.circuit_breaker.failure_count = 0;
            }
        }
        Ok(())
    }

    async fn get_routing_stats(&self) -> EventResult<RoutingStats> {
        let stats = self.statistics.read().await;
        Ok(stats.clone())
    }

    async fn add_routing_rule(&self, rule: RoutingRule) -> EventResult<()> {
        let mut rules = self.routing_rules.write().await;
        rules.push(rule);
        Ok(())
    }

    async fn remove_routing_rule(&self, rule_id: &str) -> EventResult<()> {
        let mut rules = self.routing_rules.write().await;
        rules.retain(|r| r.rule_id != rule_id);
        Ok(())
    }

    async fn test_routing(&self, event: &DaemonEvent) -> EventResult<Vec<String>> {
        self.determine_targets(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ServiceStatus, ServiceHealth};

    #[tokio::test]
    async fn test_event_router_registration() {
        let router = DefaultEventRouter::new();

        let registration = ServiceRegistration {
            service_id: "test-service".to_string(),
            service_type: "test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            supported_event_types: vec!["filesystem".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 10,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        let health = ServiceHealth {
            status: ServiceStatus::Healthy,
            message: None,
            last_check: Utc::now(),
            details: HashMap::new(),
        };

        router.update_service_health("test-service", health).await.unwrap();
        router.unregister_service("test-service").await.unwrap();
    }

    #[tokio::test]
    async fn test_routing_rules() {
        let router = DefaultEventRouter::new();

        let rule = RoutingRule {
            rule_id: "test-rule".to_string(),
            name: "Test Rule".to_string(),
            description: "Test routing rule".to_string(),
            filter: EventFilter::new(),
            targets: vec![ServiceTarget::new("test-service".to_string())],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();
        router.remove_routing_rule("test-rule").await.unwrap();
    }

    #[test]
    fn test_load_balancing_strategies() {
        assert_eq!(LoadBalancingStrategy::RoundRobin, LoadBalancingStrategy::RoundRobin);
        assert_ne!(LoadBalancingStrategy::RoundRobin, LoadBalancingStrategy::LeastConnections);
    }

    #[test]
    fn test_routing_config() {
        let config = RoutingConfig::default();
        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.default_max_retries, 3);
    }
}