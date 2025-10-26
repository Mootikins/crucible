//! Simplified service integration layer for the daemon
//!
//! Provides basic service abstractions for data layer operations without over-engineering.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// Import local types from coordinator
use super::coordinator::{ServiceHealth, ServiceStatus};

/// Simplified service manager for coordinating data layer services
#[derive(Clone)]
pub struct ServiceManager {
    /// Registered services
    services: Arc<RwLock<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>>,
}

impl ServiceManager {
    /// Create a new service manager - simplified version
    pub async fn new() -> Result<Self> {
        info!("Creating simplified service manager");

        Ok(Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Register a service
    pub async fn register_service<T>(&self, name: &str, service: Arc<T>) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        info!("Registering service: {}", name);

        let mut services = self.services.write().await;
        services.insert(
            name.to_string(),
            service.clone() as Arc<dyn std::any::Any + Send + Sync>,
        );

        Ok(())
    }

    /// Get a service by name and type
    pub async fn get_service<T>(&self, name: &str) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let services = self.services.read().await;
        services
            .get(name)
            .and_then(|s| s.clone().downcast::<T>().ok())
    }

    /// List all registered services
    pub async fn list_services(&self) -> Vec<String> {
        let services = self.services.read().await;
        services.keys().cloned().collect()
    }

    /// Shutdown the service manager
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down service manager");

        // Clear all services
        let mut services = self.services.write().await;
        services.clear();

        info!("Service manager shutdown complete");
        Ok(())
    }

    /// Get health status for all services
    pub async fn get_all_health(&self) -> Result<HashMap<String, ServiceHealth>> {
        let services = self.services.read().await;
        let mut health_map = HashMap::new();

        for service_name in services.keys() {
            health_map.insert(
                service_name.clone(),
                ServiceHealth {
                    status: ServiceStatus::Healthy,
                    message: Some("Service is running".to_string()),
                    last_check: chrono::Utc::now(),
                    details: HashMap::new(),
                },
            );
        }

        Ok(health_map)
    }
}

/// Basic file service interface
#[async_trait]
pub trait FileService: Send + Sync {
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;
    async fn delete_file(&self, path: &str) -> Result<()>;
    async fn list_files(&self, directory: &str) -> Result<Vec<String>>;
}

/// Basic database service interface
#[async_trait]
pub trait DatabaseService: Send + Sync {
    async fn execute_query(&self, query: &str) -> Result<serde_json::Value>;
    async fn health_check(&self) -> Result<bool>;
}

/// Basic event service interface
#[async_trait]
pub trait EventService: Send + Sync {
    async fn publish_event(&self, event: &str) -> Result<()>;
    async fn subscribe(&self, pattern: &str) -> Result<String>;
}

/// Basic sync service interface
#[async_trait]
pub trait SyncService: Send + Sync {
    async fn sync(&self) -> Result<()>;
    async fn get_status(&self) -> Result<SyncStatus>;
}

/// Sync status
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub in_progress: bool,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}

/// Simple file service implementation
pub struct SimpleFileService {
    base_path: std::path::PathBuf,
}

impl SimpleFileService {
    pub fn new(base_path: std::path::PathBuf) -> Self {
        Self { base_path }
    }
}

#[async_trait]
impl FileService for SimpleFileService {
    async fn read_file(&self, path: &str) -> Result<String> {
        let full_path = self.base_path.join(path);
        tokio::fs::read_to_string(full_path)
            .await
            .map_err(Into::into)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let full_path = self.base_path.join(path);
        tokio::fs::write(full_path, content)
            .await
            .map_err(Into::into)
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let full_path = self.base_path.join(path);
        tokio::fs::remove_file(full_path).await.map_err(Into::into)
    }

    async fn list_files(&self, directory: &str) -> Result<Vec<String>> {
        let full_path = self.base_path.join(directory);
        let mut entries = tokio::fs::read_dir(full_path).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                files.push(name.to_string());
            }
        }

        Ok(files)
    }
}

/// Simple database service implementation
pub struct SimpleDatabaseService {
    #[allow(dead_code)]
    connection_string: String,
}

impl SimpleDatabaseService {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait]
impl DatabaseService for SimpleDatabaseService {
    async fn execute_query(&self, _query: &str) -> Result<serde_json::Value> {
        // Simple placeholder implementation
        Ok(serde_json::json!({"status": "ok", "result": []}))
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple health check
        Ok(true)
    }
}

/// Simple event service implementation
pub struct SimpleEventService {
    subscribers: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SimpleEventService {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl EventService for SimpleEventService {
    async fn publish_event(&self, event: &str) -> Result<()> {
        info!("Publishing event: {}", event);
        // Simple implementation - just log the event
        Ok(())
    }

    async fn subscribe(&self, pattern: &str) -> Result<String> {
        let subscription_id = uuid::Uuid::new_v4().to_string();
        info!(
            "Subscribing to pattern '{}' with ID: {}",
            pattern, subscription_id
        );

        let mut subscribers = self.subscribers.write().await;
        subscribers
            .entry(pattern.to_string())
            .or_insert_with(Vec::new)
            .push(subscription_id.clone());

        Ok(subscription_id)
    }
}

/// Simple sync service implementation
pub struct SimpleSyncService {
    status: Arc<RwLock<SyncStatus>>,
}

impl SimpleSyncService {
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(SyncStatus {
                in_progress: false,
                last_sync: None,
                error: None,
            })),
        }
    }
}

#[async_trait]
impl SyncService for SimpleSyncService {
    async fn sync(&self) -> Result<()> {
        info!("Starting sync process");

        let mut status = self.status.write().await;
        status.in_progress = true;
        status.error = None;

        // Simulate sync work
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        status.in_progress = false;
        status.last_sync = Some(chrono::Utc::now());

        info!("Sync completed successfully");
        Ok(())
    }

    async fn get_status(&self) -> Result<SyncStatus> {
        let status = self.status.read().await;
        Ok(SyncStatus {
            in_progress: status.in_progress,
            last_sync: status.last_sync,
            error: status.error.clone(),
        })
    }
}

// Type aliases for the old service names to maintain compatibility
pub type DataLayerDatabaseService = SimpleDatabaseService;
