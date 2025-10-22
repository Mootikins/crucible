//! # Event Service Integration Traits
//!
//! This module provides traits and abstractions for integrating services with the
//! centralized event system. It defines a common interface that all services can
//! implement to participate in event-driven coordination.

use super::core::{DaemonEvent, EventType, EventSource, EventPayload, EventPriority, ServiceTarget};
use super::errors::{EventError, EventResult};
use super::routing::{EventRouter, ServiceRegistration};
use crate::types::ServiceHealth;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Trait for services that can integrate with the event system
#[async_trait]
pub trait EventIntegratedService: Send + Sync {
    /// Get the unique service identifier
    fn service_id(&self) -> &str;

    /// Get the service type for categorization
    fn service_type(&self) -> &str;

    /// Get the event types this service publishes
    fn published_event_types(&self) -> Vec<String>;

    /// Get the event types this service subscribes to
    fn subscribed_event_types(&self) -> Vec<String>;

    /// Handle incoming events from the event router
    async fn handle_daemon_event(&mut self, event: DaemonEvent) -> EventResult<()>;

    /// Get service registration information for the event router
    fn get_service_registration(&self) -> ServiceRegistration {
        ServiceRegistration {
            service_id: self.service_id().to_string(),
            service_type: self.service_type().to_string(),
            instance_id: format!("{}-{}", self.service_id(), Uuid::new_v4()),
            endpoint: None,
            supported_event_types: self.subscribed_event_types(),
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: vec![],
            metadata: HashMap::new(),
        }
    }

    /// Convert service-specific event to daemon event
    fn service_event_to_daemon_event(&self, service_event: &dyn std::any::Any, priority: EventPriority) -> EventResult<DaemonEvent>;

    /// Convert daemon event to service-specific event if applicable
    fn daemon_event_to_service_event(&self, daemon_event: &DaemonEvent) -> Option<Box<dyn std::any::Any>>;
}

/// Trait for services that can publish lifecycle events
#[async_trait]
pub trait EventPublishingService: Send + Sync {
    /// Publish a service lifecycle event
    async fn publish_lifecycle_event(&self, event_type: LifecycleEventType, details: HashMap<String, String>) -> EventResult<()>;

    /// Publish a service health event
    async fn publish_health_event(&self, health: ServiceHealth) -> EventResult<()>;

    /// Publish a service error event
    async fn publish_error_event(&self, error: String, context: Option<HashMap<String, String>>) -> EventResult<()>;

    /// Publish a service metric event
    async fn publish_metric_event(&self, metrics: HashMap<String, f64>) -> EventResult<()>;
}

/// Service lifecycle event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LifecycleEventType {
    /// Service started
    Started,
    /// Service stopped
    Stopped,
    /// Service restarted
    Restarted,
    /// Service configuration updated
    ConfigurationUpdated,
    /// Service entered maintenance mode
    MaintenanceMode,
    /// Service exited maintenance mode
    OperationalMode,
    /// Service registered with event router
    Registered,
    /// Service unregistered from event router
    Unregistered,
}

/// Event subscription configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    /// Event type to subscribe to
    pub event_type: String,
    /// Subscription filter (optional)
    pub filter: Option<EventSubscriptionFilter>,
    /// Whether to include payload in events
    pub include_payload: bool,
    /// Maximum events per second (rate limiting)
    pub max_events_per_second: Option<u32>,
}

/// Event subscription filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscriptionFilter {
    /// Filter by source service
    pub source_service: Option<String>,
    /// Filter by event priority
    pub priority_filter: Option<Vec<EventPriority>>,
    /// Filter by custom criteria
    pub custom_filter: Option<String>,
    /// Include only events with specific payload fields
    pub payload_fields: Option<Vec<String>>,
}

impl Default for EventSubscription {
    fn default() -> Self {
        Self {
            event_type: "*".to_string(), // All events
            filter: None,
            include_payload: true,
            max_events_per_second: None,
        }
    }
}

/// Service event adapter for converting between service-specific and daemon events
pub struct ServiceEventAdapter {
    service_id: String,
    service_type: String,
}

impl ServiceEventAdapter {
    /// Create a new event adapter for a service
    pub fn new(service_id: String, service_type: String) -> Self {
        Self {
            service_id,
            service_type,
        }
    }

