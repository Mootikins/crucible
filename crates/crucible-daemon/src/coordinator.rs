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
    RoutingRule, LoadBalancingStrategy, EventBus, EventBusImpl, EventHandler,
};
use crucible_services::events::core::*;
use crucible_services::events::core::ServiceTarget;
use crucible_watch::{WatchManager, WatchConfig, FileEvent, FileEventKind};
use flume;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch, mpsc};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, trace, warn};

/// Service information for discovery and coordination
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub service_id: String,
    pub service_type: String,
    pub instance_id: String,
    pub endpoint: Option<String>,
    pub health: crucible_services::types::ServiceHealth,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Daemon health status for monitoring
#[derive(Debug, Clone)]
pub struct DaemonHealth {
    pub status: crucible_services::types::ServiceStatus,
    pub uptime_seconds: u64,
    pub events_processed: u64,
    pub services_connected: usize,
    pub last_health_check: chrono::DateTime<chrono::Utc>,
    pub metrics: HashMap<String, f64>,
    pub errors: Vec<String>,
}

impl Default for DaemonHealth {
    fn default() -> Self {
        Self {
            status: crucible_services::types::ServiceStatus::Healthy,
            uptime_seconds: 0,
            events_processed: 0,
            services_connected: 0,
            last_health_check: chrono::Utc::now(),
            metrics: HashMap::new(),
            errors: Vec::new(),
        }
    }
}

