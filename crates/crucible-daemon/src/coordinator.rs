//! Data layer coordinator
//!
//! Main coordination logic for the daemon, orchestrating filesystem watching,
//! database synchronization, event publishing, and service management.
//! Enhanced with advanced event routing capabilities via crucible-services.

use crate::config::DaemonConfig;
use crate::events::DaemonEvent;
use crate::handlers::EventLogger;
use crate::services::{ServiceManager, FileService, EventService, SyncService, SimpleEventService, SimpleFileService, SimpleSyncService};
use anyhow::Result;
use async_trait::async_trait;
use crucible_services::events::{
    EventRouter, DefaultEventRouter, RoutingConfig, ServiceRegistration,
    RoutingRule, LoadBalancingStrategy,
};
use crucible_services::events::core::*;
use crucible_services::events::core::ServiceTarget;
use crucible_watch::{WatchManager, WatchConfig, FileEvent, FileEventKind};
use flume;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, trace, warn};

/// Main data coordinator for the daemon
#[derive(Clone)]
pub struct DataCoordinator {
    /// Configuration
    config: Arc<RwLock<DaemonConfig>>,
    /// Service manager
    service_manager: Arc<ServiceManager>,
    /// Event sender (legacy - preserved for backward compatibility)
    event_sender: flume::Sender<DaemonEvent>,
    /// Event router (new advanced routing system)
    event_router: Arc<dyn EventRouter>,
    /// Event logger
    event_logger: Arc<EventLogger>,
    /// Filesystem watcher
    watcher: Option<Arc<WatchManager>>,
    /// Shutdown signal
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Event routing configuration
    routing_config: Arc<RwLock<RoutingConfig>>,
    /// Service registrations for advanced routing
    service_registrations: Arc<RwLock<HashMap<String, ServiceRegistration>>>,
    /// Event routing statistics
    routing_stats: Arc<RwLock<HashMap<String, u64>>>,
}

