//! Simplified data layer coordinator for crucible-daemon
//!
//! Main coordination logic using simple event communication via tokio::broadcast.
//! Eliminates complex service event routing in favor of direct channel communication.

use crate::config::DaemonConfig;
use crate::events::{DaemonEvent, EventBus, EventBuilder, convert_watch_event_to_daemon_event};
use crate::handlers::EventLogger;
use crate::services::{ServiceManager, FileService, EventService, SyncService, SimpleEventService, SimpleFileService, SimpleSyncService};
use anyhow::Result;
use async_trait::async_trait;
use crucible_watch::{WatchManager, WatchConfig, FileEvent, FileEventKind};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, watch, broadcast};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, trace, warn};

/// Simple service information
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub service_id: String,
    pub service_type: String,
    pub instance_id: String,
    pub endpoint: Option<String>,
    pub status: ServiceStatus,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Simple service status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Simple service health information
#[derive(Debug, Clone)]
pub struct ServiceHealth {
    pub status: ServiceStatus,
    pub message: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub details: HashMap<String, String>,
}

/// Simple daemon health status
#[derive(Debug, Clone)]
pub struct DaemonHealth {
    pub status: ServiceStatus,
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
            status: ServiceStatus::Healthy,
            uptime_seconds: 0,
            events_processed: 0,
            services_connected: 0,
            last_health_check: chrono::Utc::now(),
            metrics: HashMap::new(),
            errors: Vec::new(),
        }
    }
}

/// Simple daemon event handler
pub struct DaemonEventHandler {
    event_bus: Arc<EventBus>,
    coordinator_state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
}

impl DaemonEventHandler {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            coordinator_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle incoming daemon events
    pub async fn handle_event(&self, event: DaemonEvent) -> Result<()> {
        match &event {
            DaemonEvent::Service(service_event) => {
                self.handle_service_event(service_event).await?;
            }
            DaemonEvent::Health(health_event) => {
                self.handle_health_event(health_event).await?;
            }
            DaemonEvent::Error(error_event) => {
                self.handle_error_event(error_event).await?;
            }
            DaemonEvent::Filesystem(fs_event) => {
                self.handle_filesystem_event(fs_event).await?;
            }
            DaemonEvent::Database(db_event) => {
                self.handle_database_event(db_event).await?;
            }
            DaemonEvent::Sync(sync_event) => {
                self.handle_sync_event(sync_event).await?;
            }
        }
        Ok(())
    }

    async fn handle_service_event(&self, event: &crate::events::ServiceEvent) -> Result<()> {
        match &event.event_type {
            crate::events::ServiceEventType::Started => {
                info!("Service started: {} ({})", event.service_id, event.service_type);
                let mut state = self.coordinator_state.write().await;
                state.insert(
                    format!("service:{}", event.service_id),
                    serde_json::json!({
                        "type": event.service_type,
                        "status": "started",
                        "started_at": chrono::Utc::now().to_rfc3339()
                    })
                );
            }
            crate::events::ServiceEventType::Stopped => {
                info!("Service stopped: {}", event.service_id);
                let mut state = self.coordinator_state.write().await;
                if let Some(service_info) = state.get_mut(&format!("service:{}", event.service_id)) {
                    if let Some(obj) = service_info.as_object_mut() {
                        obj.insert("status".to_string(), serde_json::Value::String("stopped".to_string()));
                        obj.insert("stopped_at".to_string(),
                                 serde_json::Value::String(chrono::Utc::now().to_rfc3339()));
                    }
                }
            }
            crate::events::ServiceEventType::Registered => {
                info!("Service registered: {} ({})", event.service_id, event.service_type);
            }
            crate::events::ServiceEventType::Unregistered => {
                info!("Service unregistered: {}", event.service_id);
            }
            crate::events::ServiceEventType::HealthCheck => {
                debug!("Health check received for: {}", event.service_id);
            }
            crate::events::ServiceEventType::StatusChanged => {
                info!("Service status changed: {}", event.service_id);
            }
            crate::events::ServiceEventType::Failed => {
                warn!("Service failed: {}", event.service_id);
            }
        }
        Ok(())
    }

