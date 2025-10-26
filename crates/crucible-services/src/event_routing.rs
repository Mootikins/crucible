//! Event routing system with comprehensive logging and debugging
//!
//! This module provides a streamlined event routing system focused on
//! debugging capabilities while maintaining minimal performance overhead.

use super::errors::{ServiceError, ServiceResult};
use super::logging::{EventMetrics, EventTracer};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Basic event type for routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    /// Script execution event
    ScriptExecution,
    /// Tool execution event
    ToolExecution,
    /// System event
    System,
    /// User interaction event
    UserInteraction,
    /// Error event
    Error,
    /// Custom event type
    Custom(String),
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::ScriptExecution => write!(f, "script_execution"),
            EventType::ToolExecution => write!(f, "tool_execution"),
            EventType::System => write!(f, "system"),
            EventType::UserInteraction => write!(f, "user_interaction"),
            EventType::Error => write!(f, "error"),
            EventType::Custom(name) => write!(f, "custom_{}", name),
        }
    }
}

/// Event priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Basic event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event identifier
    pub id: String,
    /// Event type
    pub event_type: EventType,
    /// Event priority
    pub priority: EventPriority,
    /// Event source
    pub source: String,
    /// Event target (destination)
    pub target: Option<String>,
    /// Event payload
    pub payload: serde_json::Value,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Event metadata
    pub metadata: HashMap<String, String>,
}

impl Event {
    /// Create a new event
    pub fn new(event_type: EventType, source: String, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type,
            priority: EventPriority::Normal,
            source,
            target: None,
            payload,
            created_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set event priority
    pub fn with_priority(mut self, priority: EventPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set event target
    pub fn with_target(mut self, target: String) -> Self {
        self.target = Some(target);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Get event age
    pub fn age(&self) -> Duration {
        chrono::Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or_default()
    }
}

/// Event routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Event ID
    pub event_id: String,
    /// Source component
    pub source: String,
    /// Target component(s)
    pub targets: Vec<String>,
    /// Routing strategy used
    pub strategy: RoutingStrategy,
    /// Decision timestamp
    pub decided_at: chrono::DateTime<chrono::Utc>,
    /// Reasoning for the decision
    pub reasoning: String,
}

/// Routing strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingStrategy {
    /// Direct routing to specific target
    Direct,
    /// Broadcast to all subscribers
    Broadcast,
    /// Route based on event type
    TypeBased,
    /// Route based on priority
    PriorityBased,
    /// Custom routing logic
    Custom(String),
}

impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::Direct => write!(f, "direct"),
            RoutingStrategy::Broadcast => write!(f, "broadcast"),
            RoutingStrategy::TypeBased => write!(f, "type_based"),
            RoutingStrategy::PriorityBased => write!(f, "priority_based"),
            RoutingStrategy::Custom(name) => write!(f, "custom_{}", name),
        }
    }
}

/// Event routing result
#[derive(Debug, Clone)]
pub struct RoutingResult {
    /// Event that was routed
    pub event: Event,
    /// Routing decision made
    pub decision: RoutingDecision,
    /// Delivery results for each target
    pub delivery_results: Vec<DeliveryResult>,
    /// Total routing time
    pub routing_time_ms: u64,
}

/// Delivery result for a single target
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    /// Target component
    pub target: String,
    /// Delivery success
    pub success: bool,
    /// Delivery time
    pub delivery_time_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Event handler trait
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Get handler name
    fn handler_name(&self) -> &str;

    /// Check if handler can process the event
    async fn can_handle(&self, event: &Event) -> bool;

    /// Handle the event
    async fn handle_event(&self, event: Event) -> Result<Event, ServiceError>;

    /// Get handler priority
    fn handler_priority(&self) -> EventPriority {
        EventPriority::Normal
    }
}

/// Event router configuration
#[derive(Debug, Clone)]
pub struct EventRouterConfig {
    /// Maximum event age before rejection
    pub max_event_age: Duration,
    /// Maximum concurrent events
    pub max_concurrent_events: usize,
    /// Enable detailed tracing
    pub enable_detailed_tracing: bool,
    /// Default routing strategy
    pub default_strategy: RoutingStrategy,
}

