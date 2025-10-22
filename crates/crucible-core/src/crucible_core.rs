//! # CrucibleCore - Centralized Service Coordination
//!
//! This module provides the central coordinator for the simplified Crucible architecture.
//! It integrates service management, event routing, configuration, and health monitoring
//! into a single, cohesive interface that eliminates complex orchestration overhead.

use super::{
    config::{ConfigManager, CrucibleConfig},
    Result as CoreResult, CrucibleError,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

// Re-export event system components
pub use crucible_services::events::{
    core::{DaemonEvent, EventPayload, EventSource, EventPriority, EventType},
    routing::{DefaultEventRouter, EventRouter, RoutingConfig, ServiceRegistration},
    errors::{EventError, EventResult},
};

// Re-export service traits
pub use crucible_services::service_traits::{
    ServiceLifecycle, HealthCheck, Configurable, Observable, EventDriven, ResourceManager,
    McpGateway, InferenceEngine, ScriptEngine, DataStore, ServiceRegistry,
};

// Re-export service types
pub use crucible_services::types::{
    ServiceHealth, ServiceStatus, ServiceMetrics,
};

/// ============================================================================
/// CRUCIBLE CORE - CENTRAL COORDINATOR
/// ============================================================================

/// Central coordinator for all Crucible services and operations
///
/// This struct serves as the single point of coordination for the entire system,
/// integrating service management, event routing, configuration, and monitoring
/// in a clean, simplified interface.
pub struct CrucibleCore {
    /// Unique identifier for this core instance
    id: Uuid,

    /// Core configuration
    config: Arc<RwLock<CrucibleConfig>>,

    /// Event router for daemon coordination
    event_router: Arc<DefaultEventRouter>,

    /// Configuration manager
    config_manager: Arc<RwLock<ConfigManager>>,

    /// Service registry for managing registered services
    services: Arc<RwLock<HashMap<String, Arc<dyn ServiceLifecycle>>>>,

    /// Event channel for core events
    event_sender: mpsc::UnboundedSender<CoreEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<CoreEvent>>>>,

    /// Health monitoring data
    health_data: Arc<RwLock<CoreHealthData>>,

    /// Metrics collection
    metrics: Arc<RwLock<CoreMetrics>>,

    /// Core state
    state: Arc<RwLock<CoreState>>,
}

/// Core state enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoreState {
    /// Core is initializing
    Initializing,
    /// Core is running and ready
    Running,
    /// Core is shutting down
    ShuttingDown,
    /// Core has stopped
    Stopped,
    /// Core is in error state
    Error(String),
}

/// Core-specific events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEvent {
    /// Service registered with core
    ServiceRegistered { service_id: String, service_type: String },

    /// Service unregistered from core
    ServiceUnregistered { service_id: String },

    /// Service health changed
    ServiceHealthChanged { service_id: String, old_health: ServiceHealth, new_health: ServiceHealth },

    /// Configuration changed
    ConfigurationChanged { changes: Vec<String> },

    /// Core state changed
    StateChanged { old_state: CoreState, new_state: CoreState },

    /// System alert
    SystemAlert { level: AlertLevel, message: String, details: HashMap<String, String> },

    /// Performance metrics collected
    MetricsCollected { metrics: CoreMetricsSnapshot },

    /// Custom core event
    Custom { event_type: String, data: serde_json::Value },
}

/// Alert level for system events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

/// Core health monitoring data
#[derive(Debug, Clone)]
pub struct CoreHealthData {
    /// Overall system health
    pub system_health: ServiceHealth,

    /// Individual service health
    pub service_health: HashMap<String, ServiceHealth>,

    /// Last health check timestamp
    pub last_check: chrono::DateTime<Utc>,

    /// Health check history
    pub health_history: Vec<HealthSnapshot>,
}

/// Health snapshot for historical tracking
#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    pub timestamp: chrono::DateTime<Utc>,
    pub system_health: ServiceHealth,
    pub service_count: usize,
    pub healthy_services: usize,
    pub degraded_services: usize,
    pub unhealthy_services: usize,
}

/// Core metrics collection
#[derive(Debug, Clone)]
pub struct CoreMetrics {
    /// Total events processed
    pub events_processed: u64,