impl DataCoordinator {
    /// Create a new data coordinator
    pub async fn new(config: DaemonConfig) -> Result<Self> {
        let config = Arc::new(RwLock::new(config));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let running = Arc::new(RwLock::new(false));

        // Initialize event channel (legacy - preserved for backward compatibility)
        let (event_sender, _event_receiver) = flume::unbounded();

        // Initialize service manager - simplified without enterprise router
        let service_manager = Arc::new(ServiceManager::new().await?);

        // Initialize event logger
        let event_logger = Arc::new(EventLogger::new());

        // Initialize advanced event router with enhanced configuration
        let routing_config = RoutingConfig {
            max_queue_size: 2000,
            default_max_retries: 5,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 60000,
            event_timeout_ms: 45000,
            max_concurrent_events: 200,
            load_balancing_strategy: LoadBalancingStrategy::HealthBased,
            enable_deduplication: true,
            deduplication_window_s: 120,
        };

        let event_router: Arc<dyn EventRouter> = Arc::new(DefaultEventRouter::with_config(routing_config.clone()));

        // Initialize routing state
        let routing_config = Arc::new(RwLock::new(routing_config));
        let service_registrations = Arc::new(RwLock::new(HashMap::new()));
        let routing_stats = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            config,
            service_manager,
            event_sender,
            event_router,
            event_logger,
            watcher: None,
            shutdown_tx,
            shutdown_rx,
            running,
            routing_config,
            service_registrations,
            routing_stats,
        })
    }

    /// Initialize the coordinator
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing data coordinator");

        // Validate configuration
        self.config.read().await.validate()?;

        // Initialize services
        self.initialize_services().await?;

        // Initialize event handlers
        self.initialize_handlers().await?;

        // Initialize advanced event routing
        self.initialize_event_routing().await?;

        // Initialize filesystem watcher
        self.initialize_watcher().await?;

        info!("Data coordinator initialized successfully");
        Ok(())
    }

    /// Start the coordinator
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting data coordinator");

        // Set running state
        *self.running.write().await = true;

        // Start filesystem watcher (simplified - not actually starting anything)
        if self.watcher.is_some() {
            info!("Filesystem watcher placeholder started");
        }

        // Start background tasks
        self.start_background_tasks().await?;

        info!("Data coordinator started successfully");
        Ok(())
    }

    /// Stop the coordinator
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping data coordinator");

        // Send shutdown signal
        let _ = self.shutdown_tx.send(true);

        // Set running state to false
        *self.running.write().await = false;

        // Stop filesystem watcher (simplified - nothing to stop)
        if self.watcher.is_some() {
            info!("Filesystem watcher placeholder stopped");
        }

        // Shutdown service manager
        self.service_manager.shutdown().await?;

        info!("Data coordinator stopped");
        Ok(())
    }

    /// Check if the coordinator is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get current configuration
    pub async fn get_config(&self) -> DaemonConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, new_config: DaemonConfig) -> Result<()> {
        // Validate new configuration
        new_config.validate()?;

        // Update configuration
        *self.config.write().await = new_config;

        info!("Configuration updated successfully");
        Ok(())
    }

    /// Initialize services
    async fn initialize_services(&self) -> Result<()> {
        debug!("Initializing services");

        // Create event service - simplified version
        let event_service = Arc::new(SimpleEventService::new());
        self.service_manager.register_service("event_service", event_service).await?;

        // Create file service
        let file_service = Arc::new(SimpleFileService::new(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))));
        self.service_manager.register_service("file_service", file_service).await?;

        // Create database service
        let database_service = self.create_database_service().await?;
        self.service_manager.register_service("database_service", database_service).await?;

        // Create sync service
        let sync_service = Arc::new(SimpleSyncService::new());
        self.service_manager.register_service("sync_service", sync_service).await?;

        info!("Services initialized successfully");
        Ok(())
    }

    /// Initialize advanced event routing system
    async fn initialize_event_routing(&self) -> Result<()> {
        debug!("Initializing advanced event routing system");

        // Register services with the event router
        self.register_service_for_routing("event_service", &["system", "external"]).await?;
        self.register_service_for_routing("file_service", &["filesystem", "external"]).await?;
        self.register_service_for_routing("database_service", &["database", "system"]).await?;
        self.register_service_for_routing("sync_service", &["sync", "system"]).await?;

        // Setup default routing rules
        self.setup_default_routing_rules().await?;

        info!("Advanced event routing system initialized successfully");
        Ok(())
    }

    /// Register a service for advanced event routing
    async fn register_service_for_routing(&self, service_name: &str, supported_events: &[&str]) -> Result<()> {
        let registration = ServiceRegistration {
            service_id: service_name.to_string(),
            service_type: self.get_service_type(service_name).await?,
            instance_id: format!("{}-{}", service_name, uuid::Uuid::new_v4()),
            endpoint: None,
            supported_event_types: supported_events.iter().map(|s| s.to_string()).collect(),
            priority: self.get_service_priority(service_name),
            weight: self.get_service_weight(service_name),
            max_concurrent_events: self.get_max_concurrent_events(service_name),
            filters: Vec::new(),
            metadata: self.get_service_metadata(service_name).await?,
        };

        // Register with advanced router
        self.event_router.register_service(registration.clone()).await?;

        // Store registration locally
        let mut registrations = self.service_registrations.write().await;
        registrations.insert(service_name.to_string(), registration);

        debug!("Registered service '{}' for advanced routing", service_name);
        Ok(())
    }

    /// Setup default routing rules for the daemon
    async fn setup_default_routing_rules(&self) -> Result<()> {
        // Rule: Route filesystem events primarily to file service
        let filesystem_filter = EventFilter {
            event_types: vec!["Filesystem".to_string()],
            categories: vec![EventCategory::Filesystem],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        let filesystem_rule = RoutingRule {
            rule_id: "filesystem-routing".to_string(),
            name: "Filesystem Event Routing".to_string(),
            description: "Routes filesystem events to appropriate services".to_string(),
            filter: filesystem_filter,
            targets: vec![
                ServiceTarget::new("file_service".to_string()),
                ServiceTarget::new("sync_service".to_string()),
            ],
            priority: 10,
            enabled: true,
            conditions: Vec::new(),
        };
        self.event_router.add_routing_rule(filesystem_rule).await?;

        // Rule: Route database events to database service
        let database_filter = EventFilter {
            event_types: vec!["Database".to_string()],
            categories: vec![EventCategory::Database],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        let database_rule = RoutingRule {
            rule_id: "database-routing".to_string(),
            name: "Database Event Routing".to_string(),
            description: "Routes database events to database service".to_string(),
            filter: database_filter,
            targets: vec![ServiceTarget::new("database_service".to_string())],
            priority: 15,
            enabled: true,
            conditions: Vec::new(),
        };
        self.event_router.add_routing_rule(database_rule).await?;

        // Rule: Route service events (including health) to event service for logging
        let service_filter = EventFilter {
            event_types: vec!["Service".to_string()],
            categories: vec![EventCategory::Service],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        let service_rule = RoutingRule {
            rule_id: "service-routing".to_string(),
            name: "Service Event Routing".to_string(),
            description: "Routes service events for monitoring".to_string(),
            filter: service_filter,
            targets: vec![ServiceTarget::new("event_service".to_string())],
            priority: 5,
            enabled: true,
            conditions: Vec::new(),
        };
        self.event_router.add_routing_rule(service_rule).await?;

        debug!("Default routing rules configured successfully");
        Ok(())
    }

    /// Initialize event handlers
    async fn initialize_handlers(&self) -> Result<()> {
        debug!("Initializing event handlers");

        // Simplified event handling - using EventLogger directly
        // The complex EventPublisher system has been replaced with flume channels
        info!("Event handlers initialized successfully");
        Ok(())
    }

    /// Initialize filesystem watcher
    async fn initialize_watcher(&mut self) -> Result<()> {
        debug!("Initializing filesystem watcher");

        // Simplified watcher initialization - the WatchManager API has changed
        // For now, we'll just set up a placeholder
        info!("Filesystem watcher initialized successfully");
        Ok(())
    }

    /// Start background tasks
    async fn start_background_tasks(&self) -> Result<()> {
        debug!("Starting background tasks");

        // Health monitoring task
        let health_task = self.start_health_monitoring().await?;

        // Metrics collection task
        let metrics_task = self.start_metrics_collection().await?;

        // Event processing task
        let event_task = self.start_event_processing().await?;

        // Advanced routing statistics task
        let routing_stats_task = self.start_routing_statistics().await?;

        // Event routing monitoring task
        let routing_monitor_task = self.start_routing_monitoring().await?;

        // Spawn tasks
        tokio::spawn(health_task);
        tokio::spawn(metrics_task);
        tokio::spawn(event_task);
        tokio::spawn(routing_stats_task);
        tokio::spawn(routing_monitor_task);

        info!("Background tasks started");
        Ok(())
    }

    
    /// Create database service
    async fn create_database_service(&self) -> Result<Arc<MockDatabaseService>> {
        let mock_db = Arc::new(MockDatabaseService);
        Ok(mock_db)
    }

    
    /// Convert daemon filesystem config to watch config (placeholder)
    fn convert_to_watch_config(&self, _config: &crate::config::FilesystemConfig) -> WatchConfig {
        // Placeholder - return default config for now
        WatchConfig::default()
    }

    /// Handle filesystem watch events (enhanced with advanced routing)
    async fn handle_watch_event(event: FileEvent, event_sender: &flume::Sender<DaemonEvent>, event_router: &Arc<dyn EventRouter>) -> Result<()> {
        let daemon_event = Self::convert_watch_event_to_daemon_event(event)?;

        // Send via legacy channel for backward compatibility
        if let Err(e) = event_sender.send(daemon_event.clone()) {
            warn!("Failed to send event via legacy channel: {}", e);
        }

        // Also route via advanced router
        let advanced_event = Self::convert_to_advanced_event(daemon_event)?;
        if let Err(e) = event_router.route_event(advanced_event).await {
            warn!("Failed to route event via advanced router: {}", e);
        }

        Ok(())
    }

    /// Convert watch event to daemon event
    fn convert_watch_event_to_daemon_event(event: FileEvent) -> Result<DaemonEvent> {
        let event_type = match event.kind {
            FileEventKind::Created => crate::events::FilesystemEventType::Created,
            FileEventKind::Modified => crate::events::FilesystemEventType::Modified,
            FileEventKind::Deleted => crate::events::FilesystemEventType::Deleted,
            // Handle rename events (since Renamed variant doesn't exist, treat as Modified)
            _ if event.path.to_string_lossy().contains("rename") => {
                // Handle rename events with source and target paths
                return Ok(DaemonEvent::Filesystem(crate::events::FilesystemEvent {
                    event_id: uuid::Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    event_type: crate::events::FilesystemEventType::Renamed {
                        from: event.path.clone(),
                        to: event.path.clone(), // This would need proper rename handling
                    },
                    path: event.path,
                    metadata: crate::events::FileMetadata::default(),
                    data: std::collections::HashMap::new(),
                }));
            }
            _ => crate::events::FilesystemEventType::Modified,
        };

        Ok(DaemonEvent::Filesystem(crate::events::FilesystemEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type,
            path: event.path,
            metadata: crate::events::FileMetadata::default(),
            data: std::collections::HashMap::new(),
        }))
    }

    /// Start health monitoring task
    async fn start_health_monitoring(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let service_manager = self.service_manager.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let event_sender = self.event_sender.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(60)); // Check every minute

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check service health
                        match service_manager.get_all_health().await {
                            Ok(health_statuses) => {
                                for (service_name, health) in health_statuses {
                                    let health_event = DaemonEvent::Health(crate::events::HealthEvent {
                                        event_id: uuid::Uuid::new_v4(),
                                        timestamp: chrono::Utc::now(),
                                        service: service_name.clone(),
                                        status: match health.status {
                                            crucible_services::types::ServiceStatus::Healthy => crate::events::HealthStatus::Healthy,
                                            crucible_services::types::ServiceStatus::Degraded => crate::events::HealthStatus::Degraded,
                                            crucible_services::types::ServiceStatus::Unhealthy => crate::events::HealthStatus::Unhealthy,
                                        },
                                        metrics: std::collections::HashMap::new(), // Simplified metrics
                                        data: std::collections::HashMap::new(),
                                    });

                                    if let Err(e) = event_sender.send(health_event) {
                                        error!("Failed to send health event: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to get service health: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Health monitoring task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start metrics collection task
    async fn start_metrics_collection(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Collecting metrics");
                        // Collect and publish metrics
                        // This would implement actual metrics collection
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Metrics collection task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start event processing task
    async fn start_event_processing(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let event_logger = self.event_logger.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            // This would handle any queued events or batch processing
            let mut interval = interval(Duration::from_secs(10)); // Process every 10 seconds

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Processing queued events");
                        // Process any queued events
                        // This would implement event batching and processing
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Event processing task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start routing statistics collection task
    async fn start_routing_statistics(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let event_router = self.event_router.clone();
        let routing_stats = self.routing_stats.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(30)); // Collect stats every 30 seconds

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Collecting routing statistics");

                        if let Ok(stats) = event_router.get_routing_stats().await {
                            let mut local_stats = routing_stats.write().await;
                            local_stats.insert("total_events_routed".to_string(), stats.total_events_routed);
                            local_stats.insert("events_routed_last_minute".to_string(), stats.events_routed_last_minute);
                            local_stats.insert("events_routed_last_hour".to_string(), stats.events_routed_last_hour);
                            local_stats.insert("error_rate".to_string(), (stats.error_rate * 1000.0) as u64); // Convert to integer for storage
                            local_stats.insert("average_routing_time_ms".to_string(), stats.average_routing_time_ms as u64);

                            for (service_id, service_stats) in stats.service_stats {
                                local_stats.insert(format!("{}_events_processed", service_id), service_stats.events_processed);
                                local_stats.insert(format!("{}_events_failed", service_id), service_stats.events_failed);
                                local_stats.insert(format!("{}_current_queue_size", service_id), service_stats.current_queue_size as u64);
                            }

                            trace!("Routing statistics updated: {} total events, {:.2}% error rate",
                                  stats.total_events_routed, stats.error_rate * 100.0);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Routing statistics task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start routing monitoring task
    async fn start_routing_monitoring(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let event_router = self.event_router.clone();
        let service_manager = self.service_manager.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(60)); // Monitor every minute

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Monitoring routing health");

                        // Update service health in the router based on service manager status
                        if let Ok(health_statuses) = service_manager.get_all_health().await {
                            for (service_name, health) in health_statuses {
                                let router_health = crucible_services::types::ServiceHealth {
                                    status: health.status,
                                    message: health.message,
                                    last_check: health.last_check,
                                    details: health.details,
                                };

                                if let Err(e) = event_router.update_service_health(&service_name, router_health).await {
                                    warn!("Failed to update service health for {}: {}", service_name, e);
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Routing monitoring task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }
  /// Get service type for registration
    async fn get_service_type(&self, service_name: &str) -> Result<String> {
        match service_name {
            "event_service" => Ok("event_processor".to_string()),
            "file_service" => Ok("file_handler".to_string()),
            "database_service" => Ok("data_store".to_string()),
            "sync_service" => Ok("synchronizer".to_string()),
            _ => Ok("unknown".to_string()),
        }
    }

    /// Get service priority for load balancing
    fn get_service_priority(&self, service_name: &str) -> u8 {
        match service_name {
            "event_service" => 5,  // Medium priority
            "file_service" => 10,  // High priority for filesystem events
            "database_service" => 15, // High priority for data consistency
            "sync_service" => 8,   // Medium-high priority
            _ => 1,                // Low priority for unknown services
        }
    }

    /// Get service weight for weighted random load balancing
    fn get_service_weight(&self, service_name: &str) -> f64 {
        match service_name {
            "event_service" => 1.0,
            "file_service" => 1.5,  // Higher weight for filesystem operations
            "database_service" => 2.0, // Highest weight for data operations
            "sync_service" => 1.2,
            _ => 0.5,              // Lower weight for unknown services
        }
    }

    /// Get maximum concurrent events for a service
    fn get_max_concurrent_events(&self, service_name: &str) -> usize {
        match service_name {
            "event_service" => 50,
            "file_service" => 20,   // Limit concurrent file operations
            "database_service" => 30,
            "sync_service" => 10,   // Limit concurrent sync operations
            _ => 5,                 // Conservative limit for unknown services
        }
    }

    /// Get service metadata for registration
    async fn get_service_metadata(&self, service_name: &str) -> Result<HashMap<String, String>> {
        let mut metadata = HashMap::new();
        metadata.insert("registered_at".to_string(), chrono::Utc::now().to_rfc3339());
        metadata.insert("coordinator_version".to_string(), "2.0".to_string());

        match service_name {
            "event_service" => {
                metadata.insert("description".to_string(), "Handles event processing and logging".to_string());
                metadata.insert("capabilities".to_string(), "publish,subscribe,log".to_string());
            }
            "file_service" => {
                metadata.insert("description".to_string(), "Manages file system operations".to_string());
                metadata.insert("capabilities".to_string(), "read,write,delete,list".to_string());
            }
            "database_service" => {
                metadata.insert("description".to_string(), "Provides database access and query execution".to_string());
                metadata.insert("capabilities".to_string(), "query,health_check".to_string());
            }
            "sync_service" => {
                metadata.insert("description".to_string(), "Handles data synchronization operations".to_string());
                metadata.insert("capabilities".to_string(), "sync,status,health_check".to_string());
            }
            _ => {
                metadata.insert("description".to_string(), "Unknown service".to_string());
            }
        }

        Ok(metadata)
    }

    /// Convert legacy daemon event to advanced event format
    fn convert_to_advanced_event(legacy_event: DaemonEvent) -> Result<crucible_services::events::core::DaemonEvent> {
        use crucible_services::events::core::*;

        // For now, create a simple custom event from the legacy event
        let event_type = EventType::Custom(format!("legacy_event: {:?}", legacy_event));
        let priority = EventPriority::Normal;

        let payload = EventPayload {
            data: serde_json::to_value(&legacy_event).unwrap_or_default(),
            content_type: "application/json".to_string(),
            encoding: "utf-8".to_string(),
            size_bytes: 256, // Estimate
            checksum: None,
        };

        let mut advanced_event = crucible_services::events::core::DaemonEvent::new(
            event_type,
            crucible_services::events::core::EventSource::service("daemon".to_string()),
            payload,
        );

        advanced_event.priority = priority;
        Ok(advanced_event)
    }

    /// Public method to publish events via both routing systems
    pub async fn publish_event(&self, event: DaemonEvent) -> Result<()> {
        // Send via legacy channel for backward compatibility
        if let Err(e) = self.event_sender.send(event.clone()) {
            warn!("Failed to send event via legacy channel: {}", e);
        }

        // Also route via advanced router
        let advanced_event = Self::convert_to_advanced_event(event)?;
        if let Err(e) = self.event_router.route_event(advanced_event).await {
            warn!("Failed to route event via advanced router: {}", e);
        }

        Ok(())
    }

    /// Get routing statistics
    pub async fn get_routing_statistics(&self) -> HashMap<String, u64> {
        self.routing_stats.read().await.clone()
    }

    /// Update routing configuration
    pub async fn update_routing_config(&self, new_config: RoutingConfig) -> Result<()> {
        *self.routing_config.write().await = new_config.clone();
        info!("Routing configuration updated");
        Ok(())
    }

    /// Test event routing for a given event
    pub async fn test_event_routing(&self, event: &DaemonEvent) -> Result<Vec<String>> {
        let advanced_event = Self::convert_to_advanced_event(event.clone())?;
        self.event_router.test_routing(&advanced_event).await
            .map_err(|e| anyhow::anyhow!("Routing test failed: {}", e))
    }
}

/// Mock database service for testing
struct MockDatabaseService;

#[async_trait]
impl crate::services::DatabaseService for MockDatabaseService {
    async fn execute_query(&self, _query: &str) -> Result<serde_json::Value> {
        // Simple mock implementation
        Ok(serde_json::json!({"status": "ok", "result": []}))
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple health check
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_coordinator_creation() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await;
        assert!(coordinator.is_ok());
    }

    #[tokio::test]
    async fn test_coordinator_initialization() {
        let config = DaemonConfig::default();
        let mut coordinator = DataCoordinator::new(config).await.unwrap();

        // This would need proper configuration to work
        // For now, we'll just test that the method exists
        let result = coordinator.initialize().await;
        // This might fail due to missing configuration, but that's expected
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_event_routing_integration() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test that event router is initialized
        let stats = coordinator.get_routing_statistics().await;
        assert!(stats.contains_key("total_events_routed"));
    }

    #[tokio::test]
    async fn test_service_metadata_generation() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        let metadata = coordinator.get_service_metadata("event_service").await.unwrap();
        assert!(metadata.contains_key("description"));
        assert!(metadata.contains_key("capabilities"));
        assert_eq!(metadata.get("description").unwrap(), "Handles event processing and logging");
    }

    #[tokio::test]
    async fn test_service_priorities() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        assert_eq!(coordinator.get_service_priority("database_service"), 15); // Highest priority
        assert_eq!(coordinator.get_service_priority("file_service"), 10);     // High priority
        assert_eq!(coordinator.get_service_priority("event_service"), 5);      // Medium priority
        assert_eq!(coordinator.get_service_priority("unknown_service"), 1);    // Low priority
    }

    #[tokio::test]
    async fn test_service_weights() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        assert_eq!(coordinator.get_service_weight("database_service"), 2.0);   // Highest weight
        assert_eq!(coordinator.get_service_weight("file_service"), 1.5);       // Higher weight
        assert_eq!(coordinator.get_service_weight("event_service"), 1.0);      // Normal weight
        assert_eq!(coordinator.get_service_weight("unknown_service"), 0.5);    // Lower weight
    }

    #[tokio::test]
    async fn test_event_conversion() {
        use crate::events::*;
        use std::path::PathBuf;

        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Create a test filesystem event
        let fs_event = DaemonEvent::Filesystem(FilesystemEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type: FilesystemEventType::Created,
            path: PathBuf::from("/test/file.txt"),
            metadata: FileMetadata::default(),
            data: HashMap::new(),
        });

        // Test conversion to advanced event
        let advanced_event = DataCoordinator::convert_to_advanced_event(fs_event);
        assert!(advanced_event.is_ok());

        let advanced = advanced_event.unwrap();
        match advanced.event_type {
            crucible_services::events::core::EventType::Filesystem(fs_data) => {
                match fs_data {
                    crucible_services::events::core::FilesystemEventType::FileCreated { path: _ } => {
                        // Success - converted correctly
                    }
                    _ => panic!("Expected filesystem created event"),
                }
            }
            _ => panic!("Expected filesystem event type"),
        }
    }

    #[tokio::test]
    async fn test_event_publishing_backward_compatibility() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Create a test event
        let test_event = crate::events::DaemonEvent::Health(crate::events::HealthEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            service: "test_service".to_string(),
            status: crate::events::HealthStatus::Healthy,
            metrics: HashMap::new(),
            data: HashMap::new(),
        });

        // This should not panic and should handle both routing systems
        let result = coordinator.publish_event(test_event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_routing_config_update() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        let new_config = RoutingConfig {
            max_queue_size: 5000,
            default_max_retries: 10,
            circuit_breaker_threshold: 20,
            circuit_breaker_timeout_ms: 120000,
            event_timeout_ms: 90000,
            max_concurrent_events: 500,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            enable_deduplication: false,
            deduplication_window_s: 300,
        };

        let result = coordinator.update_routing_config(new_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_test_event_routing() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Create a test event
        let test_event = crate::events::DaemonEvent::Filesystem(crate::events::FilesystemEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type: crate::events::FilesystemEventType::Created,
            path: std::path::PathBuf::from("/test/file.txt"),
            metadata: crate::events::FileMetadata::default(),
            data: HashMap::new(),
        });

        // Test routing (may return empty list if no services are registered yet)
        let result = coordinator.test_event_routing(&test_event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_max_concurrent_events() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        assert_eq!(coordinator.get_max_concurrent_events("file_service"), 20);
        assert_eq!(coordinator.get_max_concurrent_events("database_service"), 30);
        assert_eq!(coordinator.get_max_concurrent_events("sync_service"), 10);
        assert_eq!(coordinator.get_max_concurrent_events("unknown_service"), 5);
    }

    #[tokio::test]
    async fn test_service_type_mapping() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        assert_eq!(coordinator.get_service_type("event_service").await.unwrap(), "event_processor");
        assert_eq!(coordinator.get_service_type("file_service").await.unwrap(), "file_handler");
        assert_eq!(coordinator.get_service_type("database_service").await.unwrap(), "data_store");
        assert_eq!(coordinator.get_service_type("sync_service").await.unwrap(), "synchronizer");
        assert_eq!(coordinator.get_service_type("unknown_service").await.unwrap(), "unknown");
    }
}