    /// Create a daemon event from a service event
    pub fn create_daemon_event(
        &self,
        event_type: EventType,
        payload: EventPayload,
        priority: EventPriority,
        targets: Option<Vec<ServiceTarget>>,
    ) -> DaemonEvent {
        let source = EventSource::service(self.service_id.clone())
            .with_metadata("service_type".to_string(), self.service_type.clone());

        let mut event = DaemonEvent::new(event_type, source, payload)
            .with_priority(priority);

        if let Some(targets) = targets {
            event = event.with_targets(targets);
        }

        event
    }

    /// Create a lifecycle event
    pub fn create_lifecycle_event(
        &self,
        lifecycle_type: LifecycleEventType,
        details: HashMap<String, String>,
    ) -> DaemonEvent {
        let event_type = EventType::Service(crate::events::core::ServiceEventType::ServiceStatusChanged {
            service_id: self.service_id.clone(),
            old_status: "unknown".to_string(),
            new_status: format!("{:?}", lifecycle_type),
        });

        let payload = EventPayload::json(serde_json::json!({
            "lifecycle_type": lifecycle_type,
            "service_id": self.service_id,
            "service_type": self.service_type,
            "details": details,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        self.create_daemon_event(event_type, payload, EventPriority::Normal, None)
    }

    /// Create a health event
    pub fn create_health_event(&self, health: ServiceHealth) -> DaemonEvent {
        let event_type = EventType::Service(crate::events::core::ServiceEventType::HealthCheck {
            service_id: self.service_id.clone(),
            status: format!("{:?}", health.status),
        });

        let payload = EventPayload::json(serde_json::json!({
            "service_id": self.service_id,
            "service_type": self.service_type,
            "health": health,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        self.create_daemon_event(event_type, payload, EventPriority::Normal, None)
    }

    /// Create an error event
    pub fn create_error_event(
        &self,
        error: String,
        context: Option<HashMap<String, String>>,
    ) -> DaemonEvent {
        let event_type = EventType::Service(crate::events::core::ServiceEventType::ConfigurationChanged {
            service_id: self.service_id.clone(),
            changes: HashMap::from([("error".to_string(), serde_json::Value::String(error.clone()))]),
        });

        let payload = EventPayload::json(serde_json::json!({
            "service_id": self.service_id,
            "service_type": self.service_type,
            "error": error,
            "context": context.unwrap_or_default(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        self.create_daemon_event(event_type, payload, EventPriority::High, None)
    }

    /// Create a metric event
    pub fn create_metric_event(&self, metrics: HashMap<String, f64>) -> DaemonEvent {
        let event_type = EventType::Service(crate::events::core::ServiceEventType::RequestReceived {
            from_service: self.service_id.clone(),
            to_service: "metrics".to_string(),
            request: serde_json::json!({ "metrics": metrics }),
        });

        let payload = EventPayload::json(serde_json::json!({
            "service_id": self.service_id,
            "service_type": self.service_type,
            "metrics": metrics,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        self.create_daemon_event(event_type, payload, EventPriority::Low, None)
    }
}

/// Event integration manager for services
pub struct EventIntegrationManager {
    service_id: String,
    service_type: String,
    event_router: Arc<dyn EventRouter>,
    event_adapter: ServiceEventAdapter,
    event_sender: mpsc::UnboundedSender<DaemonEvent>,
    event_receiver: Arc<tokio::sync::RwLock<Option<mpsc::UnboundedReceiver<DaemonEvent>>>>,
}

impl EventIntegrationManager {
    /// Create a new event integration manager
    pub fn new(
        service_id: String,
        service_type: String,
        event_router: Arc<dyn EventRouter>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let event_adapter = ServiceEventAdapter::new(service_id.clone(), service_type.clone());

        Self {
            service_id,
            service_type,
            event_router,
            event_adapter,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
        }
    }

    /// Register the service with the event router
    pub async fn register_service(&self, registration: ServiceRegistration) -> EventResult<()> {
        self.event_router.register_service(registration).await
    }

    /// Unregister the service from the event router
    pub async fn unregister_service(&self) -> EventResult<()> {
        self.event_router.unregister_service(&self.service_id).await
    }

    /// Publish an event to the event router
    pub async fn publish_event(&self, event: DaemonEvent) -> EventResult<()> {
        self.event_router.route_event(event).await
    }

    /// Get the event receiver for incoming events
    pub async fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<DaemonEvent>> {
        self.event_receiver.write().await.take()
    }

    /// Get the event adapter
    pub fn adapter(&self) -> &ServiceEventAdapter {
        &self.event_adapter
    }

    /// Start event processing loop
    pub async fn start_event_processing<F, Fut>(&self, mut event_handler: F) -> EventResult<()>
    where
        F: FnMut(DaemonEvent) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = EventResult<()>> + Send + 'static,
    {
        if let Some(mut receiver) = self.take_event_receiver().await {
            tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    if let Err(e) = event_handler(event).await {
                        tracing::error!("Error handling event in service {}: {}",
                                       std::env::var("SERVICE_ID").unwrap_or_else(|_| "unknown".to_string()), e);
                    }
                }
            });
        }
        Ok(())
    }

    /// Update service health with the event router
    pub async fn update_service_health(&self, health: ServiceHealth) -> EventResult<()> {
        self.event_router.update_service_health(&self.service_id, health).await
    }
}

/// Macro for implementing common event publishing patterns
#[macro_export]
macro_rules! impl_event_publishing {
    ($service_type:ty, $service_id:expr, $service_type_str:expr) => {
        impl $crate::events::integration::EventPublishingService for $service_type {
            async fn publish_lifecycle_event(&self, event_type: $crate::events::integration::LifecycleEventType, details: std::collections::HashMap<String, String>) -> $crate::events::errors::EventResult<()> {
                // Implementation would use the service's event integration manager
                Ok(())
            }

            async fn publish_health_event(&self, health: $crate::types::ServiceHealth) -> $crate::events::errors::EventResult<()> {
                // Implementation would use the service's event integration manager
                Ok(())
            }

            async fn publish_error_event(&self, error: String, context: Option<std::collections::HashMap<String, String>>) -> $crate::events::errors::EventResult<()> {
                // Implementation would use the service's event integration manager
                Ok(())
            }

            async fn publish_metric_event(&self, metrics: std::collections::HashMap<String, f64>) -> $crate::events::errors::EventResult<()> {
                // Implementation would use the service's event integration manager
                Ok(())
            }
        }
    };
}

/// Utility functions for event integration
pub mod utils {
    use super::*;

    /// Convert a timestamp to a chrono DateTime
    pub fn timestamp_to_datetime(timestamp: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| chrono::Utc::now())
    }

    /// Create a correlation ID for related events
    pub fn create_correlation_id() -> Uuid {
        Uuid::new_v4()
    }

    /// Create an event filter for a specific service
    pub fn create_service_filter(service_id: &str) -> crate::events::core::EventFilter {
        let mut filter = crate::events::core::EventFilter::new();
        filter.sources.push(service_id.to_string());
        filter
    }

    /// Create an event filter for specific event types
    pub fn create_event_type_filter(event_types: Vec<String>) -> crate::events::core::EventFilter {
        let mut filter = crate::events::core::EventFilter::new();
        filter.event_types = event_types;
        filter
    }

    /// Validate event payload size
    pub fn validate_payload_size(payload: &EventPayload, max_size: usize) -> EventResult<()> {
        if payload.size_bytes > max_size {
            return Err(EventError::EventTooLarge {
                size: payload.size_bytes,
                max_size,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::core::{EventType, ServiceEventType};

    #[test]
    fn test_service_event_adapter() {
        let adapter = ServiceEventAdapter::new("test-service".to_string(), "test".to_string());

        let lifecycle_event = adapter.create_lifecycle_event(
            LifecycleEventType::Started,
            HashMap::new(),
        );

        assert_eq!(lifecycle_event.source.id, "test-service");
        assert!(matches!(lifecycle_event.event_type, EventType::Service(_)));
    }

    #[test]
    fn test_event_subscription_default() {
        let subscription = EventSubscription::default();
        assert_eq!(subscription.event_type, "*");
        assert!(subscription.include_payload);
        assert!(subscription.filter.is_none());
    }

    #[test]
    fn test_service_filter_creation() {
        let filter = utils::create_service_filter("test-service");
        assert!(filter.sources.contains(&"test-service".to_string()));
    }

    #[test]
    fn test_event_type_filter_creation() {
        let filter = utils::create_event_type_filter(vec!["filesystem".to_string(), "database".to_string()]);
        assert_eq!(filter.event_types.len(), 2);
        assert!(filter.event_types.contains(&"filesystem".to_string()));
    }
}