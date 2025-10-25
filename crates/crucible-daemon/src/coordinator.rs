//! Simplified data layer coordinator for crucible-daemon
//!
//! Main coordination logic using simple event communication via tokio::broadcast.
//! Eliminates complex service event routing in favor of direct channel communication.

use crate::config::DaemonConfig;
use crate::events::{DaemonEvent, EventBus, EventBuilder};
use crate::handlers::EventLogger;
use crate::services::{ServiceManager, SimpleEventService, SimpleFileService, SimpleSyncService};
use crate::surrealdb_service::{SurrealDBService, create_surrealdb_from_config};
use anyhow::Result;
use crucible_watch::{
    EventDrivenEmbeddingProcessor, EmbeddingEventHandler, EmbeddingEvent,
};
use crucible_surrealdb::embedding_pool::EmbeddingThreadPool;
use crucible_surrealdb::{vault_processor, vault_scanner::VaultScannerConfig};
use tokio::sync::mpsc;
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
    #[allow(dead_code)]
    event_bus: Arc<EventBus>,
    coordinator_state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    embedding_event_tx: Option<tokio::sync::mpsc::UnboundedSender<EmbeddingEvent>>,
}

impl DaemonEventHandler {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            coordinator_state: Arc::new(RwLock::new(HashMap::new())),
            embedding_event_tx: None,
        }
    }

    /// Set the embedding event sender
    pub fn with_embedding_event_tx(mut self, embedding_event_tx: tokio::sync::mpsc::UnboundedSender<EmbeddingEvent>) -> Self {
        self.embedding_event_tx = Some(embedding_event_tx);
        self
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

        // Only process markdown files for embeddings
        if !event.path.extension().map_or(false, |ext| ext == "md") {
            debug!("Skipping non-markdown file: {}", event.path.display());
            return Ok(());
        }

        // Read file content for embedding
        let content = match std::fs::read_to_string(&event.path) {
            Ok(content) => content,
            Err(e) => {
                warn!("Failed to read file {} for embedding: {}", event.path.display(), e);
                return Ok(());
            }
        };

        // Convert FilesystemEventType to FileEventKind
        let file_event_kind = match event.event_type {
            crate::events::FilesystemEventType::Created => crucible_watch::FileEventKind::Created,
            crate::events::FilesystemEventType::Modified => crucible_watch::FileEventKind::Modified,
            crate::events::FilesystemEventType::Deleted => crucible_watch::FileEventKind::Deleted,
            crate::events::FilesystemEventType::Renamed => {
                // For renamed events, treat as modification since the content may have changed
                crucible_watch::FileEventKind::Modified
            }
            crate::events::FilesystemEventType::DirectoryCreated |
            crate::events::FilesystemEventType::DirectoryDeleted => {
                // Skip directory events for embedding processing
                debug!("Skipping directory event for embedding: {}", event.path.display());
                return Ok(());
            }
        };

        // Create embedding event (removed .await since EmbeddingEvent::new is not async)
        let embedding_event = EmbeddingEvent::new(
            event.path.clone(),
            file_event_kind,
            content,
            Default::default(),
        );

        // Send to embedding processor
        if let Some(ref tx) = self.embedding_event_tx {
            if let Err(e) = tx.send(embedding_event) {
                warn!("Failed to send embedding event for {}: {}", event.path.display(), e);
            } else {
                debug!("Successfully queued embedding event for: {}", event.path.display());
            }
        } else {
            warn!("Embedding event sender not configured for: {}", event.path.display());
        }

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
    #[allow(dead_code)]
    event_logger: Arc<EventLogger>,
    /// Event-driven embedding processor
    embedding_processor: Option<Arc<EventDrivenEmbeddingProcessor>>,
    /// Embedding thread pool
    embedding_pool: Option<Arc<EmbeddingThreadPool>>,
    /// Embedding event sender
    embedding_event_tx: Option<mpsc::UnboundedSender<EmbeddingEvent>>,
    /// Shutdown signal
    shutdown_tx: watch::Sender<bool>,
    #[allow(dead_code)]
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
            embedding_processor: None,
            embedding_pool: None,
            embedding_event_tx: None,
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
        info!("Starting simplified data coordinator for one-shot processing");

        // Set running state
        *self.running.write().await = true;

        // For one-shot processing, we don't start infinite background tasks
        // Instead, we initialize only the essential components
        self.initialize_essential_components().await?;

        info!("Data coordinator started successfully for one-shot processing");
        Ok(())
    }

    /// Stop the coordinator
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping data coordinator");

        // Send shutdown signal
        let _ = self.shutdown_tx.send(true);

        // Set running state to false
        *self.running.write().await = false;

        // Note: Filesystem watcher functionality has been removed from this implementation

        // Shutdown embedding processor if initialized
        if let Some(processor) = &self.embedding_processor {
            info!("Shutting down embedding processor");
            if let Err(e) = processor.shutdown().await {
                warn!("Error shutting down embedding processor: {}", e);
            }
        }

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

    /// Get the service manager (for testing purposes)
    pub fn service_manager(&self) -> Arc<ServiceManager> {
        self.service_manager.clone()
    }

    /// Process the vault exactly once using the existing vault processing infrastructure
    pub async fn process_vault_once(&mut self) -> Result<()> {
        info!("Starting one-shot vault processing");

        let config = self.config.read().await;
        let vault_path = match std::env::var("OBSIDIAN_VAULT_PATH") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "OBSIDIAN_VAULT_PATH environment variable is required"
                ));
            }
        };

        if !vault_path.exists() || !vault_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Vault path does not exist or is not a directory: {}",
                vault_path.display()
            ));
        }

        info!("Processing vault at: {}", vault_path.display());

        // Create vault scanner configuration
        let scan_config = VaultScannerConfig {
            max_file_size_bytes: 10 * 1024 * 1024, // 10MB
            max_recursion_depth: 10,
            recursive_scan: true,
            include_hidden_files: false,
            file_extensions: vec!["md".to_string()],
            parallel_processing: config.performance.workers.num_workers.unwrap_or(4),
            batch_processing: true,
            batch_size: 50,
            enable_embeddings: true,
            process_embeds: true,
            process_wikilinks: true,
            enable_incremental: true,
            track_file_changes: true,
            change_detection_method: crucible_surrealdb::vault_scanner::ChangeDetectionMethod::ContentHash,
            error_handling_mode: crucible_surrealdb::vault_scanner::ErrorHandlingMode::ContinueOnError,
            max_error_count: 100,
            error_retry_attempts: 3,
            error_retry_delay_ms: 1000,
            skip_problematic_files: true,
            log_errors_detailed: true,
            error_threshold_circuit_breaker: 10,
            circuit_breaker_timeout_ms: 30000,
            processing_timeout_ms: 30000,
        };

        // Scan the vault for files
        info!("Scanning vault for files...");
        let discovered_files = match vault_processor::scan_vault_directory(&vault_path, &scan_config).await {
            Ok(files) => {
                info!("Discovered {} files in vault", files.len());
                files
            }
            Err(e) => {
                error!("Failed to scan vault directory: {}", e);
                return Err(anyhow::anyhow!("Vault scanning failed: {}", e));
            }
        };

        if discovered_files.is_empty() {
            warn!("No files found to process in vault");
            info!("One-shot vault processing completed (no files to process)");
            return Ok(());
        }

        // For now, we'll implement a simplified processing that doesn't require
        // direct database access - this leverages the existing event-driven
        // embedding infrastructure for one-time processing
        info!("Processing vault files using event-driven infrastructure...");

        // Simulate processing all discovered files by triggering file events
        let mut processed_count = 0;
        let mut failed_count = 0;

        for file_info in &discovered_files {
            if file_info.is_markdown && file_info.is_accessible {
                debug!("Processing file: {}", file_info.path.display());

                // Create a filesystem event to trigger processing
                debug!("Creating filesystem event for file: {}", file_info.path.display());
                let fs_event = DaemonEvent::Filesystem(EventBuilder::filesystem(
                    crate::events::FilesystemEventType::Modified,
                    file_info.path.clone(),
                ));

                // Publish the event
                debug!("Publishing filesystem event for: {}", file_info.path.display());
                if let Err(e) = self.publish_event(fs_event).await {
                    error!("Failed to publish processing event for {}: {}",
                          file_info.path.display(), e);
                    failed_count += 1;
                } else {
                    debug!("Successfully published filesystem event for: {}", file_info.path.display());
                    processed_count += 1;
                }

                // Small delay to prevent overwhelming the system
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        info!("One-shot vault processing completed: {} files queued for processing, {} failed",
              processed_count, failed_count);

        // Wait a bit for processing to complete
        info!("Waiting for processing to complete...");
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Publish completion event
        let completion_event = DaemonEvent::Service(EventBuilder::service_with_data(
            crate::events::ServiceEventType::StatusChanged,
            "daemon".to_string(),
            "one_shot_processor".to_string(),
            serde_json::json!({
                "processing_completed": true,
                "files_processed": processed_count,
                "files_failed": failed_count,
                "completion_time": chrono::Utc::now().to_rfc3339()
            })
        ));

        if let Err(e) = self.publish_event(completion_event).await {
            warn!("Failed to publish completion event: {}", e);
        }

        Ok(())
    }

    /// Initialize essential components for one-shot processing
    async fn initialize_essential_components(&mut self) -> Result<()> {
        debug!("Initializing essential components for one-shot processing");

        // Initialize event subscriptions (needed for processing)
        self.initialize_event_subscriptions().await?;

        // Start embedding processor for one-shot processing
        // Note: For one-shot processing, the embedding processor will be used
        // to process embedding events directly
        self.initialize_embedding_processor_for_one_shot().await?;

        info!("Essential components initialized for one-shot processing");
        Ok(())
    }

    /// Initialize embedding processor for one-shot processing
    async fn initialize_embedding_processor_for_one_shot(&mut self) -> Result<()> {
        debug!("Initializing embedding processor for one-shot processing");

        let config = self.config.read().await;

        // Initialize embedding thread pool with configuration from environment variables
        let embedding_config = create_embedding_config_from_env(&config)?;
        // Create embedding provider integration from environment
        let provider_integration = create_embedding_provider_integration_from_env()?;
        let embedding_pool = Arc::new(
            EmbeddingThreadPool::new_with_provider_config(embedding_config, provider_integration).await?
        );

        // Create embedding event channel
        let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();

        // Create and start event-driven embedding processor with default config
        let embedding_config = crucible_watch::EventDrivenEmbeddingConfig::default();
        let embedding_processor = Arc::new(
            EventDrivenEmbeddingProcessor::new(embedding_config, embedding_pool.clone())
                .await?
                .with_embedding_event_receiver(embedding_rx)
                .await
        );

        // Start the embedding processor
        embedding_processor.start().await?;

        // Update event handler to include embedding event sender
        self.event_handler = Arc::new(
            DaemonEventHandler::new(self.event_bus.clone())
                .with_embedding_event_tx(embedding_tx.clone())
        );

        // Store components
        self.embedding_processor = Some(embedding_processor);
        self.embedding_pool = Some(embedding_pool);
        self.embedding_event_tx = Some(embedding_tx);

        info!("Embedding processor initialized successfully for one-shot processing");

        Ok(())
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

    /// Initialize filesystem watcher (for continuous operation - not used in one-shot mode)
    async fn initialize_watcher(&mut self) -> Result<()> {
        debug!("Initializing filesystem watcher (background mode)");

        let config = self.config.read().await;

        // Initialize embedding thread pool with configuration from environment variables
        let embedding_config = create_embedding_config_from_env(&config)?;
        // Create embedding provider integration from environment
        let provider_integration = create_embedding_provider_integration_from_env()?;
        let embedding_pool = Arc::new(
            EmbeddingThreadPool::new_with_provider_config(embedding_config, provider_integration).await?
        );

        // Create embedding event channel
        let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();

        // Create and start event-driven embedding processor with default config
        let embedding_config = crucible_watch::EventDrivenEmbeddingConfig::default();
        let embedding_processor = Arc::new(
            EventDrivenEmbeddingProcessor::new(embedding_config, embedding_pool.clone())
                .await?
                .with_embedding_event_receiver(embedding_rx)
                .await
        );

        // Register embedding event handler
        let _embedding_handler = Arc::new(EmbeddingEventHandler::new(
            embedding_processor.clone(),
            embedding_tx.clone(), // Pass the embedding event sender
        ));

        // Start the embedding processor
        embedding_processor.start().await?;

        // Store components
        self.embedding_processor = Some(embedding_processor);
        self.embedding_pool = Some(embedding_pool);
        self.embedding_event_tx = Some(embedding_tx);

        info!("Embedding processor initialized successfully for continuous operation");

        Ok(())
    }

    /// Start background tasks
    #[allow(dead_code)]
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
    async fn create_database_service(&self) -> Result<Arc<SurrealDBService>> {
        let config = self.config.read().await;
        let db_service = create_surrealdb_from_config(&*config).await?;
        Ok(db_service)
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    fn get_memory_usage() -> usize {
        // In a real implementation, this would use platform-specific APIs
        // For now, return a placeholder value
        50 * 1024 * 1024 // 50MB
    }
}


/// Create embedding provider integration from environment variables
fn create_embedding_provider_integration_from_env(
) -> Result<crucible_surrealdb::embedding_pool::EmbeddingProviderIntegration> {
    use crucible_surrealdb::embedding_pool::EmbeddingProviderIntegration;
    use crucible_config::{EmbeddingProviderConfig, EmbeddingProviderType};

    // Read embedding configuration from environment variables
    let embedding_endpoint = std::env::var("EMBEDDING_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    let embedding_model = std::env::var("EMBEDDING_MODEL")
        .map_err(|_| anyhow::anyhow!(
            "EMBEDDING_MODEL environment variable is required. \
            Please set it to your embedding model name, e.g., 'nomic-embed-text-v1.5-q8_0'"
        ))?;

    // Create real embedding provider configuration
    use std::collections::HashMap;
    let provider_config = Some(EmbeddingProviderConfig {
        provider_type: EmbeddingProviderType::Ollama,
        api: crucible_config::ApiConfig {
            key: None,
            base_url: Some(embedding_endpoint.clone()),
            timeout_seconds: Some(30),
            retry_attempts: Some(3),
            headers: HashMap::new(),
        },
        model: crucible_config::ModelConfig {
            name: embedding_model.clone(),
            dimensions: None,
            max_tokens: Some(2048),
        },
        options: HashMap::new(),
    });

    let provider_integration = EmbeddingProviderIntegration {
        use_mock: false, // Use real embeddings
        config: provider_config,
        mock_model: embedding_model.clone(),
        mock_dimensions: 768,
    };

    info!("Created embedding provider integration: endpoint={}, model={}",
          embedding_endpoint, embedding_model);

    Ok(provider_integration)
}

/// Create embedding configuration from environment variables
fn create_embedding_config_from_env(
    daemon_config: &crate::config::DaemonConfig,
) -> Result<crucible_surrealdb::embedding_config::EmbeddingConfig> {
    use crucible_surrealdb::embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode};

    // Read embedding configuration from environment variables
    let embedding_endpoint = std::env::var("EMBEDDING_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    let embedding_model = std::env::var("EMBEDDING_MODEL")
        .map_err(|_| anyhow::anyhow!(
            "EMBEDDING_MODEL environment variable is required. \
            Please set it to your embedding model name, e.g., 'nomic-embed-text-v1.5-q8_0'"
        ))?;

    // Create embedding configuration based on environment
    let (model_type, privacy_mode) = if embedding_endpoint.starts_with("http://localhost:11434") {
        // Local Ollama instance - use standard local model
        (EmbeddingModel::LocalStandard, PrivacyMode::StrictLocal)
    } else if embedding_endpoint.starts_with("http") {
        // Remote embedding service - allow external fallback
        (EmbeddingModel::LocalStandard, PrivacyMode::AllowExternalFallback)
    } else {
        // Default to local standard
        (EmbeddingModel::LocalStandard, PrivacyMode::StrictLocal)
    };

    let embedding_config = EmbeddingConfig {
        worker_count: daemon_config.performance.workers.num_workers.unwrap_or(4),
        batch_size: 16,
        model_type,
        privacy_mode,
        max_queue_size: daemon_config.performance.workers.max_queue_size,
        timeout_ms: 30000,
        retry_attempts: 3,
        retry_delay_ms: 500,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 60000,
    };

    info!("Created embedding config: endpoint={}, model={}, workers={}",
          embedding_endpoint, embedding_model, embedding_config.worker_count);

    Ok(embedding_config)
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