    /// Services managed
    pub services_managed: u64,

    /// Uptime in milliseconds
    pub uptime_ms: u64,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,

    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,

    /// Error rate (errors per 1000 operations)
    pub error_rate: f64,

    /// Last updated timestamp
    pub last_updated: chrono::DateTime<Utc>,
}

/// Core metrics snapshot for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMetricsSnapshot {
    pub timestamp: chrono::DateTime<Utc>,
    pub events_processed: u64,
    pub services_managed: u64,
    pub uptime_ms: u64,
    pub memory_usage_bytes: u64,
    pub avg_response_time_ms: f64,
    pub error_rate: f64,
    pub active_services: Vec<String>,
    pub system_load: f64,
}

/// Core configuration for initialization
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Maximum number of services to manage
    pub max_services: usize,

    /// Event routing configuration
    pub routing_config: RoutingConfig,

    /// Health check interval in seconds
    pub health_check_interval_s: u64,

    /// Metrics collection interval in seconds
    pub metrics_interval_s: u64,

    /// Enable automatic service recovery
    pub enable_auto_recovery: bool,

    /// Maximum recovery attempts per service
    pub max_recovery_attempts: u32,

    /// Event queue size
    pub event_queue_size: usize,

    /// Enable debug logging
    pub enable_debug: bool,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            max_services: 100,
            routing_config: RoutingConfig::default(),
            health_check_interval_s: 30,
            metrics_interval_s: 60,
            enable_auto_recovery: true,
            max_recovery_attempts: 3,
            event_queue_size: 10000,
            enable_debug: false,
        }
    }
}

impl CrucibleCore {
    /// Create a new CrucibleCore instance
    pub async fn new(config: CoreConfig) -> CoreResult<Self> {
        let id = Uuid::new_v4();
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        // Initialize core components
        let event_router = Arc::new(DefaultEventRouter::with_config(config.routing_config.clone()));
        let config_manager = ConfigManager::new().await
            .map_err(|e| CrucibleError::InvalidOperation(format!("Failed to create config manager: {}", e)))?;
        let config_manager = Arc::new(RwLock::new(config_manager));

        let core = Self {
            id,
            config: Arc::new(RwLock::new(CrucibleConfig::default())),
            event_router,
            config_manager,
            services: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            health_data: Arc::new(RwLock::new(CoreHealthData::new())),
            metrics: Arc::new(RwLock::new(CoreMetrics::new())),
            state: Arc::new(RwLock::new(CoreState::Initializing)),
        };

        // Start background tasks
        core.start_background_tasks(config).await?;

        Ok(core)
    }

    /// Start the CrucibleCore and all managed services
    pub async fn start(&self) -> CoreResult<()> {
        self.set_state(CoreState::Running).await;

        // Start event processing
        self.start_event_processor().await?;

        // Start health monitoring
        self.start_health_monitoring().await?;

        // Start metrics collection
        self.start_metrics_collection().await?;

        self.emit_core_event(CoreEvent::StateChanged {
            old_state: CoreState::Initializing,
            new_state: CoreState::Running,
        }).await;

        tracing::info!("CrucibleCore {} started successfully", self.id);
        Ok(())
    }

    /// Stop the CrucibleCore and all managed services
    pub async fn stop(&self) -> CoreResult<()> {
        self.set_state(CoreState::ShuttingDown).await;

        // Stop all registered services
        {
            let services = self.services.read().await;
            for (_service_id, service) in services.iter() {
                // In a real implementation, we'd call service.stop().await here
                // For now, we'll just log
                tracing::info!("Stopping service: {}", service.service_name());
            }
        }

        // Clear services
        {
            let mut services = self.services.write().await;
            services.clear();
        }

        self.set_state(CoreState::Stopped).await;

        self.emit_core_event(CoreEvent::StateChanged {
            old_state: CoreState::ShuttingDown,
            new_state: CoreState::Stopped,
        }).await;

        tracing::info!("CrucibleCore {} stopped successfully", self.id);
        Ok(())
    }

