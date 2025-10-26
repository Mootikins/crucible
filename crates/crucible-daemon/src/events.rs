//! Simplified event system for crucible-daemon
//!
//! Simple event types and communication using tokio::broadcast channels.
//! Replaces the complex service event routing architecture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Maximum number of events to retain in broadcast channels
const MAX_EVENT_RETENTION: usize = 1000;

/// Simple daemon event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DaemonEvent {
    /// Filesystem monitoring events
    Filesystem(FilesystemEvent),
    /// Database synchronization events
    Database(DatabaseEvent),
    /// Sync status and progress events
    Sync(SyncEvent),
    /// Error and warning events
    Error(ErrorEvent),
    /// Health check and monitoring events
    Health(HealthEvent),
    /// Service lifecycle events
    Service(ServiceEvent),
}

/// Simple filesystem events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type
    pub event_type: FilesystemEventType,
    /// Affected file path
    pub path: PathBuf,
    /// Optional source path for rename operations
    pub source_path: Option<PathBuf>,
}

/// Simple filesystem event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilesystemEventType {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed/moved
    Renamed,
    /// Directory was created
    DirectoryCreated,
    /// Directory was deleted
    DirectoryDeleted,
}

/// Simple database events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type
    pub event_type: DatabaseEventType,
    /// Database name
    pub database: String,
    /// Table/collection name (if applicable)
    pub table: Option<String>,
    /// Record ID (if applicable)
    pub record_id: Option<String>,
}

/// Simple database event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DatabaseEventType {
    /// Record was inserted
    RecordInserted,
    /// Record was updated
    RecordUpdated,
    /// Record was deleted
    RecordDeleted,
    /// Table was created
    TableCreated,
    /// Database operation completed
    OperationCompleted,
}

/// Simple sync events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type
    pub event_type: SyncEventType,
    /// Sync source
    pub source: String,
    /// Sync target
    pub target: String,
    /// Optional progress percentage (0.0 to 1.0)
    pub progress: Option<f32>,
}

/// Simple sync event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncEventType {
    /// Sync operation started
    Started,
    /// Sync operation completed successfully
    Completed,
    /// Sync operation failed
    Failed,
    /// Sync operation was paused
    Paused,
    /// Sync operation was resumed
    Resumed,
    /// Sync progress update
    Progress,
}

/// Simple error events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Error category
    pub category: ErrorCategory,
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Optional detailed error description
    pub details: Option<String>,
}

/// Simple error severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorSeverity {
    /// Debug information
    Debug,
    /// Informational message
    Info,
    /// Warning message
    Warning,
    /// Error that should be addressed
    Error,
    /// Critical error requiring immediate attention
    Critical,
}

/// Simple error categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorCategory {
    /// Filesystem-related errors
    Filesystem,
    /// Database-related errors
    Database,
    /// Network-related errors
    Network,
    /// Configuration errors
    Configuration,
    /// Authentication/authorization errors
    Authentication,
    /// Validation errors
    Validation,
    /// Performance-related errors
    Performance,
    /// Resource-related errors (memory, disk, etc.)
    Resource,
    /// Synchronization errors
    Synchronization,
    /// Unknown/uncategorized errors
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Filesystem => write!(f, "Filesystem"),
            ErrorCategory::Database => write!(f, "Database"),
            ErrorCategory::Network => write!(f, "Network"),
            ErrorCategory::Configuration => write!(f, "Configuration"),
            ErrorCategory::Authentication => write!(f, "Authentication"),
            ErrorCategory::Validation => write!(f, "Validation"),
            ErrorCategory::Performance => write!(f, "Performance"),
            ErrorCategory::Resource => write!(f, "Resource"),
            ErrorCategory::Synchronization => write!(f, "Synchronization"),
            ErrorCategory::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Simple health check events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Service name
    pub service: String,
    /// Health status
    pub status: HealthStatus,
    /// Optional health message
    pub message: Option<String>,
}

/// Simple health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is degraded but functional
    Degraded,
    /// Service is unhealthy
    Unhealthy,
    /// Service is in maintenance mode
    Maintenance,
    /// Service status is unknown
    Unknown,
}

/// Simple service events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceEvent {
    /// Unique event identifier
    pub event_id: Uuid,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event type
    pub event_type: ServiceEventType,
    /// Service ID
    pub service_id: String,
    /// Service type
    pub service_type: String,
    /// Optional event data
    pub data: Option<serde_json::Value>,
}

/// Simple service event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceEventType {
    /// Service was started
    Started,
    /// Service was stopped
    Stopped,
    /// Service was registered
    Registered,
    /// Service was unregistered
    Unregistered,
    /// Health check completed
    HealthCheck,
    /// Service status changed
    StatusChanged,
    /// Service failed
    Failed,
}

/// Simple event bus using tokio::broadcast
#[derive(Clone)]
pub struct EventBus {
    /// Event sender
    sender: broadcast::Sender<DaemonEvent>,
    /// Event statistics
    stats: Arc<RwLock<EventStats>>,
}