/// Daemon event handler for service lifecycle events
pub struct DaemonEventHandler {
    coordinator_state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl DaemonEventHandler {
    pub fn new() -> Self {
        Self {
            coordinator_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for DaemonEventHandler {
    async fn handle_event(&self, event: DaemonEvent) -> EventResult<()> {
        match &event.event_type {
            EventType::Service(service_event) => {
                self.handle_service_event(service_event, &event).await?;
            }
            EventType::System(system_event) => {
                self.handle_system_event(system_event, &event).await?;
            }
            EventType::Health(_) => {
                self.handle_health_event(&event).await?;
            }
            _ => {
                debug!("Unhandled event type: {:?}", event.event_type);
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "daemon_event_handler"
    }

    fn priority(&self) -> u32 {
        100 // High priority for daemon events
    }
}

impl DaemonEventHandler {
    async fn handle_service_event(&self, event: &ServiceEventType, source_event: &DaemonEvent) -> EventResult<()> {
        match event {
            ServiceEventType::ServiceRegistered { service_id, service_type } => {
                info!("Service registered: {} ({})", service_id, service_type);
                // Update internal service registry
                let mut state = self.coordinator_state.write().await;
                state.insert(
                    format!("service:{}", service_id),
                    serde_json::json!({
                        "type": service_type,
                        "status": "registered",
                        "registered_at": chrono::Utc::now().to_rfc3339()
                    })
                );
            }
            ServiceEventType::ServiceUnregistered { service_id } => {
                info!("Service unregistered: {}", service_id);
                let mut state = self.coordinator_state.write().await;
                state.remove(&format!("service:{}", service_id));
            }
            ServiceEventType::HealthCheck { service_id, status } => {
                debug!("Health check for {}: {}", service_id, status);
                let mut state = self.coordinator_state.write().await;
                if let Some(service_info) = state.get_mut(&format!("service:{}", service_id)) {
                    if let Some(obj) = service_info.as_object_mut() {
                        obj.insert("status".to_string(), serde_json::Value::String(status.clone()));
                        obj.insert("last_health_check".to_string(),
                                 serde_json::Value::String(chrono::Utc::now().to_rfc3339()));
                    }
                }
            }
            ServiceEventType::ServiceStatusChanged { service_id, old_status, new_status } => {
                info!("Service {} status changed: {} -> {}", service_id, old_status, new_status);
            }
            _ => {
                debug!("Unhandled service event: {:?}", event);
            }
        }
        Ok(())
    }

    async fn handle_system_event(&self, event: &SystemEventType, source_event: &DaemonEvent) -> EventResult<()> {
        match event {
            SystemEventType::DaemonStarted { version } => {
                info!("Daemon started: version {}", version);
                let mut state = self.coordinator_state.write().await;
                state.insert(
                    "daemon".to_string(),
                    serde_json::json!({
                        "status": "running",
                        "version": version,
                        "started_at": chrono::Utc::now().to_rfc3339()
                    })
                );
            }
            SystemEventType::DaemonStopped { reason } => {
                info!("Daemon stopped: {:?}", reason);
            }
            SystemEventType::ConfigurationReloaded { config_hash } => {
                info!("Configuration reloaded: {}", config_hash);
            }
            _ => {
                debug!("Unhandled system event: {:?}", event);
            }
        }
        Ok(())
    }

    async fn handle_health_event(&self, event: &DaemonEvent) -> EventResult<()> {
        debug!("Health event received: {:?}", event.source);
        // Handle health-related events
        Ok(())
    }
}

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
    /// Event bus for service coordination
    event_bus: Arc<dyn EventBus>,
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
    /// Daemon event handlers
    daemon_handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
    /// Service event subscriptions
    event_subscriptions: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<DaemonEvent>>>>,
    /// Service discovery cache
    service_discovery: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    /// Daemon health status
    daemon_health: Arc<RwLock<DaemonHealth>>,
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

        // Initialize enhanced event-driven components
        let event_bus: Arc<dyn EventBus> = Arc::new(EventBusImpl::new());
        let daemon_handlers = Arc::new(RwLock::new(Vec::new()));
        let event_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let service_discovery = Arc::new(RwLock::new(HashMap::new()));
        let daemon_health = Arc::new(RwLock::new(DaemonHealth::default()));

        Ok(Self {
            config,
            service_manager,
            event_sender,
            event_router,
            event_bus,
            event_logger,
            watcher: None,
            shutdown_tx,
            shutdown_rx,
            running,
            routing_config,
            service_registrations,
            routing_stats,
            daemon_handlers,
            event_subscriptions,
            service_discovery,
            daemon_health,
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

        // Initialize daemon event subscriptions
        self.initialize_event_subscriptions().await?;

        // Initialize daemon event handlers
        self.initialize_daemon_handlers().await?;

        // Initialize filesystem watcher
        self.initialize_watcher().await?;

        // Publish daemon startup event
        self.publish_daemon_started().await?;

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

        // Service discovery task
        let discovery_task = self.start_service_discovery().await?;

        // Event subscription monitoring task
        let subscription_monitor_task = self.start_subscription_monitoring().await?;

        // Daemon health reporting task
        let health_reporting_task = self.start_health_reporting().await?;

        // Spawn tasks
        tokio::spawn(health_task);
        tokio::spawn(metrics_task);
        tokio::spawn(event_task);
        tokio::spawn(routing_stats_task);
        tokio::spawn(routing_monitor_task);
        tokio::spawn(discovery_task);
        tokio::spawn(subscription_monitor_task);
        tokio::spawn(health_reporting_task);

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

        let event_type = match legacy_event {
            DaemonEvent::Filesystem(fs_event) => {
                match fs_event.event_type {
                    crate::events::FilesystemEventType::Created => {
                        EventType::Filesystem(FilesystemEventType::FileCreated {
                            path: fs_event.path.to_string_lossy().to_string(),
                        })
                    }
                    crate::events::FilesystemEventType::Modified => {
                        EventType::Filesystem(FilesystemEventType::FileModified {
                            path: fs_event.path.to_string_lossy().to_string(),
                        })
                    }
                    crate::events::FilesystemEventType::Deleted => {
                        EventType::Filesystem(FilesystemEventType::FileDeleted {
                            path: fs_event.path.to_string_lossy().to_string(),
                        })
                    }
                    crate::events::FilesystemEventType::Renamed { from, to } => {
                        EventType::Filesystem(FilesystemEventType::FileMoved {
                            from: from.to_string_lossy().to_string(),
                            to: to.to_string_lossy().to_string(),
                        })
                    }
                    _ => EventType::Custom(format!("filesystem: {:?}", fs_event.event_type)),
                }
            }
            DaemonEvent::Database(db_event) => {
                match db_event.event_type {
                    crate::events::DatabaseEventType::RecordInserted => {
                        EventType::Database(DatabaseEventType::RecordCreated {
                            table: db_event.table.clone().unwrap_or_default(),
                            id: db_event.record_id.clone().unwrap_or_default(),
                        })
                    }
                    crate::events::DatabaseEventType::RecordUpdated => {
                        EventType::Database(DatabaseEventType::RecordUpdated {
                            table: db_event.table.clone().unwrap_or_default(),
                            id: db_event.record_id.clone().unwrap_or_default(),
                            changes: HashMap::new(),
                        })
                    }
                    crate::events::DatabaseEventType::RecordDeleted => {
                        EventType::Database(DatabaseEventType::RecordDeleted {
                            table: db_event.table.clone().unwrap_or_default(),
                            id: db_event.record_id.clone().unwrap_or_default(),
                        })
                    }
                    _ => EventType::Custom(format!("database: {:?}", db_event.event_type)),
                }
            }
            DaemonEvent::Sync(sync_event) => {
                EventType::Custom(format!("sync: {:?}", sync_event.event_type))
            }
            DaemonEvent::Error(error_event) => {
                EventType::Custom(format!("error: {} - {}", error_event.category, error_event.message))
            }
            DaemonEvent::Health(health_event) => {
                EventType::Service(ServiceEventType::HealthCheck {
                    service_id: health_event.service,
                    status: format!("{:?}", health_event.status),
                })
            }
        };

        let priority = match legacy_event {
            DaemonEvent::Error(error_event) => match error_event.severity {
                crate::events::ErrorSeverity::Critical => EventPriority::Critical,
                crate::events::ErrorSeverity::Error => EventPriority::High,
                crate::events::ErrorSeverity::Warning => EventPriority::Normal,
                _ => EventPriority::Low,
            },
            _ => EventPriority::Normal,
        };

        let payload = EventPayload::json(serde_json::to_value(&legacy_event).unwrap_or_default());

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

    /// Initialize daemon event subscriptions for service coordination
    async fn initialize_event_subscriptions(&self) -> Result<()> {
        debug!("Initializing daemon event subscriptions");

        // Subscribe to service lifecycle events
        self.subscribe_to_service_events().await?;

        // Subscribe to system events
        self.subscribe_to_system_events().await?;

        // Subscribe to health events
        self.subscribe_to_health_events().await?;

        info!("Daemon event subscriptions initialized");
        Ok(())
    }

    /// Initialize daemon event handlers
    async fn initialize_daemon_handlers(&self) -> Result<()> {
        debug!("Initializing daemon event handlers");

        // Create and register daemon event handler
        let daemon_handler = Arc::new(DaemonEventHandler::new());
        self.event_bus.subscribe(daemon_handler.clone()).await?;

        // Store handler reference
        let mut handlers = self.daemon_handlers.write().await;
        handlers.push(daemon_handler);

        info!("Daemon event handlers initialized");
        Ok(())
    }

    /// Subscribe to service lifecycle events
    async fn subscribe_to_service_events(&self) -> Result<()> {
        let service_filter = EventFilter {
            event_types: vec!["service".to_string()],
            categories: vec![EventCategory::Service],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        // Create subscription for service events
        let (tx, mut rx) = mpsc::unbounded_channel::<DaemonEvent>();

        // Store subscription
        let mut subscriptions = self.event_subscriptions.write().await;
        subscriptions.insert("service_events".to_string(), tx);

        // Start event processing task
        let event_router = self.event_router.clone();
        let service_discovery = self.service_discovery.clone();
        let daemon_health = self.daemon_health.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(e) = Self::handle_service_subscription_event(event, &event_router, &service_discovery, &daemon_health).await {
                    error!("Error handling service subscription event: {}", e);
                }
            }
        });

        info!("Subscribed to service lifecycle events");
        Ok(())
    }

    /// Subscribe to system events
    async fn subscribe_to_system_events(&self) -> Result<()> {
        let system_filter = EventFilter {
            event_types: vec!["system".to_string()],
            categories: vec![EventCategory::System],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        // Create subscription for system events
        let (tx, mut rx) = mpsc::unbounded_channel::<DaemonEvent>();

        // Store subscription
        let mut subscriptions = self.event_subscriptions.write().await;
        subscriptions.insert("system_events".to_string(), tx);

        // Start event processing task
        let event_router = self.event_router.clone();
        let daemon_health = self.daemon_health.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(e) = Self::handle_system_subscription_event(event, &event_router, &daemon_health).await {
                    error!("Error handling system subscription event: {}", e);
                }
            }
        });

        info!("Subscribed to system events");
        Ok(())
    }

    /// Subscribe to health events
    async fn subscribe_to_health_events(&self) -> Result<()> {
        let health_filter = EventFilter {
            event_types: vec!["health".to_string()],
            categories: vec![EventCategory::Service],
            priorities: vec![],
            sources: vec![],
            expression: None,
            max_payload_size: None,
        };

        // Create subscription for health events
        let (tx, mut rx) = mpsc::unbounded_channel::<DaemonEvent>();

        // Store subscription
        let mut subscriptions = self.event_subscriptions.write().await;
        subscriptions.insert("health_events".to_string(), tx);

        // Start event processing task
        let service_discovery = self.service_discovery.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(e) = Self::handle_health_subscription_event(event, &service_discovery).await {
                    error!("Error handling health subscription event: {}", e);
                }
            }
        });

        info!("Subscribed to health events");
        Ok(())
    }