    /// Register a service with the core
    pub async fn register_service<T>(&self, service: Arc<T>) -> CoreResult<()>
    where
        T: ServiceLifecycle + Send + Sync + 'static,
    {
        let service_id = service.service_name().to_string();

        // Check service limit
        {
            let services = self.services.read().await;
            if services.len() >= 100 { // TODO: Make configurable
                return Err(CrucibleError::InvalidOperation(
                    "Maximum number of services reached".to_string()
                ));
            }
        }

        // Register with core
        {
            let mut services = self.services.write().await;
            services.insert(service_id.clone(), service as Arc<dyn ServiceLifecycle>);
        }

        // Register with event router
        let registration = ServiceRegistration {
            service_id: service_id.clone(),
            service_type: "unknown".to_string(), // TODO: Get from service
            instance_id: format!("{}-1", service_id),
            endpoint: None,
            supported_event_types: vec!["system".to_string()], // TODO: Get from service
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: vec![],
            metadata: HashMap::new(),
        };

        self.event_router.register_service(registration).await
            .map_err(|e| CrucibleError::InvalidOperation(format!("Failed to register service with event router: {}", e)))?;

        // Service is now registered with the core
        tracing::info!("Service {} registered with orchestrator", service_id);

        self.emit_core_event(CoreEvent::ServiceRegistered {
            service_id: service_id.clone(),
            service_type: "unknown".to_string(),
        }).await;

        tracing::info!("Service {} registered with CrucibleCore", service_id);
        Ok(())
    }

    /// Unregister a service from the core
    pub async fn unregister_service(&self, service_id: &str) -> CoreResult<()> {
        // Remove from core registry
        {
            let mut services = self.services.write().await;
            services.remove(service_id);
        }

        // Unregister from event router
        self.event_router.unregister_service(service_id).await
            .map_err(|e| CrucibleError::InvalidOperation(format!("Failed to unregister service from event router: {}", e)))?;

        // Service is now unregistered from the core
        tracing::info!("Service {} unregistered from orchestrator", service_id);

        self.emit_core_event(CoreEvent::ServiceUnregistered {
            service_id: service_id.to_string(),
        }).await;

        tracing::info!("Service {} unregistered from CrucibleCore", service_id);
        Ok(())
    }

    /// Get a service by name
    pub async fn get_service(&self, service_id: &str) -> CoreResult<Option<Arc<dyn ServiceLifecycle>>> {
        let services = self.services.read().await;
        Ok(services.get(service_id).cloned())
    }

    /// List all registered services
    pub async fn list_services(&self) -> CoreResult<Vec<String>> {
        let services = self.services.read().await;
        Ok(services.keys().cloned().collect())
    }

    /// Route an event through the system
    pub async fn route_event(&self, event: DaemonEvent) -> CoreResult<()> {
        self.event_router.route_event(event).await
            .map_err(|e| CrucibleError::InvalidOperation(format!("Failed to route event: {}", e)))?;

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.events_processed += 1;
            metrics.last_updated = Utc::now();
        }