/// Event statistics
#[derive(Debug, Default, Clone)]
pub struct EventStats {
    /// Total events published
    pub total_published: u64,
    /// Total events by type
    pub by_type: HashMap<String, u64>,
    /// Last event timestamp
    pub last_event: Option<DateTime<Utc>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(MAX_EVENT_RETENTION);
        let stats = Arc::new(RwLock::new(EventStats::default()));

        Self { sender, stats }
    }

    /// Publish an event
    pub async fn publish(&self, event: DaemonEvent) -> Result<usize, String> {
        let event_type_name = self.get_event_type_name(&event);

        match self.sender.send(event.clone()) {
            Ok(receiver_count) => {
                // Update statistics
                let mut stats = self.stats.write().await;
                stats.total_published += 1;
                *stats.by_type.entry(event_type_name).or_insert(0) += 1;
                stats.last_event = Some(Utc::now());

                Ok(receiver_count)
            }
            Err(_) => Err("No active receivers".to_string()),
        }
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.sender.subscribe()
    }

    /// Get event statistics
    pub async fn get_stats(&self) -> EventStats {
        self.stats.read().await.clone()
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
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event builder for creating events with default values
pub struct EventBuilder;

impl EventBuilder {
    /// Create a new filesystem event
    pub fn filesystem(event_type: FilesystemEventType, path: PathBuf) -> FilesystemEvent {
        FilesystemEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            path,
            source_path: None,
        }
    }

    /// Create a new filesystem rename event
    pub fn filesystem_rename(from: PathBuf, to: PathBuf) -> FilesystemEvent {
        FilesystemEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: FilesystemEventType::Renamed,
            path: to,
            source_path: Some(from),
        }
    }

    /// Create a new database event
    pub fn database(event_type: DatabaseEventType, database: String) -> DatabaseEvent {
        DatabaseEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            database,
            table: None,
            record_id: None,
        }
    }

    /// Create a new database record event
    pub fn database_record(
        event_type: DatabaseEventType,
        database: String,
        table: String,
        record_id: String,
    ) -> DatabaseEvent {
        DatabaseEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            database,
            table: Some(table),
            record_id: Some(record_id),
        }
    }

    /// Create a new sync event
    pub fn sync(event_type: SyncEventType, source: String, target: String) -> SyncEvent {
        SyncEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            source,
            target,
            progress: None,
        }
    }

    /// Create a new sync progress event
    pub fn sync_progress(source: String, target: String, progress: f32) -> SyncEvent {
        SyncEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: SyncEventType::Progress,
            source,
            target,
            progress: Some(progress),
        }
    }

    /// Create a new error event
    pub fn error(
        severity: ErrorSeverity,
        category: ErrorCategory,
        code: String,
        message: String,
    ) -> ErrorEvent {
        ErrorEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity,
            category,
            code,
            message,
            details: None,
        }
    }

    /// Create a new error event with details
    pub fn error_with_details(
        severity: ErrorSeverity,
        category: ErrorCategory,
        code: String,
        message: String,
        details: String,
    ) -> ErrorEvent {
        ErrorEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            severity,
            category,
            code,
            message,
            details: Some(details),
        }
    }

    /// Create a new health event
    pub fn health(service: String, status: HealthStatus) -> HealthEvent {
        HealthEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            service,
            status,
            message: None,
        }
    }

    /// Create a new health event with message
    pub fn health_with_message(
        service: String,
        status: HealthStatus,
        message: String,
    ) -> HealthEvent {
        HealthEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            service,
            status,
            message: Some(message),
        }
    }

    /// Create a new service event
    pub fn service(
        event_type: ServiceEventType,
        service_id: String,
        service_type: String,
    ) -> ServiceEvent {
        ServiceEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            service_id,
            service_type,
            data: None,
        }
    }

    /// Create a new service event with data
    pub fn service_with_data(
        event_type: ServiceEventType,
        service_id: String,
        service_type: String,
        data: serde_json::Value,
    ) -> ServiceEvent {
        ServiceEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            service_id,
            service_type,
            data: Some(data),
        }
    }
}