impl Default for EventRouterConfig {
    fn default() -> Self {
        Self {
            max_event_age: Duration::from_secs(300), // 5 minutes
            max_concurrent_events: 1000,
            enable_detailed_tracing: std::env::var("CRUCIBLE_EVENT_TRACE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            default_strategy: RoutingStrategy::TypeBased,
        }
    }
}

/// Event router with comprehensive logging
pub struct EventRouter {
    /// Router configuration
    config: EventRouterConfig,
    /// Registered event handlers
    handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
    /// Event tracer for debugging
    event_tracer: EventTracer,
    /// Event metrics
    metrics: Arc<RwLock<EventMetrics>>,
    /// Routing history for debugging
    routing_history: Arc<RwLock<Vec<RoutingDecision>>>,
    /// Active events tracking
    active_events: Arc<RwLock<HashMap<String, Instant>>>,
}

impl EventRouter {
    /// Create a new event router
    pub fn new(config: EventRouterConfig) -> Self {
        Self {
            event_tracer: EventTracer::new("EventRouter"),
            handlers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(EventMetrics::default())),
            routing_history: Arc::new(RwLock::new(Vec::new())),
            active_events: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register an event handler
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) -> ServiceResult<()> {
        let handler_name = handler.handler_name();
        let handler_priority = handler.handler_priority();

        info!(
            handler_name = %handler_name,
            priority = ?handler_priority,
            "Registering event handler"
        );

        let mut handlers = self.handlers.write().await;
        handlers.push(handler.clone());

        // Sort handlers by priority (higher priority first)
        handlers.sort_by(|a, b| b.handler_priority().cmp(&a.handler_priority()));

        debug!(
            handler_name = %handler_name,
            total_handlers = handlers.len(),
            "Event handler registered successfully"
        );

        Ok(())
    }