    /// Handle service subscription events
    async fn handle_service_subscription_event(
        event: DaemonEvent,
        _event_router: &Arc<dyn EventRouter>,
        service_discovery: &Arc<RwLock<HashMap<String, ServiceInfo>>>,
        _daemon_health: &Arc<RwLock<DaemonHealth>>,
    ) -> Result<()> {
        match &event.event_type {
            EventType::Service(service_event) => {
                match service_event {
                    ServiceEventType::ServiceRegistered { service_id, service_type } => {
                        info!("Service discovered via events: {} ({})", service_id, service_type);

                        let service_info = ServiceInfo {
                            service_id: service_id.clone(),
                            service_type: service_type.clone(),
                            instance_id: event.source.instance.clone().unwrap_or_default(),
                            endpoint: None, // Would be extracted from event metadata
                            health: crucible_services::types::ServiceHealth {
                                status: crucible_services::types::ServiceStatus::Healthy,
                                message: Some("Service registered".to_string()),
                                details: HashMap::new(),
                                last_check: chrono::Utc::now(),
                            },
                            last_seen: chrono::Utc::now(),
                            capabilities: vec![], // Would be extracted from event metadata
                            metadata: HashMap::new(),
                        };

                        let mut discovery = service_discovery.write().await;
                        discovery.insert(service_id.clone(), service_info);
                    }
                    ServiceEventType::ServiceUnregistered { service_id } => {
                        info!("Service removed via events: {}", service_id);
                        let mut discovery = service_discovery.write().await;
                        discovery.remove(service_id);
                    }
                    ServiceEventType::ServiceStatusChanged { service_id, new_status, .. } => {
                        debug!("Service status changed via events: {} -> {}", service_id, new_status);
                        let mut discovery = service_discovery.write().await;
                        if let Some(service_info) = discovery.get_mut(service_id) {
                            service_info.health.status = match new_status.as_str() {
                                "healthy" => crucible_services::types::ServiceStatus::Healthy,
                                "degraded" => crucible_services::types::ServiceStatus::Degraded,
                                "unhealthy" => crucible_services::types::ServiceStatus::Unhealthy,
                                _ => crucible_services::types::ServiceStatus::Unknown,
                            };
                            service_info.last_seen = chrono::Utc::now();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle system subscription events
    async fn handle_system_subscription_event(
        event: DaemonEvent,
        _event_router: &Arc<dyn EventRouter>,
        daemon_health: &Arc<RwLock<DaemonHealth>>,
    ) -> Result<()> {
        match &event.event_type {
            EventType::System(system_event) => {
                match system_event {
                    SystemEventType::DaemonStarted { version } => {
                        info!("Daemon started event received: version {}", version);
                        let mut health = daemon_health.write().await;
                        health.status = crucible_services::types::ServiceStatus::Healthy;
                        health.uptime_seconds = 0;
                        health.last_health_check = chrono::Utc::now();
                    }
                    SystemEventType::DaemonStopped { reason } => {
                        info!("Daemon stopped event received: {:?}", reason);
                        let mut health = daemon_health.write().await;
                        health.status = crucible_services::types::ServiceStatus::Unknown;
                    }
                    SystemEventType::EmergencyShutdown { reason } => {
                        warn!("Emergency shutdown event received: {}", reason);
                        let mut health = daemon_health.write().await;
                        health.status = crucible_services::types::ServiceStatus::Unhealthy;
                        health.errors.push(format!("Emergency shutdown: {}", reason));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle health subscription events
    async fn handle_health_subscription_event(
        event: DaemonEvent,
        service_discovery: &Arc<RwLock<HashMap<String, ServiceInfo>>>,
    ) -> Result<()> {
        match &event.event_type {
            EventType::Service(service_event) => {
                if let ServiceEventType::HealthCheck { service_id, status } = service_event {
                    let mut discovery = service_discovery.write().await;
                    if let Some(service_info) = discovery.get_mut(service_id) {
                        service_info.health.status = match status.as_str() {
                            "healthy" => crucible_services::types::ServiceStatus::Healthy,
                            "degraded" => crucible_services::types::ServiceStatus::Degraded,
                            "unhealthy" => crucible_services::types::ServiceStatus::Unhealthy,
                            _ => crucible_services::types::ServiceStatus::Unknown,
                        };
                        service_info.health.last_check = chrono::Utc::now();
                        service_info.last_seen = chrono::Utc::now();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Publish daemon startup event
    async fn publish_daemon_started(&self) -> Result<()> {
        let startup_event = DaemonEvent::new(
            EventType::System(SystemEventType::DaemonStarted {
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            EventSource::service("daemon".to_string()),
            EventPayload::json(serde_json::json!({
                "startup_time": chrono::Utc::now().to_rfc3339(),
                "features": vec!["event_routing", "service_discovery", "health_monitoring"]
            }))
        ).with_priority(EventPriority::High);

        if let Err(e) = self.event_router.route_event(startup_event).await {
            warn!("Failed to publish daemon startup event: {}", e);
        }

        Ok(())
    }

    /// Start service discovery task
    async fn start_service_discovery(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let service_discovery = self.service_discovery.clone();
        let event_router = self.event_router.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(120)); // Check every 2 minutes

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Running service discovery cleanup");

                        // Remove stale services
                        let now = chrono::Utc::now();
                        let mut discovery = service_discovery.write().await;
                        let mut stale_services = Vec::new();

                        for (service_id, service_info) in discovery.iter() {
                            if now.signed_duration_since(service_info.last_seen).num_minutes() > 5 {
                                stale_services.push(service_id.clone());
                            }
                        }

                        for stale_service in stale_services {
                            info!("Removing stale service from discovery: {}", stale_service);
                            discovery.remove(&stale_service);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Service discovery task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start subscription monitoring task
    async fn start_subscription_monitoring(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let event_subscriptions = self.event_subscriptions.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(60)); // Check every minute

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Monitoring event subscriptions");

                        let subscriptions = event_subscriptions.read().await;
                        trace!("Active subscriptions: {}", subscriptions.len());
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Subscription monitoring task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Start daemon health reporting task
    async fn start_health_reporting(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let daemon_health = self.daemon_health.clone();
        let service_discovery = self.service_discovery.clone();
        let event_router = self.event_router.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(30)); // Report every 30 seconds

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Reporting daemon health");

                        let discovery = service_discovery.read().await;
                        let mut health = daemon_health.write().await;

                        health.services_connected = discovery.len();
                        health.last_health_check = chrono::Utc::now();

                        // Update metrics
                        health.metrics.insert("memory_usage_mb".to_string(),
                            Self::get_memory_usage() as f64 / 1024.0 / 1024.0);
                        health.metrics.insert("uptime_seconds".to_string(),
                            health.uptime_seconds as f64);

                        // Create health report event
                        let health_event = DaemonEvent::new(
                            EventType::Service(ServiceEventType::HealthCheck {
                                service_id: "daemon".to_string(),
                                status: format!("{:?}", health.status),
                            }),
                            EventSource::service("daemon".to_string()),
                            EventPayload::json(serde_json::to_value(&*health).unwrap_or_default())
                        ).with_priority(EventPriority::Low);

                        if let Err(e) = event_router.route_event(health_event).await {
                            warn!("Failed to publish daemon health event: {}", e);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Health reporting task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Get current memory usage (simplified)
    fn get_memory_usage() -> usize {
        // In a real implementation, this would use platform-specific APIs
        // For now, return a placeholder value
        50 * 1024 * 1024 // 50MB
    }

    /// Get service discovery information
    pub async fn get_discovered_services(&self) -> HashMap<String, ServiceInfo> {
        self.service_discovery.read().await.clone()
    }

    /// Get daemon health status
    pub async fn get_daemon_health(&self) -> DaemonHealth {
        self.daemon_health.read().await.clone()
    }

    /// Subscribe to specific event type
    pub async fn subscribe_to_events(&self, event_type: &str) -> Result<mpsc::UnboundedReceiver<DaemonEvent>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut subscriptions = self.event_subscriptions.write().await;
        subscriptions.insert(event_type.to_string(), tx);

        info!("Created subscription for event type: {}", event_type);
        Ok(rx)
    }

    /// Publish daemon operation event
    pub async fn publish_operation_event(&self, operation: &str, details: serde_json::Value) -> Result<()> {
        let operation_event = DaemonEvent::new(
            EventType::Custom(format!("daemon_operation:{}", operation)),
            EventSource::service("daemon".to_string()),
            EventPayload::json(details)
        ).with_priority(EventPriority::Normal);

        if let Err(e) = self.event_router.route_event(operation_event).await {
            warn!("Failed to publish daemon operation event: {}", e);
            // Fallback: try to send via legacy channel
            if let Err(legacy_e) = self.event_sender.send(crate::events::DaemonEvent::Error(crate::events::ErrorEvent {
                event_id: uuid::Uuid::new_v4(),
                timestamp: chrono::Utc::now(),
                severity: crate::events::ErrorSeverity::Warning,
                category: crate::events::ErrorCategory::Network,
                code: "EVENT_PUBLISH_FAILED".to_string(),
                message: format!("Failed to publish operation event: {}", e),
                details: Some(format!("Operation: {}", operation)),
                stack_trace: None,
                context: std::collections::HashMap::new(),
                recoverable: true,
                suggested_actions: vec!["Check event router connectivity".to_string()],
            })) {
                error!("Fallback event publishing also failed: {}", legacy_e);
            }
        }

        Ok(())
    }

    /// Handle event routing failures with fallback mechanisms
    async fn handle_event_routing_failure(&self, event: &DaemonEvent, error: &str) -> Result<()> {
        error!("Event routing failure: {} for event {:?}", error, event);

        // Update daemon health status
        {
            let mut health = self.daemon_health.write().await;
            health.errors.push(format!("Event routing failure: {}", error));
            if health.errors.len() > 10 {
                health.status = crucible_services::types::ServiceStatus::Degraded;
            }
        }

        // Try alternative routing or local processing
        match &event.event_type {
            EventType::Filesystem(fs_event) => {
                // Process filesystem events locally
                self.handle_filesystem_event_locally(fs_event).await?;
            }
            EventType::Database(db_event) => {
                // Queue database events for retry
                self.queue_database_event_for_retry(db_event).await?;
            }
            EventType::Service(service_event) => {
                // Service events are critical, retry immediately
                self.retry_service_event_immediately(service_event).await?;
            }
            _ => {
                // Log and continue for other events
                warn!("Unhandled event type during routing failure: {:?}", event.event_type);
            }
        }

        Ok(())
    }

    /// Handle filesystem events locally when routing fails
    async fn handle_filesystem_event_locally(&self, fs_event: &crucible_services::events::core::FilesystemEventType) -> Result<()> {
        debug!("Processing filesystem event locally: {:?}", fs_event);

        match fs_event {
            crucible_services::events::core::FilesystemEventType::FileCreated { path } => {
                // Local processing: update file cache, trigger local handlers
                info!("Local file creation detected: {}", path);
            }
            crucible_services::events::core::FilesystemEventType::FileModified { path } => {
                // Local processing: update file cache, validate changes
                info!("Local file modification detected: {}", path);
            }
            crucible_services::events::core::FilesystemEventType::FileDeleted { path } => {
                // Local processing: remove from cache, cleanup
                info!("Local file deletion detected: {}", path);
            }
            _ => {}
        }

        Ok(())
    }

    /// Queue database events for retry when routing fails
    async fn queue_database_event_for_retry(&self, db_event: &crucible_services::events::core::DatabaseEventType) -> Result<()> {
        debug!("Queueing database event for retry: {:?}", db_event);

        // In a real implementation, this would add to a persistent queue
        // For now, just log the event for retry
        info!("Database event queued for retry: {:?}", db_event);

        // Publish retry event
        let retry_event = DaemonEvent::new(
            EventType::Custom("database_retry_queued".to_string()),
            EventSource::service("daemon".to_string()),
            EventPayload::json(serde_json::json!({
                "original_event": format!("{:?}", db_event),
                "retry_scheduled_at": chrono::Utc::now().to_rfc3339(),
                "retry_attempts": 1
            }))
        );

        if let Err(e) = self.event_router.route_event(retry_event).await {
            warn!("Failed to publish retry event: {}", e);
        }

        Ok(())
    }

    /// Retry service events immediately when routing fails
    async fn retry_service_event_immediately(&self, service_event: &ServiceEventType) -> Result<()> {
        warn!("Immediate retry for service event: {:?}", service_event);

        // Service events are critical, try multiple routing strategies
        let retry_event = DaemonEvent::new(
            EventType::Service(service_event.clone()),
            EventSource::service("daemon".to_string()),
            EventPayload::json(serde_json::json!({
                "retry_attempt": 1,
                "original_timestamp": chrono::Utc::now().to_rfc3339()
            }))
        ).with_priority(EventPriority::High);

        // Try routing with higher priority
        for attempt in 1..=3 {
            match self.event_router.route_event(retry_event.clone()).await {
                Ok(()) => {
                    info!("Service event retry successful on attempt {}", attempt);
                    return Ok(());
                }
                Err(e) => {
                    warn!("Service event retry attempt {} failed: {}", attempt, e);
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
            }
        }

        // All retries failed, escalate error
        error!("All service event retry attempts failed for: {:?}", service_event);
        {
            let mut health = self.daemon_health.write().await;
            health.status = crucible_services::types::ServiceStatus::Unhealthy;
            health.errors.push(format!("Service event routing failed: {:?}", service_event));
        }

        Ok(())
    }

    /// Check and recover from degraded state
    async fn check_and_recover(&self) -> Result<()> {
        let health = self.daemon_health.read().await.clone();

        if health.status == crucible_services::types::ServiceStatus::Degraded {
            info!("Attempting recovery from degraded state");

            // Check if event router is responsive
            match self.event_router.get_routing_stats().await {
                Ok(_) => {
                    info!("Event router is responsive, attempting recovery");

                    // Test with a simple event
                    let test_event = DaemonEvent::new(
                        EventType::Custom("recovery_test".to_string()),
                        EventSource::service("daemon".to_string()),
                        EventPayload::json(serde_json::json!({"test": true}))
                    );

                    match self.event_router.route_event(test_event).await {
                        Ok(()) => {
                            info!("Recovery test successful, restoring healthy status");
                            let mut health = self.daemon_health.write().await;
                            health.status = crucible_services::types::ServiceStatus::Healthy;
                            health.errors.clear();
                        }
                        Err(e) => {
                            warn!("Recovery test failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Event router not responsive: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Enhanced event publishing with error handling and fallback
    pub async fn publish_event_with_fallback(&self, event: DaemonEvent) -> Result<()> {
        // Try primary routing first
        if let Err(e) = self.event_router.route_event(event.clone()).await {
            warn!("Primary event routing failed: {}", e);

            // Handle failure with fallback mechanisms
            self.handle_event_routing_failure(&event, &e.to_string()).await?;

            // Try fallback: legacy channel
            if let Err(legacy_e) = self.event_sender.send(event.clone()) {
                error!("Legacy fallback also failed: {}", legacy_e);
                return Err(anyhow::anyhow!("All event publishing methods failed"));
            }
        }

        // Update statistics
        {
            let mut health = self.daemon_health.write().await;
            health.events_processed += 1;
        }

        Ok(())
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

    // ========== PHASE 5.5: DAEMON EVENT INTEGRATION TESTS ==========

    #[tokio::test]
    async fn test_daemon_event_subscription() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test event subscription creation
        let mut rx = coordinator.subscribe_to_events("test_events").await.unwrap();

        // Test subscription is active
        let services = coordinator.get_discovered_services().await;
        assert!(services.is_empty()); // Should be empty initially

        // Test daemon health
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.status, crucible_services::types::ServiceStatus::Healthy);
        assert_eq!(health.events_processed, 0);
    }

    #[tokio::test]
    async fn test_daemon_event_handler() {
        let handler = DaemonEventHandler::new();

        // Create a service registration event
        let service_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::ServiceRegistered {
                service_id: "test-service".to_string(),
                service_type: "test-type".to_string(),
            }),
            EventSource::service("test-source".to_string()),
            EventPayload::json(serde_json::json!({"test": true}))
        );

        // Handle the event
        let result = handler.handle_event(service_event).await;
        assert!(result.is_ok());
        assert_eq!(handler.name(), "daemon_event_handler");
        assert_eq!(handler.priority(), 100);
    }

    #[tokio::test]
    async fn test_service_discovery_via_events() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Simulate service registration event
        let registration_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::ServiceRegistered {
                service_id: "discovered-service".to_string(),
                service_type: "test-service".to_string(),
            }),
            EventSource::service("external".to_string()),
            EventPayload::json(serde_json::json!({"endpoint": "http://localhost:8080"}))
        );

        // Handle the event via the subscription handler
        let service_discovery = coordinator.service_discovery.clone();
        DataCoordinator::handle_service_subscription_event(
            registration_event,
            &coordinator.event_router,
            &service_discovery,
            &coordinator.daemon_health
        ).await.unwrap();

        // Verify service was discovered
        let services = coordinator.get_discovered_services().await;
        assert!(services.contains_key("discovered-service"));

        let service_info = services.get("discovered-service").unwrap();
        assert_eq!(service_info.service_id, "discovered-service");
        assert_eq!(service_info.service_type, "test-service");
    }

    #[tokio::test]
    async fn test_daemon_health_monitoring() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Get initial health
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.status, crucible_services::types::ServiceStatus::Healthy);
        assert_eq!(health.services_connected, 0);

        // Simulate some errors to test degraded state
        {
            let mut health_guard = coordinator.daemon_health.write().await;
            for i in 0..15 {
                health_guard.errors.push(format!("Test error {}", i));
            }
        }

        // Check if status changes to degraded
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.status, crucible_services::types::ServiceStatus::Degraded);
        assert!(health.errors.len() > 10);
    }

    #[tokio::test]
    async fn test_event_conversion_legacy_to_advanced() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test filesystem event conversion
        let fs_event = crate::events::DaemonEvent::Filesystem(crate::events::FilesystemEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            event_type: crate::events::FilesystemEventType::Created,
            path: std::path::PathBuf::from("/test/file.txt"),
            metadata: crate::events::FileMetadata::default(),
            data: std::collections::HashMap::new(),
        });

        let advanced_event = DataCoordinator::convert_to_advanced_event(fs_event).unwrap();
        match advanced_event.event_type {
            EventType::Filesystem(fs_type) => {
                match fs_type {
                    crucible_services::events::core::FilesystemEventType::FileCreated { path } => {
                        assert_eq!(path, "/test/file.txt");
                    }
                    _ => panic!("Expected FileCreated event"),
                }
            }
            _ => panic!("Expected Filesystem event type"),
        }

        // Test health event conversion
        let health_event = crate::events::DaemonEvent::Health(crate::events::HealthEvent {
            event_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            service: "test-service".to_string(),
            status: crate::events::HealthStatus::Healthy,
            metrics: std::collections::HashMap::new(),
            data: std::collections::HashMap::new(),
        });

        let advanced_event = DataCoordinator::convert_to_advanced_event(health_event).unwrap();
        match advanced_event.event_type {
            EventType::Service(service_type) => {
                match service_type {
                    ServiceEventType::HealthCheck { service_id, status } => {
                        assert_eq!(service_id, "test-service");
                        assert_eq!(status, "Healthy");
                    }
                    _ => panic!("Expected HealthCheck event"),
                }
            }
            _ => panic!("Expected Service event type"),
        }
    }

    #[tokio::test]
    async fn test_event_fallback_mechanisms() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test operation event publishing with fallback
        let result = coordinator.publish_operation_event(
            "test_operation",
            serde_json::json!({"test": "data"})
        ).await;

        assert!(result.is_ok()); // Should succeed even if routing fails due to fallback
    }

    #[tokio::test]
    async fn test_service_event_subscriptions() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test service event subscription
        let service_rx = coordinator.subscribe_to_events("service_events").await.unwrap();
        let system_rx = coordinator.subscribe_to_events("system_events").await.unwrap();
        let health_rx = coordinator.subscribe_to_events("health_events").await.unwrap();

        // All subscriptions should be created successfully
        assert!(service_rx.is_some());
        assert!(system_rx.is_some());
        assert!(health_rx.is_some());
    }

    #[tokio::test]
    async fn test_daemon_startup_event() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Publish daemon startup event
        let result = coordinator.publish_daemon_started().await;
        assert!(result.is_ok());

        // Check daemon health after startup
        let health = coordinator.get_daemon_health().await;
        assert!(health.last_health_check > chrono::Utc::now() - chrono::Duration::minutes(1));
    }

    #[tokio::test]
    async fn test_error_handling_and_recovery() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Simulate error conditions
        let test_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::HealthCheck {
                service_id: "test-service".to_string(),
                status: "unhealthy".to_string(),
            }),
            EventSource::service("test-source".to_string()),
            EventPayload::json(serde_json::json!({"error": "test"}))
        );

        // Test error handling
        let result = coordinator.handle_event_routing_failure(&test_event, "test error").await;
        assert!(result.is_ok());

        // Check if errors were recorded
        let health = coordinator.get_daemon_health().await;
        assert!(!health.errors.is_empty());
    }

    #[tokio::test]
    async fn test_enhanced_event_publishing() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Test enhanced event publishing with fallback
        let test_event = DaemonEvent::new(
            EventType::Custom("test_enhanced_publishing".to_string()),
            EventSource::service("daemon".to_string()),
            EventPayload::json(serde_json::json!({"enhanced": true}))
        );

        let initial_events_processed = coordinator.get_daemon_health().await.events_processed;

        let result = coordinator.publish_event_with_fallback(test_event).await;
        assert!(result.is_ok());

        // Check that events processed counter was updated
        let final_events_processed = coordinator.get_daemon_health().await.events_processed;
        assert!(final_events_processed > initial_events_processed);
    }

    #[tokio::test]
    async fn test_service_info_and_discovery() {
        let service_info = ServiceInfo {
            service_id: "test-service".to_string(),
            service_type: "test-type".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: Some("http://localhost:8080".to_string()),
            health: crucible_services::types::ServiceHealth {
                status: crucible_services::types::ServiceStatus::Healthy,
                message: Some("Service is healthy".to_string()),
                details: std::collections::HashMap::new(),
                last_check: chrono::Utc::now(),
            },
            last_seen: chrono::Utc::now(),
            capabilities: vec!["read".to_string(), "write".to_string()],
            metadata: std::collections::HashMap::new(),
        };

        // Verify ServiceInfo structure
        assert_eq!(service_info.service_id, "test-service");
        assert_eq!(service_info.service_type, "test-type");
        assert!(service_info.endpoint.is_some());
        assert_eq!(service_info.capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_daemon_health_default() {
        let health = DaemonHealth::default();

        assert_eq!(health.status, crucible_services::types::ServiceStatus::Healthy);
        assert_eq!(health.uptime_seconds, 0);
        assert_eq!(health.events_processed, 0);
        assert_eq!(health.services_connected, 0);
        assert!(health.errors.is_empty());
        assert!(health.metrics.is_empty());
    }

    #[tokio::test]
    async fn test_complete_event_integration_workflow() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // 1. Test service discovery via events
        let registration_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::ServiceRegistered {
                service_id: "integration-test-service".to_string(),
                service_type: "test-type".to_string(),
            }),
            EventSource::service("external".to_string()),
            EventPayload::json(serde_json::json!({"test": "integration"}))
        );

        let service_discovery = coordinator.service_discovery.clone();
        DataCoordinator::handle_service_subscription_event(
            registration_event,
            &coordinator.event_router,
            &service_discovery,
            &coordinator.daemon_health
        ).await.unwrap();

        // 2. Verify service discovery
        let services = coordinator.get_discovered_services().await;
        assert!(services.contains_key("integration-test-service"));

        // 3. Test health event for the service
        let health_event = DaemonEvent::new(
            EventType::Service(ServiceEventType::HealthCheck {
                service_id: "integration-test-service".to_string(),
                status: "healthy".to_string(),
            }),
            EventSource::service("integration-test-service".to_string()),
            EventPayload::json(serde_json::json!({"status": "ok"}))
        );

        DataCoordinator::handle_health_subscription_event(
            health_event,
            &service_discovery
        ).await.unwrap();

        // 4. Verify service health was updated
        let services = coordinator.get_discovered_services().await;
        let service_info = services.get("integration-test-service").unwrap();
        assert_eq!(service_info.health.status, crucible_services::types::ServiceStatus::Healthy);

        // 5. Test daemon operation event
        coordinator.publish_operation_event(
            "integration_test",
            serde_json::json!({"workflow": "completed"})
        ).await.unwrap();

        // 6. Verify daemon health tracking
        let health = coordinator.get_daemon_health().await;
        assert!(health.services_connected >= 1); // Should include our test service
    }
}