/// Convert crucible-watch FileEvent to DaemonEvent
pub fn convert_watch_event_to_daemon_event(
    event: crucible_watch::FileEvent,
) -> Result<DaemonEvent, String> {
    let event_type = match event.kind {
        crucible_watch::FileEventKind::Created => FilesystemEventType::Created,
        crucible_watch::FileEventKind::Modified => FilesystemEventType::Modified,
        crucible_watch::FileEventKind::Deleted => FilesystemEventType::Deleted,
        // Handle other event types as modifications
        _ => FilesystemEventType::Modified,
    };

    let fs_event = EventBuilder::filesystem(event_type, event.path);
    Ok(DaemonEvent::Filesystem(fs_event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_event_bus_creation_and_subscription() {
        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();

        // Publish an event
        let event = DaemonEvent::Health(EventBuilder::health(
            "test-service".to_string(),
            HealthStatus::Healthy,
        ));

        let published_count = event_bus.publish(event).await.unwrap();
        assert!(published_count >= 1); // At least our receiver

        // Receive the event
        let received_event = receiver.recv().await.unwrap();
        match received_event {
            DaemonEvent::Health(health_event) => {
                assert_eq!(health_event.service, "test-service");
                assert_eq!(health_event.status, HealthStatus::Healthy);
            }
            _ => panic!("Expected health event"),
        }
    }

    #[tokio::test]
    async fn test_event_statistics() {
        let event_bus = EventBus::new();

        // Publish some events
        let fs_event = DaemonEvent::Filesystem(EventBuilder::filesystem(
            FilesystemEventType::Created,
            PathBuf::from("/test/file.txt"),
        ));
        event_bus.publish(fs_event).await.unwrap();

        let error_event = DaemonEvent::Error(EventBuilder::error(
            ErrorSeverity::Warning,
            ErrorCategory::Filesystem,
            "TEST_ERROR".to_string(),
            "Test error message".to_string(),
        ));
        event_bus.publish(error_event).await.unwrap();

        // Check statistics
        let stats = event_bus.get_stats().await;
        assert_eq!(stats.total_published, 2);
        assert_eq!(stats.by_type.get("filesystem"), Some(&1));
        assert_eq!(stats.by_type.get("error"), Some(&1));
        assert!(stats.last_event.is_some());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let event_bus = EventBus::new();
        let mut receiver1 = event_bus.subscribe();
        let mut receiver2 = event_bus.subscribe();

        let event = DaemonEvent::Service(EventBuilder::service(
            ServiceEventType::Started,
            "test-service".to_string(),
            "test-type".to_string(),
        ));

        let published_count = event_bus.publish(event).await.unwrap();
        assert_eq!(published_count, 2); // Both receivers

        // Both receivers should get the event
        let received1 = receiver1.recv().await.unwrap();
        let received2 = receiver2.recv().await.unwrap();

        match (&received1, &received2) {
            (DaemonEvent::Service(_), DaemonEvent::Service(_)) => {
                // Both events are service events
            }
            _ => panic!("Both events should be service events"),
        }
    }

    #[test]
    fn test_event_builder_creates_valid_events() {
        let fs_event = EventBuilder::filesystem(
            FilesystemEventType::Created,
            PathBuf::from("/test/file.txt"),
        );
        assert_eq!(fs_event.event_type, FilesystemEventType::Created);
        assert_eq!(fs_event.path, PathBuf::from("/test/file.txt"));
        assert!(fs_event.source_path.is_none());

        let rename_event = EventBuilder::filesystem_rename(
            PathBuf::from("/test/old.txt"),
            PathBuf::from("/test/new.txt"),
        );
        assert_eq!(rename_event.event_type, FilesystemEventType::Renamed);
        assert_eq!(rename_event.path, PathBuf::from("/test/new.txt"));
        assert_eq!(
            rename_event.source_path,
            Some(PathBuf::from("/test/old.txt"))
        );

        let db_event = EventBuilder::database_record(
            DatabaseEventType::RecordInserted,
            "test_db".to_string(),
            "test_table".to_string(),
            "record_123".to_string(),
        );
        assert_eq!(db_event.event_type, DatabaseEventType::RecordInserted);
        assert_eq!(db_event.database, "test_db");
        assert_eq!(db_event.table, Some("test_table".to_string()));
        assert_eq!(db_event.record_id, Some("record_123".to_string()));

        let sync_event =
            EventBuilder::sync_progress("source".to_string(), "target".to_string(), 0.75);
        assert_eq!(sync_event.event_type, SyncEventType::Progress);
        assert_eq!(sync_event.progress, Some(0.75));

        let error_event = EventBuilder::error_with_details(
            ErrorSeverity::Error,
            ErrorCategory::Database,
            "DB_ERROR".to_string(),
            "Database connection failed".to_string(),
            "Connection timeout after 30 seconds".to_string(),
        );
        assert_eq!(error_event.severity, ErrorSeverity::Error);
        assert_eq!(error_event.category, ErrorCategory::Database);
        assert_eq!(
            error_event.details,
            Some("Connection timeout after 30 seconds".to_string())
        );

        let health_event = EventBuilder::health_with_message(
            "api-service".to_string(),
            HealthStatus::Degraded,
            "High latency detected".to_string(),
        );
        assert_eq!(health_event.service, "api-service");
        assert_eq!(health_event.status, HealthStatus::Degraded);
        assert_eq!(
            health_event.message,
            Some("High latency detected".to_string())
        );

        let service_event = EventBuilder::service_with_data(
            ServiceEventType::StatusChanged,
            "web-server".to_string(),
            "http-server".to_string(),
            serde_json::json!({"old_status": "stopped", "new_status": "running"}),
        );
        assert_eq!(service_event.event_type, ServiceEventType::StatusChanged);
        assert!(service_event.data.is_some());
    }
}