        Ok(())
    }

    /// Get current system health
    pub async fn get_system_health(&self) -> CoreResult<ServiceHealth> {
        let health_data = self.health_data.read().await;
        Ok(health_data.system_health.clone())
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> CoreResult<CoreMetricsSnapshot> {
        let metrics = self.metrics.read().await;
        let services = self.services.read().await;

        Ok(CoreMetricsSnapshot {
            timestamp: metrics.last_updated,
            events_processed: metrics.events_processed,
            services_managed: services.len() as u64,
            uptime_ms: metrics.uptime_ms,
            memory_usage_bytes: metrics.memory_usage_bytes,
            avg_response_time_ms: metrics.avg_response_time_ms,
            error_rate: metrics.error_rate,
            active_services: services.keys().cloned().collect(),
            system_load: 0.0, // TODO: Implement system load detection
        })
    }

    /// Get current state
    pub async fn get_state(&self) -> CoreState {
        self.state.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, config: CrucibleConfig) -> CoreResult<()> {
        {
            let mut core_config = self.config.write().await;
            *core_config = config;
        }

        self.emit_core_event(CoreEvent::ConfigurationChanged {
            changes: vec!["core_config_updated".to_string()],
        }).await;

        Ok(())
    }

    /// Perform health check on all services
    pub async fn perform_health_check(&self) -> CoreResult<HashMap<String, ServiceHealth>> {
        let mut health_results = HashMap::new();
        let services = self.services.read().await;

        for (service_id, _service) in services.iter() {
            // Try to perform health check if service supports it
            // Note: This is a simplified approach - in a full implementation,
            // we'd use trait downcasting or a health check registry
            let health = ServiceHealth {
                status: ServiceStatus::Healthy,
                message: Some("Service is running".to_string()),
                last_check: Utc::now(),
                details: HashMap::new(),
            };

            health_results.insert(service_id.clone(), health);
        }

        // Update health data
        {
            let mut health_data = self.health_data.write().await;
            health_data.update_system_health(&health_results);
        }

        Ok(health_results)
    }

    // -------------------------------------------------------------------------
    // PRIVATE HELPER METHODS
    // -------------------------------------------------------------------------

    /// Set core state
    async fn set_state(&self, new_state: CoreState) {
        let mut state = self.state.write().await;
        let old_state = state.clone();
        *state = new_state.clone();

        // Emit state change event
        drop(state);
        self.emit_core_event(CoreEvent::StateChanged { old_state, new_state }).await;
    }

    /// Emit a core event
    async fn emit_core_event(&self, event: CoreEvent) {
        if let Err(e) = self.event_sender.send(event) {
            tracing::error!("Failed to emit core event: {}", e);
        }
    }

    /// Start background tasks
    async fn start_background_tasks(&self, _config: CoreConfig) -> CoreResult<()> {
        // This would start health monitoring, metrics collection, etc.
        // For now, it's a placeholder
        Ok(())
    }

    /// Start event processor
    async fn start_event_processor(&self) -> CoreResult<()> {
        let receiver = {
            let mut receiver_guard = self.event_receiver.write().await;
            receiver_guard.take().ok_or_else(|| {
                CrucibleError::InvalidOperation("Event receiver already taken".to_string())
            })?
        };

        let health_data = self.health_data.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut receiver = receiver;
            while let Some(event) = receiver.recv().await {
                match event {
                    CoreEvent::ServiceHealthChanged { service_id, new_health, .. } => {
                        // Update health data
                        let mut health = health_data.write().await;
                        health.service_health.insert(service_id, new_health);
                        health.last_check = Utc::now();
                    }
                    CoreEvent::MetricsCollected { metrics: new_metrics } => {
                        // Update metrics
                        let mut current_metrics = metrics.write().await;
                        current_metrics.update_from_snapshot(new_metrics);
                    }
                    _ => {
                        tracing::debug!("Received core event: {:?}", event);
                    }
                }
            }
        });

        Ok(())
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) -> CoreResult<()> {
        // Placeholder for health monitoring implementation
        Ok(())
    }

    /// Start metrics collection
    async fn start_metrics_collection(&self) -> CoreResult<()> {
        // Placeholder for metrics collection implementation
        Ok(())
    }
}

// -------------------------------------------------------------------------
// IMPLEMENTATIONS FOR HELPER STRUCTS
// -------------------------------------------------------------------------

impl CoreHealthData {
    pub fn new() -> Self {
        Self {
            system_health: ServiceHealth {
                status: ServiceStatus::Healthy,
                message: Some("System initializing".to_string()),
                last_check: Utc::now(),
                details: HashMap::new(),
            },
            service_health: HashMap::new(),
            last_check: Utc::now(),
            health_history: Vec::new(),
        }
    }