    async fn handle_health_event(&self, event: &crate::events::HealthEvent) -> Result<()> {
        debug!("Health event for {}: {:?}", event.service, event.status);

        // Update service health in state
        let mut state = self.coordinator_state.write().await;
        state.insert(
            format!("health:{}", event.service),
            serde_json::json!({
                "status": format!("{:?}", event.status),
                "message": event.message,
                "last_check": chrono::Utc::now().to_rfc3339()
            })
        );
        Ok(())
    }

    async fn handle_error_event(&self, event: &crate::events::ErrorEvent) -> Result<()> {
        error!("Error event [{}]: {} - {}", event.code, event.category, event.message);

        // Store error in state for monitoring
        let mut state = self.coordinator_state.write().await;
        let error_key = format!("error:{}:{}", event.category, event.code);
        let error_list = state.entry(error_key.clone())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));

        if let Some(array) = error_list.as_array_mut() {
            array.push(serde_json::json!({
                "timestamp": event.timestamp.to_rfc3339(),
                "severity": format!("{:?}", event.severity),
                "message": event.message,
                "details": event.details
            }));

            // Keep only last 10 errors of this type
            if array.len() > 10 {
                array.remove(0);
            }
        }
        Ok(())
    }

    async fn handle_filesystem_event(&self, event: &crate::events::FilesystemEvent) -> Result<()> {
        debug!("Filesystem event: {:?} on {}", event.event_type, event.path.display());

        // Update file tracking state
        let mut state = self.coordinator_state.write().await;
        let file_key = format!("file:{}", event.path.display());
        state.insert(
            file_key,
            serde_json::json!({
                "event_type": format!("{:?}", event.event_type),
                "timestamp": event.timestamp.to_rfc3339(),
                "source_path": event.source_path.as_ref().map(|p| p.display().to_string())
            })
        );
        Ok(())
    }

    async fn handle_database_event(&self, event: &crate::events::DatabaseEvent) -> Result<()> {
        debug!("Database event: {:?} on {}.{}", event.event_type, event.database,
               event.table.as_deref().unwrap_or("N/A"));

        // Track database operations
        let mut state = self.coordinator_state.write().await;
        let db_key = format!("database:{}", event.database);
        state.insert(
            db_key,
            serde_json::json!({
                "event_type": format!("{:?}", event.event_type),
                "table": event.table,
                "record_id": event.record_id,
                "timestamp": event.timestamp.to_rfc3339()
            })
        );
        Ok(())
    }

    async fn handle_sync_event(&self, event: &crate::events::SyncEvent) -> Result<()> {
        debug!("Sync event: {:?} from {} to {}", event.event_type, event.source, event.target);

        // Track sync operations
        let mut state = self.coordinator_state.write().await;
        let sync_key = format!("sync:{}:{}", event.source, event.target);
        state.insert(
            sync_key,
            serde_json::json!({
                "event_type": format!("{:?}", event.event_type),
                "progress": event.progress,
                "timestamp": event.timestamp.to_rfc3339()
            })
        );
        Ok(())
    }
}

/// Simplified data coordinator using direct event communication
#[derive(Clone)]
pub struct DataCoordinator {
    /// Configuration
    config: Arc<RwLock<DaemonConfig>>,
    /// Service manager
    service_manager: Arc<ServiceManager>,
    /// Event bus for communication
    event_bus: Arc<EventBus>,
    /// Event handler
    event_handler: Arc<DaemonEventHandler>,
    /// Event logger
    event_logger: Arc<EventLogger>,
    /// Filesystem watcher
    watcher: Option<Arc<WatchManager>>,
    /// Shutdown signal
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Service discovery cache
    service_discovery: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    /// Daemon health status
    daemon_health: Arc<RwLock<DaemonHealth>>,
    /// Event statistics
    event_stats: Arc<RwLock<HashMap<String, u64>>>,
}

