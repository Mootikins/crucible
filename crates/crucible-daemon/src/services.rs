//! Service integration layer for the daemon
//!
//! Provides service abstractions and implementations for data layer operations
//! using the crucible-services framework.

use anyhow::Result;
use async_trait::async_trait;
use crucible_services::{
    DatabaseService, ServiceError, ServiceResult, ServiceInfo, ServiceHealth,
    BaseService, router::{ServiceRouter, ServiceRequest, ServiceResponse},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Service manager for coordinating all data layer services
#[derive(Clone)]
pub struct ServiceManager {
    /// Service router for request routing
    router: Arc<dyn ServiceRouter>,
    /// Service registry
    services: Arc<RwLock<HashMap<String, Arc<dyn BaseService>>>>,
    /// Service health monitor
    health_monitor: Arc<ServiceHealthMonitor>,
}

impl ServiceManager {
    /// Create a new service manager
    pub async fn new(router: Arc<dyn ServiceRouter>) -> Result<Self> {
        let health_monitor = Arc::new(ServiceHealthMonitor::new());

        Ok(Self {
            router,
            services: Arc::new(RwLock::new(HashMap::new())),
            health_monitor,
        })
    }

    /// Register a new service
    pub async fn register_service<T>(&self, service: Arc<T>) -> Result<()>
    where
        T: BaseService + Send + Sync + 'static,
    {
        let service_info = service.service_info().await?;
        let service_name = service_info.name.clone();

        {
            let mut services = self.services.write().await;
            services.insert(service_name.clone(), service.clone() as Arc<dyn BaseService>);
        }

        // Start health monitoring for the service
        self.health_monitor.start_monitoring(service.clone()).await?;

        tracing::info!("Registered service: {}", service_name);
        Ok(())
    }

    /// Get a service by name
    pub async fn get_service(&self, name: &str) -> Result<Option<Arc<dyn BaseService>>> {
        let services = self.services.read().await;
        Ok(services.get(name).cloned())
    }

    /// Route a request to the appropriate service
    pub async fn route_request(&self, request: ServiceRequest) -> Result<ServiceResponse, ServiceError> {
        self.router.route_request(request).await
    }

    /// Get health status of all services
    pub async fn get_all_health(&self) -> Result<HashMap<String, ServiceHealth>> {
        self.health_monitor.get_all_health().await
    }

    /// Get health status of a specific service
    pub async fn get_service_health(&self, service_name: &str) -> Result<Option<ServiceHealth>> {
        self.health_monitor.get_service_health(service_name).await
    }

    /// Shutdown all services
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down service manager");

        // Stop health monitoring
        self.health_monitor.shutdown().await?;

        // Shutdown all services
        let services = self.services.read().await;
        for (name, service) in services.iter() {
            if let Err(e) = service.shutdown().await {
                tracing::warn!("Failed to shutdown service {}: {}", name, e);
            }
        }

        tracing::info!("Service manager shutdown complete");
        Ok(())
    }
}

/// File service for filesystem operations
#[derive(Clone)]
pub struct FileService {
    service_info: ServiceInfo,
    file_handlers: Arc<RwLock<HashMap<String, Arc<dyn FileHandler>>>>,
}