    pub fn update_system_health(&mut self, service_health: &HashMap<String, ServiceHealth>) {
        self.service_health = service_health.clone();
        self.last_check = Utc::now();

        // Calculate overall system health
        let healthy_count = service_health.values()
            .filter(|h| h.status == ServiceStatus::Healthy)
            .count();
        let total_count = service_health.len();

        if total_count == 0 {
            self.system_health.status = ServiceStatus::Degraded;
            self.system_health.message = Some("No services registered".to_string());
        } else if healthy_count == total_count {
            self.system_health.status = ServiceStatus::Healthy;
            self.system_health.message = Some("All services healthy".to_string());
        } else if healthy_count > total_count / 2 {
            self.system_health.status = ServiceStatus::Degraded;
            self.system_health.message = Some(format!("{}/{} services healthy", healthy_count, total_count));
        } else {
            self.system_health.status = ServiceStatus::Unhealthy;
            self.system_health.message = Some(format!("Only {}/{} services healthy", healthy_count, total_count));
        }

        // Add to history
        self.health_history.push(HealthSnapshot {
            timestamp: Utc::now(),
            system_health: self.system_health.clone(),
            service_count: total_count,
            healthy_services: healthy_count,
            degraded_services: service_health.values()
                .filter(|h| h.status == ServiceStatus::Degraded)
                .count(),
            unhealthy_services: service_health.values()
                .filter(|h| h.status == ServiceStatus::Unhealthy)
                .count(),
        });

        // Keep only last 100 snapshots
        if self.health_history.len() > 100 {
            self.health_history.remove(0);
        }
    }
}

impl CoreMetrics {
    pub fn new() -> Self {
        Self {
            events_processed: 0,
            services_managed: 0,
            uptime_ms: 0,
            memory_usage_bytes: 0,
            avg_response_time_ms: 0.0,
            error_rate: 0.0,
            last_updated: Utc::now(),
        }
    }

    pub fn update_from_snapshot(&mut self, snapshot: CoreMetricsSnapshot) {
        self.events_processed = snapshot.events_processed;
        self.services_managed = snapshot.services_managed;
        self.avg_response_time_ms = snapshot.avg_response_time_ms;
        self.error_rate = snapshot.error_rate;
        self.last_updated = snapshot.timestamp;

        // Memory usage and uptime would be calculated separately
        // For now, we'll use placeholder values
        self.memory_usage_bytes = snapshot.memory_usage_bytes;
        self.uptime_ms = snapshot.uptime_ms;
    }
}

/// Builder for creating CrucibleCore instances
pub struct CrucibleCoreBuilder {
    config: CoreConfig,
}

impl CrucibleCoreBuilder {
    pub fn new() -> Self {
        Self {
            config: CoreConfig::default(),
        }
    }

    pub fn with_config(mut self, config: CoreConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_max_services(mut self, max_services: usize) -> Self {
        self.config.max_services = max_services;
        self
    }

    pub fn with_routing_config(mut self, routing_config: RoutingConfig) -> Self {
        self.config.routing_config = routing_config;
        self
    }

    pub fn with_health_check_interval(mut self, interval_s: u64) -> Self {
        self.config.health_check_interval_s = interval_s;
        self
    }

    pub fn with_auto_recovery(mut self, enabled: bool) -> Self {
        self.config.enable_auto_recovery = enabled;
        self
    }

    pub async fn build(self) -> CoreResult<CrucibleCore> {
        CrucibleCore::new(self.config).await
    }
}

impl Default for CrucibleCoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crucible_core_creation() {
        let config = CoreConfig::default();
        let core = CrucibleCore::new(config).await;
        assert!(core.is_ok());
    }

    #[tokio::test]
    async fn test_crucible_core_lifecycle() {
        let core = CrucibleCoreBuilder::new()
            .with_max_services(10)
            .build()
            .await
            .unwrap();

        // Test starting
        assert!(core.start().await.is_ok());
        assert_eq!(core.get_state().await, CoreState::Running);

        // Test stopping
        assert!(core.stop().await.is_ok());
        assert_eq!(core.get_state().await, CoreState::Stopped);
    }

    #[tokio::test]
    async fn test_service_registration() {
        // This test would require a mock service implementation
        // For now, we'll test the basic functionality
        let core = CrucibleCoreBuilder::new().build().await.unwrap();

        let services = core.list_services().await.unwrap();
        assert!(services.is_empty());
    }

    #[tokio::test]
    async fn test_health_monitoring() {
        let core = CrucibleCoreBuilder::new().build().await.unwrap();

        let health = core.get_system_health().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy | ServiceStatus::Degraded));
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let core = CrucibleCoreBuilder::new().build().await.unwrap();

        let metrics = core.get_metrics().await.unwrap();
        assert_eq!(metrics.services_managed, 0);
        assert_eq!(metrics.events_processed, 0);
    }
}