    /// Route an event through the system
    pub async fn route_event(&self, event: Event) -> ServiceResult<RoutingResult> {
        let start_time = Instant::now();
        let event_id = event.id.clone();

        // Validate event
        if event.age() > self.config.max_event_age {
            warn!(
                event_id = %event_id,
                event_age_ms = event.age().as_millis(),
                max_age_ms = self.config.max_event_age.as_millis(),
                "Rejecting expired event"
            );
            return Err(ServiceError::ValidationError("Event too old".to_string()));
        }

        // Track active event
        {
            let mut active = self.active_events.write().await;
            if active.len() >= self.config.max_concurrent_events {
                warn!(
                    active_events = active.len(),
                    max_concurrent = self.config.max_concurrent_events,
                    "System at capacity, rejecting event"
                );
                return Err(ServiceError::ExecutionError(
                    "System at capacity".to_string(),
                ));
            }
            active.insert(event_id.clone(), start_time);
        }

        // Log event start
        self.event_tracer.trace_event_start(
            &event_id,
            &event.event_type.to_string(),
            Some(&event.payload),
        );

        debug!(
            event_id = %event_id,
            event_type = %event.event_type,
            source = %event.source,
            priority = ?event.priority,
            "Starting event routing"
        );

        // Find suitable handlers
        let handlers = {
            let handlers_lock = self.handlers.read().await;
            let mut suitable_handlers = Vec::new();

            for handler in handlers_lock.iter() {
                if handler.can_handle(&event).await {
                    suitable_handlers.push(handler.clone());
                }
            }

            suitable_handlers
        };

        if handlers.is_empty() {
            warn!(
                event_id = %event_id,
                event_type = %event.event_type,
                "No handlers found for event"
            );

            // Clean up active event tracking
            let mut active = self.active_events.write().await;
            active.remove(&event_id);

            return Err(ServiceError::ValidationError(
                "No handlers available".to_string(),
            ));
        }

        // Make routing decision
        let decision = self.make_routing_decision(&event, &handlers).await;

        // Log routing decision
        self.event_tracer.trace_routing(
            &event_id,
            &event.source,
            &decision.targets.join(","),
            &decision.strategy.to_string(),
        );

        // Deliver event to handlers
        let mut delivery_results = Vec::new();
        let mut processed_event = event.clone();

        for target in &decision.targets {
            let handler_start = Instant::now();

            match self
                .deliver_to_handler(&processed_event, target, &handlers)
                .await
            {
                Ok(updated_event) => {
                    processed_event = updated_event;
                    delivery_results.push(DeliveryResult {
                        target: target.clone(),
                        success: true,
                        delivery_time_ms: handler_start.elapsed().as_millis() as u64,
                        error: None,
                    });

                    debug!(
                        event_id = %event_id,
                        target = %target,
                        delivery_time_ms = handler_start.elapsed().as_millis(),
                        "Event delivered successfully"
                    );
                }
                Err(e) => {
                    error!(
                        event_id = %event_id,
                        target = %target,
                        error = %e,
                        "Event delivery failed"
                    );

                    delivery_results.push(DeliveryResult {
                        target: target.clone(),
                        success: false,
                        delivery_time_ms: handler_start.elapsed().as_millis() as u64,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let routing_time = start_time.elapsed();
        let routing_time_ms = routing_time.as_millis() as u64;

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            let success_count = delivery_results.iter().filter(|r| r.success).count();
            metrics.record_event(routing_time_ms, success_count == delivery_results.len());
        }

        // Store routing decision for debugging
        {
            let mut history = self.routing_history.write().await;
            history.push(decision.clone());

            // Keep only last 1000 decisions
            if history.len() > 1000 {
                history.remove(0);
            }
        }

        // Clean up active event tracking
        {
            let mut active = self.active_events.write().await;
            active.remove(&event_id);
        }

        // Log completion
        self.event_tracer
            .trace_event_complete(&event_id, routing_time_ms, true);

        info!(
            event_id = %event_id,
            routing_time_ms = routing_time_ms,
            targets_count = decision.targets.len(),
            success_count = delivery_results.iter().filter(|r| r.success).count(),
            "Event routing completed"
        );

        Ok(RoutingResult {
            event: processed_event,
            decision,
            delivery_results,
            routing_time_ms,
        })
    }

    /// Make routing decision based on event and available handlers
    async fn make_routing_decision(
        &self,
        event: &Event,
        handlers: &[Arc<dyn EventHandler>],
    ) -> RoutingDecision {
        let targets: Vec<String> = handlers
            .iter()
            .map(|h| h.handler_name().to_string())
            .collect();

        let strategy = if event.target.is_some() {
            RoutingStrategy::Direct
        } else {
            self.config.default_strategy.clone()
        };

        let reasoning = match &strategy {
            RoutingStrategy::Direct => {
                format!("Direct routing to specified target: {:?}", event.target)
            }
            RoutingStrategy::Broadcast => "Broadcasting to all suitable handlers".to_string(),
            RoutingStrategy::TypeBased => {
                format!("Type-based routing for event type: {}", event.event_type)
            }
            RoutingStrategy::PriorityBased => {
                format!("Priority-based routing for priority: {:?}", event.priority)
            }
            RoutingStrategy::Custom(name) => format!("Custom routing strategy: {}", name),
        };

        RoutingDecision {
            event_id: event.id.clone(),
            source: event.source.clone(),
            targets,
            strategy,
            decided_at: chrono::Utc::now(),
            reasoning,
        }
    }

    /// Deliver event to a specific handler
    async fn deliver_to_handler(
        &self,
        event: &Event,
        target: &str,
        handlers: &[Arc<dyn EventHandler>],
    ) -> Result<Event, ServiceError> {
        let handler = handlers
            .iter()
            .find(|h| h.handler_name() == target)
            .ok_or_else(|| ServiceError::ServiceNotFound(target.to_string()))?;

        handler.handle_event(event.clone()).await
    }

    /// Get routing metrics
    pub async fn get_metrics(&self) -> EventMetrics {
        self.metrics.read().await.clone()
    }

    /// Get routing history
    pub async fn get_routing_history(&self, limit: Option<usize>) -> Vec<RoutingDecision> {
        let history = self.routing_history.read().await;
        match limit {
            Some(limit) => history.iter().rev().take(limit).cloned().collect(),
            None => history.iter().rev().cloned().collect(),
        }
    }

    /// Get active events count
    pub async fn get_active_events_count(&self) -> usize {
        self.active_events.read().await.len()
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.reset();
        info!("Event routing metrics reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::EventMetrics;

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

        async fn handle_event(&self, event: Event) -> Result<Event, ServiceError> {
            Ok(event)
        }
    }

    #[tokio::test]
    async fn test_event_creation() {
        let event = Event::new(
            EventType::ScriptExecution,
            "test_source".to_string(),
            serde_json::json!({"test": "data"}),
        );

        assert_eq!(event.source, "test_source");
        assert_eq!(event.event_type, EventType::ScriptExecution);
        assert_eq!(event.priority, EventPriority::Normal);
    }

    #[tokio::test]
    async fn test_event_router_registration() {
        let router = EventRouter::new(EventRouterConfig::default());
        let handler = Arc::new(MockEventHandler {
            name: "test_handler".to_string(),
            can_handle_types: vec![EventType::ScriptExecution],
        });

        router.register_handler(handler).await.unwrap();
        assert_eq!(router.get_active_events_count().await, 0);
    }

    #[tokio::test]
    async fn test_event_metrics() {
        let mut metrics = EventMetrics::default();
        metrics.record_event(100, true);
        metrics.record_event(200, false);

        assert_eq!(metrics.total_events, 2);
        assert_eq!(metrics.successful_events, 1);
        assert_eq!(metrics.failed_events, 1);
    }
}