impl FileService {
    /// Create a new file service
    pub fn new() -> Self {
        Self {
            service_info: ServiceInfo {
                service_id: Uuid::new_v4(),
                name: "file_service".to_string(),
                version: "1.0.0".to_string(),
                description: "Filesystem operations service".to_string(),
                dependencies: vec![],
                metadata: HashMap::new(),
            },
            file_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a file handler
    pub async fn register_handler(&self, file_type: String, handler: Arc<dyn FileHandler>) {
        let mut handlers = self.file_handlers.write().await;
        handlers.insert(file_type, handler);
    }

    /// Process a file
    pub async fn process_file(&self, file_path: &std::path::Path) -> Result<FileProcessingResult> {
        let file_type = self.detect_file_type(file_path).await?;

        let handlers = self.file_handlers.read().await;
        if let Some(handler) = handlers.get(&file_type) {
            handler.process_file(file_path).await
        } else {
            Ok(FileProcessingResult {
                success: false,
                error: Some(format!("No handler found for file type: {}", file_type)),
                metadata: HashMap::new(),
            })
        }
    }

    /// Detect file type from path
    async fn detect_file_type(&self, file_path: &std::path::Path) -> Result<String> {
        if let Some(extension) = file_path.extension() {
            Ok(extension.to_string_lossy().to_lowercase())
        } else {
            Ok("unknown".to_string())
        }
    }
}

#[async_trait]
impl BaseService for FileService {
    async fn service_info(&self) -> ServiceResult<ServiceInfo> {
        Ok(self.service_info.clone())
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: crucible_services::types::HealthStatus::Healthy,
            message: Some("File service is healthy".to_string()),
            last_check: chrono::Utc::now(),
            metrics: HashMap::new(),
        })
    }

    async fn shutdown(&self) -> ServiceResult<()> {
        tracing::info!("Shutting down file service");
        Ok(())
    }
}

/// File processing result
#[derive(Debug, Clone)]
pub struct FileProcessingResult {
    pub success: bool,
    pub error: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// File handler trait
#[async_trait]
pub trait FileHandler: Send + Sync {
    /// Process a file
    async fn process_file(&self, file_path: &std::path::Path) -> Result<FileProcessingResult>;
}

/// Database service wrapper around crucible-services DatabaseService
#[derive(Clone)]
pub struct DataLayerDatabaseService {
    inner: Arc<dyn DatabaseService>,
    service_info: ServiceInfo,
}

impl DataLayerDatabaseService {
    /// Create a new data layer database service
    pub fn new(database_service: Arc<dyn DatabaseService>) -> Self {
        let service_info = ServiceInfo {
            service_id: Uuid::new_v4(),
            name: "database_service".to_string(),
            version: "1.0.0".to_string(),
            description: "Database operations service".to_string(),
            dependencies: vec![],
            metadata: HashMap::new(),
        };

        Self {
            inner: database_service,
            service_info,
        }
    }

    /// Sync file changes to database
    pub async fn sync_file_change(&self, file_change: FileChange) -> Result<()> {
        // Convert file change to database operations
        match file_change.change_type {
            FileChangeType::Created => {
                // Insert new file record
                self.insert_file_record(file_change).await?;
            }
            FileChangeType::Modified => {
                // Update existing file record
                self.update_file_record(file_change).await?;
            }
            FileChangeType::Deleted => {
                // Delete file record
                self.delete_file_record(file_change).await?;
            }
        }
        Ok(())
    }

    async fn insert_file_record(&self, file_change: FileChange) -> Result<()> {
        let document = serde_json::json!({
            "path": file_change.path.to_string_lossy(),
            "size": file_change.metadata.size,
            "modified": file_change.metadata.modified_time,
            "created": file_change.metadata.created_time,
            "checksum": file_change.metadata.checksum,
            "mime_type": file_change.metadata.mime_type,
            "synced_at": chrono::Utc::now(),
        });

        // This would use the actual database service to insert the document
        // For now, we'll just log it
        tracing::debug!("Inserting file record: {}", serde_json::to_string_pretty(&document)?);
        Ok(())
    }

    async fn update_file_record(&self, file_change: FileChange) -> Result<()> {
        let updates = serde_json::json!({
            "size": file_change.metadata.size,
            "modified": file_change.metadata.modified_time,
            "checksum": file_change.metadata.checksum,
            "synced_at": chrono::Utc::now(),
        });

        tracing::debug!("Updating file record {}: {}",
            file_change.path.to_string_lossy(),
            serde_json::to_string_pretty(&updates)?
        );
        Ok(())
    }