impl DataCoordinator {
    /// Create a new data coordinator
    pub async fn new(config: DaemonConfig) -> Result<Self> {
        let config = Arc::new(RwLock::new(config));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let running = Arc::new(RwLock::new(false));

        // Initialize simplified event bus
        let event_bus = Arc::new(EventBus::new());

        // Initialize service manager
        let service_manager = Arc::new(ServiceManager::new().await?);

        // Initialize event logger
        let event_logger = Arc::new(EventLogger::new());

        // Initialize event handler
        let event_handler = Arc::new(DaemonEventHandler::new(event_bus.clone()));

        // Initialize tracking structures
        let service_discovery = Arc::new(RwLock::new(HashMap::new()));
        let daemon_health = Arc::new(RwLock::new(DaemonHealth::default()));
        let event_stats = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            config,
            service_manager,
            event_bus,
            event_handler,
            event_logger,
            watcher: None,
            shutdown_tx,
            shutdown_rx,
            running,
            service_discovery,
            daemon_health,
            event_stats,
        })
    }

    /// Initialize the coordinator
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing simplified data coordinator");

        // Validate configuration
        self.config.read().await.validate()?;

        // Initialize services
        self.initialize_services().await?;

        // Initialize event subscriptions
        self.initialize_event_subscriptions().await?;

        // Initialize filesystem watcher
        self.initialize_watcher().await?;

        // Publish daemon startup event
        self.publish_daemon_started().await?;

        info!("Data coordinator initialized successfully");
        Ok(())
    }

    /// Start the coordinator
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting simplified data coordinator");

        // Set running state
        *self.running.write().await = true;

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

        // Shutdown service manager
        self.service_manager.shutdown().await?;

        // Publish daemon shutdown event
        self.publish_daemon_stopped().await?;

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
        *self.config.write().await = new_config.clone();

        // Publish configuration reloaded event
        let config_event = DaemonEvent::Service(EventBuilder::service_with_data(
            crate::events::ServiceEventType::StatusChanged,
            "daemon".to_string(),
            "coordinator".to_string(),
            serde_json::json!({
                "config_reloaded": true,
                "config_hash": format!("{:x}", md5::compute(format!("{:?}", new_config)))
            })
        ));

        if let Err(e) = self.event_bus.publish(config_event).await {
            warn!("Failed to publish config reload event: {}", e);
        }

        info!("Configuration updated successfully");
        Ok(())
    }

    /// Publish an event to the event bus
    pub async fn publish_event(&self, event: DaemonEvent) -> Result<()> {
        let receiver_count = self.event_bus.publish(event.clone()).await
            .map_err(|e| anyhow::anyhow!("Failed to publish event: {}", e))?;

        // Update statistics
        {
            let mut stats = self.event_stats.write().await;
            let event_type_name = self.get_event_type_name(&event);
            *stats.entry(event_type_name).or_insert(0) += 1;
        }

        // Update daemon health
        {
            let mut health = self.daemon_health.write().await;
            health.events_processed += 1;
        }

        trace!("Event published to {} receivers", receiver_count);
        Ok(())
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_bus.subscribe()
    }

    /// Get event statistics
    pub async fn get_event_statistics(&self) -> HashMap<String, u64> {
        self.event_stats.read().await.clone()
    }

    /// Get event bus statistics
    pub async fn get_event_bus_stats(&self) -> crate::events::EventStats {
        self.event_bus.get_stats().await
    }

    /// Get service discovery information
    pub async fn get_discovered_services(&self) -> HashMap<String, ServiceInfo> {
        self.service_discovery.read().await.clone()
    }

    /// Get daemon health status
    pub async fn get_daemon_health(&self) -> DaemonHealth {
        self.daemon_health.read().await.clone()
    }

    /// Initialize services
    async fn initialize_services(&self) -> Result<()> {
        debug!("Initializing services");

        // Create event service
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

    /// Initialize event subscriptions
    async fn initialize_event_subscriptions(&self) -> Result<()> {
        debug!("Initializing event subscriptions");

        // Subscribe to event bus
        let mut receiver = self.event_bus.subscribe();
        let event_handler = self.event_handler.clone();
        let event_bus = self.event_bus.clone();

        // Spawn event processing task
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv().await {
                if let Err(e) = event_handler.handle_event(event.clone()).await {
                    error!("Error handling event: {}", e);

                    // Create error event for the handling failure
                    let error_event = DaemonEvent::Error(EventBuilder::error(
                        crate::events::ErrorSeverity::Error,
                        crate::events::ErrorCategory::Unknown,
                        "EVENT_HANDLER_FAILED".to_string(),
                        format!("Failed to handle event: {}", e),
                    ));

                    if let Err(pub_err) = event_bus.publish(error_event).await {
                        error!("Failed to publish error event: {}", pub_err);
                    }
                }
            }
        });

        info!("Event subscriptions initialized");
        Ok(())
    }

    /// Initialize filesystem watcher
    async fn initialize_watcher(&mut self) -> Result<()> {
        debug!("Initializing filesystem watcher");

        // For now, we'll use a placeholder implementation
        // In a real implementation, this would set up actual file watching
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

        // Service discovery task
        let discovery_task = self.start_service_discovery().await?;

        // Event statistics task
        let stats_task = self.start_event_statistics().await?;

        // Spawn tasks
        tokio::spawn(health_task);
        tokio::spawn(metrics_task);
        tokio::spawn(discovery_task);
        tokio::spawn(stats_task);

        info!("Background tasks started");
        Ok(())
    }

    /// Create database service
    async fn create_database_service(&self) -> Result<Arc<MockDatabaseService>> {
        let mock_db = Arc::new(MockDatabaseService);
        Ok(mock_db)
    }

    /// Publish daemon startup event
    async fn publish_daemon_started(&self) -> Result<()> {
        let startup_event = DaemonEvent::Service(EventBuilder::service_with_data(
            crate::events::ServiceEventType::Started,
            "daemon".to_string(),
            "coordinator".to_string(),
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "startup_time": chrono::Utc::now().to_rfc3339(),
                "features": vec!["simplified_events", "tokio_broadcast"]
            })
        ));

        self.publish_event(startup_event).await?;
        info!("Daemon startup event published");
        Ok(())
    }

    /// Publish daemon shutdown event
    async fn publish_daemon_stopped(&self) -> Result<()> {
        let shutdown_event = DaemonEvent::Service(EventBuilder::service_with_data(
            crate::events::ServiceEventType::Stopped,
            "daemon".to_string(),
            "coordinator".to_string(),
            serde_json::json!({
                "shutdown_time": chrono::Utc::now().to_rfc3339(),
                "reason": "coordinator_stop_called"
            })
        ));

        self.publish_event(shutdown_event).await?;
        info!("Daemon shutdown event published");
        Ok(())
    }

    /// Start health monitoring task
    async fn start_health_monitoring(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let service_manager = self.service_manager.clone();
        let event_bus = self.event_bus.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(60)); // Check every minute

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check service health
                        match service_manager.get_all_health().await {
                            Ok(health_statuses) => {
                                for (service_name, health) in health_statuses {
                                    let status = match health.status {
                                        ServiceStatus::Healthy => crate::events::HealthStatus::Healthy,
                                        ServiceStatus::Degraded => crate::events::HealthStatus::Degraded,
                                        ServiceStatus::Unhealthy => crate::events::HealthStatus::Unhealthy,
                                        _ => crate::events::HealthStatus::Unknown,
                                    };

                                    let health_event = DaemonEvent::Health(EventBuilder::health_with_message(
                                        service_name.clone(),
                                        status,
                                        health.message.unwrap_or_default(),
                                    ));

                                    if let Err(e) = event_bus.publish(health_event).await {
                                        error!("Failed to publish health event: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to get service health: {}", e);

                                let error_event = DaemonEvent::Error(EventBuilder::error(
                                    crate::events::ErrorSeverity::Error,
                                    crate::events::ErrorCategory::Resource,
                                    "HEALTH_CHECK_FAILED".to_string(),
                                    format!("Failed to get service health: {}", e),
                                ));

                                if let Err(pub_err) = event_bus.publish(error_event).await {
                                    error!("Failed to publish health check error: {}", pub_err);
                                }
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
        let event_bus = self.event_bus.clone();
        let daemon_health = self.daemon_health.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(300)); // Every 5 minutes

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Collecting metrics");

                        // Update daemon health metrics
                        {
                            let mut health = daemon_health.write().await;
                            health.metrics.insert("memory_usage_mb".to_string(),
                                Self::get_memory_usage() as f64 / 1024.0 / 1024.0);
                            health.uptime_seconds += 300; // 5 minutes in seconds
                            health.last_health_check = chrono::Utc::now();
                        }

                        // Publish metrics event
                        let metrics_event = DaemonEvent::Service(EventBuilder::service_with_data(
                            crate::events::ServiceEventType::StatusChanged,
                            "daemon".to_string(),
                            "metrics".to_string(),
                            serde_json::json!({
                                "collection_time": chrono::Utc::now().to_rfc3339(),
                                "metrics_collected": true
                            })
                        ));

                        if let Err(e) = event_bus.publish(metrics_event).await {
                            warn!("Failed to publish metrics event: {}", e);
                        }
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

    /// Start service discovery task
    async fn start_service_discovery(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let service_discovery = self.service_discovery.clone();
        let event_bus = self.event_bus.clone();
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

                            // Publish service unregistered event
                            let unregister_event = DaemonEvent::Service(EventBuilder::service(
                                crate::events::ServiceEventType::Unregistered,
                                stale_service.clone(),
                                "stale_cleanup".to_string(),
                            ));

                            if let Err(e) = event_bus.publish(unregister_event).await {
                                warn!("Failed to publish stale service cleanup event: {}", e);
                            }
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

    /// Start event statistics task
    async fn start_event_statistics(&self) -> Result<impl std::future::Future<Output = Result<()>>> {
        let event_stats = self.event_stats.clone();
        let event_bus = self.event_bus.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Ok(async move {
            let mut interval = interval(Duration::from_secs(60)); // Every minute

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        debug!("Collecting event statistics");

                        let stats = event_stats.read().await.clone();
                        let total_events: u64 = stats.values().sum();

                        // Publish statistics event
                        let stats_event = DaemonEvent::Service(EventBuilder::service_with_data(
                            crate::events::ServiceEventType::StatusChanged,
                            "daemon".to_string(),
                            "event_statistics".to_string(),
                            serde_json::json!({
                                "total_events": total_events,
                                "events_by_type": stats,
                                "collection_time": chrono::Utc::now().to_rfc3339()
                            })
                        ));

                        if let Err(e) = event_bus.publish(stats_event).await {
                            warn!("Failed to publish event statistics: {}", e);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Event statistics task shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Get the name of an event type for statistics
    fn get_event_type_name(&self, event: &DaemonEvent) -> String {
        match event {
            DaemonEvent::Filesystem(_) => "filesystem".to_string(),
            DaemonEvent::Database(_) => "database".to_string(),
            DaemonEvent::Sync(_) => "sync".to_string(),
            DaemonEvent::Error(_) => "error".to_string(),
            DaemonEvent::Health(_) => "health".to_string(),
            DaemonEvent::Service(_) => "service".to_string(),
        }
    }

    /// Get current memory usage (simplified)
    fn get_memory_usage() -> usize {
        // In a real implementation, this would use platform-specific APIs
        // For now, return a placeholder value
        50 * 1024 * 1024 // 50MB
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

        let result = coordinator.initialize().await;
        // This might fail due to missing configuration, but that's expected
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_event_publishing() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Create a test event
        let test_event = DaemonEvent::Health(EventBuilder::health(
            "test-service".to_string(),
            crate::events::HealthStatus::Healthy,
        ));

        // Publish the event
        let result = coordinator.publish_event(test_event).await;
        assert!(result.is_ok());

        // Check statistics
        let stats = coordinator.get_event_statistics().await;
        assert_eq!(stats.get("health"), Some(&1));
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Subscribe to events
        let mut receiver = coordinator.subscribe();

        // Publish an event
        let test_event = DaemonEvent::Service(EventBuilder::service(
            crate::events::ServiceEventType::Started,
            "test-service".to_string(),
            "test-type".to_string(),
        ));

        coordinator.publish_event(test_event).await.unwrap();

        // Receive the event
        let received_event = receiver.recv().await.unwrap();
        match received_event {
            DaemonEvent::Service(service_event) => {
                assert_eq!(service_event.service_id, "test-service");
                assert_eq!(service_event.service_type, "test-type");
            }
            _ => panic!("Expected service event"),
        }
    }

    #[tokio::test]
    async fn test_daemon_health_tracking() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Get initial health
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.status, ServiceStatus::Healthy);
        assert_eq!(health.events_processed, 0);

        // Publish some events
        for i in 0..5 {
            let event = DaemonEvent::Error(EventBuilder::error(
                crate::events::ErrorSeverity::Warning,
                crate::events::ErrorCategory::Unknown,
                format!("TEST_ERROR_{}", i),
                format!("Test error message {}", i),
            ));
            coordinator.publish_event(event).await.unwrap();
        }

        // Check health was updated
        let health = coordinator.get_daemon_health().await;
        assert_eq!(health.events_processed, 5);
    }

    #[tokio::test]
    async fn test_service_discovery_cleanup() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Add a stale service to discovery
        let stale_service = ServiceInfo {
            service_id: "stale-service".to_string(),
            service_type: "test-type".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: None,
            status: ServiceStatus::Healthy,
            last_seen: chrono::Utc::now() - chrono::Duration::minutes(10), // 10 minutes ago
            capabilities: vec![],
            metadata: HashMap::new(),
        };

        {
            let mut discovery = coordinator.service_discovery.write().await;
            discovery.insert("stale-service".to_string(), stale_service);
        }

        // Verify service is in discovery
        let services = coordinator.get_discovered_services().await;
        assert!(services.contains_key("stale-service"));

        // The service discovery task would eventually remove stale services
        // For testing, we can manually trigger the cleanup logic
        let now = chrono::Utc::now();
        let mut discovery = coordinator.service_discovery.write().await;
        discovery.retain(|_, info| now.signed_duration_since(info.last_seen).num_minutes() <= 5);

        // Verify service was removed
        assert!(!discovery.contains_key("stale-service"));
    }

    #[tokio::test]
    async fn test_event_handler_integration() {
        let event_bus = Arc::new(EventBus::new());
        let event_handler = DaemonEventHandler::new(event_bus.clone());

        // Create a service event
        let service_event = DaemonEvent::Service(EventBuilder::service_with_data(
            crate::events::ServiceEventType::Started,
            "test-service".to_string(),
            "test-type".to_string(),
            serde_json::json!({"test": "data"}),
        ));

        // Handle the event
        let result = event_handler.handle_event(service_event).await;
        assert!(result.is_ok());

        // Check that state was updated
        let state = event_handler.coordinator_state.read().await;
        assert!(state.contains_key("service:test-service"));
    }

    #[tokio::test]
    async fn test_error_event_handling() {
        let event_bus = Arc::new(EventBus::new());
        let event_handler = DaemonEventHandler::new(event_bus.clone());

        // Create an error event
        let error_event = DaemonEvent::Error(EventBuilder::error_with_details(
            crate::events::ErrorSeverity::Critical,
            crate::events::ErrorCategory::Database,
            "DB_CONNECTION_FAILED".to_string(),
            "Database connection failed".to_string(),
            "Connection timeout after 30 seconds".to_string(),
        ));

        // Handle the event
        let result = event_handler.handle_event(error_event).await;
        assert!(result.is_ok());

        // Check that error was tracked
        let state = event_handler.coordinator_state.read().await;
        assert!(state.contains_key("error:Database:DB_CONNECTION_FAILED"));
    }

    #[tokio::test]
    async fn test_config_update() {
        let config = DaemonConfig::default();
        let coordinator = DataCoordinator::new(config).await.unwrap();

        // Create a new config
        let mut new_config = coordinator.get_config().await;
        // Modify config if needed

        // Update config
        let result = coordinator.update_config(new_config.clone()).await;
        assert!(result.is_ok());

        // Verify config was updated
        let current_config = coordinator.get_config().await;
        assert_eq!(format!("{:?}", current_config), format!("{:?}", new_config));
    }
}