    async fn delete_file_record(&self, file_change: FileChange) -> Result<()> {
        tracing::debug!("Deleting file record: {}", file_change.path.to_string_lossy());
        Ok(())
    }
}

#[async_trait]
impl BaseService for DataLayerDatabaseService {
    async fn service_info(&self) -> ServiceResult<ServiceInfo> {
        Ok(self.service_info.clone())
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        self.inner.health_check().await
    }

    async fn shutdown(&self) -> ServiceResult<()> {
        self.inner.shutdown().await
    }
}

/// File change information
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: std::path::PathBuf,
    pub change_type: FileChangeType,
    pub metadata: FileMetadata,
}

/// File change types
#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: std::path::PathBuf, to: std::path::PathBuf },
}

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: Option<u64>,
    pub modified_time: Option<chrono::DateTime<chrono::Utc>>,
    pub created_time: Option<chrono::DateTime<chrono::Utc>>,
    pub checksum: Option<String>,
    pub mime_type: Option<String>,
}

/// Event service for publishing daemon events
#[derive(Clone)]
pub struct EventService {
    service_info: ServiceInfo,
    publisher: Arc<dyn crate::events::EventPublisher>,
}

impl EventService {
    /// Create a new event service
    pub fn new(publisher: Arc<dyn crate::events::EventPublisher>) -> Self {
        Self {
            service_info: ServiceInfo {
                service_id: Uuid::new_v4(),
                name: "event_service".to_string(),
                version: "1.0.0".to_string(),
                description: "Event publishing service".to_string(),
                dependencies: vec![],
                metadata: HashMap::new(),
            },
            publisher,
        }
    }

    /// Publish a daemon event
    pub async fn publish_event(&self, event: crate::events::DaemonEvent) -> Result<()> {
        self.publisher.publish(event).await.map_err(|e| {
            anyhow::anyhow!("Failed to publish event: {}", e)
        })
    }
}

#[async_trait]
impl BaseService for EventService {
    async fn service_info(&self) -> ServiceResult<ServiceInfo> {
        Ok(self.service_info.clone())
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: crucible_services::types::HealthStatus::Healthy,
            message: Some("Event service is healthy".to_string()),
            last_check: chrono::Utc::now(),
            metrics: HashMap::new(),
        })
    }

    async fn shutdown(&self) -> ServiceResult<()> {
        tracing::info!("Shutting down event service");
        Ok(())
    }
}

/// Sync service for coordinating synchronization operations
#[derive(Clone)]
pub struct SyncService {
    service_info: ServiceInfo,
    sync_strategies: Arc<RwLock<HashMap<String, Arc<dyn SyncStrategy>>>>,
}

impl SyncService {
    /// Create a new sync service
    pub fn new() -> Self {
        Self {
            service_info: ServiceInfo {
                service_id: Uuid::new_v4(),
                name: "sync_service".to_string(),
                version: "1.0.0".to_string(),
                description: "Synchronization coordination service".to_string(),
                dependencies: vec![],
                metadata: HashMap::new(),
            },
            sync_strategies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a sync strategy
    pub async fn register_strategy(&self, name: String, strategy: Arc<dyn SyncStrategy>) {
        let mut strategies = self.sync_strategies.write().await;
        strategies.insert(name, strategy);
    }

    /// Execute a sync operation
    pub async fn execute_sync(&self, request: SyncRequest) -> Result<SyncResult> {
        let strategies = self.sync_strategies.read().await;
        if let Some(strategy) = strategies.get(&request.strategy_name) {
            strategy.execute(request).await
        } else {
            Err(anyhow::anyhow!("Unknown sync strategy: {}", request.strategy_name))
        }
    }
}

#[async_trait]
impl BaseService for SyncService {
    async fn service_info(&self) -> ServiceResult<ServiceInfo> {
        Ok(self.service_info.clone())
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: crucible_services::types::HealthStatus::Healthy,
            message: Some("Sync service is healthy".to_string()),
            last_check: chrono::Utc::now(),
            metrics: HashMap::new(),
        })
    }

    async fn shutdown(&self) -> ServiceResult<()> {
        tracing::info!("Shutting down sync service");
        Ok(())
    }
}

/// Sync request
#[derive(Debug, Clone)]
pub struct SyncRequest {
    pub strategy_name: String,
    pub source: String,
    pub target: String,
    pub options: HashMap<String, serde_json::Value>,
}

/// Sync result
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub success: bool,
    pub items_processed: u64,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// Sync strategy trait
#[async_trait]
pub trait SyncStrategy: Send + Sync {
    /// Execute the sync strategy
    async fn execute(&self, request: SyncRequest) -> Result<SyncResult>;
}

/// Service health monitor
pub struct ServiceHealthMonitor {
    monitored_services: Arc<RwLock<HashMap<String, Arc<dyn BaseService>>>>,
    health_statuses: Arc<RwLock<HashMap<String, ServiceHealth>>>,
    shutdown_tx: Option<flume::Sender<()>>,
}

impl ServiceHealthMonitor {
    /// Create a new health monitor
    pub fn new() -> Self {
        Self {
            monitored_services: Arc::new(RwLock::new(HashMap::new())),
            health_statuses: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
        }
    }

    /// Start monitoring a service
    pub async fn start_monitoring<T>(&self, service: Arc<T>) -> Result<()>
    where
        T: BaseService + Send + Sync + 'static,
    {
        let service_info = service.service_info().await?;
        let service_name = service_info.name.clone();

        {
            let mut services = self.monitored_services.write().await;
            services.insert(service_name.clone(), service.clone() as Arc<dyn BaseService>);
        }

        // Perform initial health check
        let health = service.health_check().await?;
        {
            let mut statuses = self.health_statuses.write().await;
            statuses.insert(service_name.clone(), health);
        }

        tracing::debug!("Started health monitoring for service: {}", service_name);
        Ok(())
    }

    /// Get health status of all services
    pub async fn get_all_health(&self) -> Result<HashMap<String, ServiceHealth>> {
        let statuses = self.health_statuses.read().await;
        Ok(statuses.clone())
    }

    /// Get health status of a specific service
    pub async fn get_service_health(&self, service_name: &str) -> Result<Option<ServiceHealth>> {
        let statuses = self.health_statuses.read().await;
        Ok(statuses.get(service_name).cloned())
    }

    /// Shutdown the health monitor
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }
        tracing::info!("Health monitor shutdown");
        Ok(())
    }
}

impl Default for ServiceHealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{InMemoryEventPublisher, DaemonEvent};

    #[tokio::test]
    async fn test_service_manager_creation() {
        let router = crucible_services::presets::development().unwrap();
        let manager = ServiceManager::new(router).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_file_service() {
        let file_service = FileService::new();
        let info = file_service.service_info().await.unwrap();
        assert_eq!(info.name, "file_service");

        let health = file_service.health_check().await.unwrap();
        assert!(matches!(health.status, crucible_services::types::HealthStatus::Healthy));
    }

    #[tokio::test]
    async fn test_event_service() {
        let (publisher, _receiver) = InMemoryEventPublisher::new();
        let event_service = EventService::new(Arc::new(publisher));

        let info = event_service.service_info().await.unwrap();
        assert_eq!(info.name, "event_service");
    }

    #[tokio::test]
    async fn test_sync_service() {
        let sync_service = SyncService::new();
        let info = sync_service.service_info().await.unwrap();
        assert_eq!(info.name, "sync_service");
    }

    #[tokio::test]
    async fn test_health_monitor() {
        let monitor = ServiceHealthMonitor::new();
        let file_service = Arc::new(FileService::new());

        monitor.start_monitoring(file_service).await.unwrap();

        let health = monitor.get_service_health("file_service").await.unwrap();
        assert!(health.is_some());
    }
}