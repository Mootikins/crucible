//! Data layer events for the daemon
//!
//! Defines all events that can be published by the daemon to notify the core controller
//! about filesystem changes, database updates, sync status, and errors.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Daemon event types
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
}

/// Filesystem events
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
    /// File metadata (size, permissions, etc.)
    pub metadata: FileMetadata,
    /// Additional event data
    pub data: HashMap<String, serde_json::Value>,
}

/// Filesystem event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilesystemEventType {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
    /// File was renamed/moved
    Renamed { from: PathBuf, to: PathBuf },
    /// Directory was created
    DirectoryCreated,
    /// Directory was deleted
    DirectoryDeleted,
    /// File permissions changed
    PermissionChanged,
    /// File metadata changed
    MetadataChanged,
}

/// File metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: Option<u64>,
    /// File modification time
    pub modified_time: Option<DateTime<Utc>>,
    /// File creation time
    pub created_time: Option<DateTime<Utc>>,
    /// File permissions (Unix mode)
    pub permissions: Option<u32>,
    /// File MIME type (if detected)
    pub mime_type: Option<String>,
    /// File checksum (for integrity verification)
    pub checksum: Option<String>,
    /// Custom metadata
    pub custom: HashMap<String, String>,
}

/// Database events
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
    /// Event data
    pub data: HashMap<String, serde_json::Value>,
}

/// Database event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DatabaseEventType {
    /// Database was created
    DatabaseCreated,
    /// Database was dropped
    DatabaseDropped,
    /// Table was created
    TableCreated,
    /// Table was dropped
    TableDropped,
    /// Record was inserted
    RecordInserted,
    /// Record was updated
    RecordUpdated,
    /// Record was deleted
    RecordDeleted,
    /// Index was created
    IndexCreated,
    /// Index was dropped
    IndexDropped,
    /// Transaction was committed
    TransactionCommitted,
    /// Transaction was rolled back
    TransactionRolledBack,
    /// Backup was created
    BackupCreated,
    /// Backup was restored
    BackupRestored,
}

/// Sync events
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
    /// Progress information
    pub progress: SyncProgress,
    /// Additional sync data
    pub data: HashMap<String, serde_json::Value>,
}

/// Sync event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncEventType {
    /// Sync operation started
    Started,
    /// Sync operation completed successfully
    Completed,
    /// Sync operation failed
    Failed { error: String },
    /// Sync operation was paused
    Paused,
    /// Sync operation was resumed
    Resumed,
    /// Sync progress update
    Progress,
    /// Sync conflict detected
    ConflictDetected { conflict_type: String },
    /// Sync conflict resolved
    ConflictResolved,
}

/// Sync progress information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncProgress {
    /// Total items to sync
    pub total_items: Option<u64>,
    /// Items processed so far
    pub processed_items: u64,
    /// Items that failed to sync
    pub failed_items: u64,
    /// Estimated time remaining (in seconds)
    pub eta_seconds: Option<u64>,
    /// Current operation
    pub current_operation: String,
    /// Progress percentage (0.0 to 1.0)
    pub percentage: f32,
}

/// Error events
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
    /// Detailed error description
    pub details: Option<String>,
    /// Stack trace (if available)
    pub stack_trace: Option<String>,
    /// Context information
    pub context: HashMap<String, serde_json::Value>,
    /// Whether the error is recoverable
    pub recoverable: bool,
    /// Suggested actions to resolve the error
    pub suggested_actions: Vec<String>,
}

/// Error severity levels
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
    /// Fatal error that will cause the daemon to stop
    Fatal,
}

/// Error categories
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

/// Health check events
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
    /// Health metrics
    pub metrics: HashMap<String, f64>,
    /// Additional health data
    pub data: HashMap<String, serde_json::Value>,
}

/// Health status
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
            metadata: FileMetadata::default(),
            data: HashMap::new(),
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
            data: HashMap::new(),
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
            progress: SyncProgress::default(),
            data: HashMap::new(),
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
            stack_trace: None,
            context: HashMap::new(),
            recoverable: true,
            suggested_actions: Vec::new(),
        }
    }

    /// Create a new health event
    pub fn health(service: String, status: HealthStatus) -> HealthEvent {
        HealthEvent {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            service,
            status,
            metrics: HashMap::new(),
            data: HashMap::new(),
        }
    }
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            size: None,
            modified_time: None,
            created_time: None,
            permissions: None,
            mime_type: None,
            checksum: None,
            custom: HashMap::new(),
        }
    }
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            total_items: None,
            processed_items: 0,
            failed_items: 0,
            eta_seconds: None,
            current_operation: "Starting".to_string(),
            percentage: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_filesystem_event_creation() {
        let event = EventBuilder::filesystem(
            FilesystemEventType::Created,
            PathBuf::from("/test/file.txt"),
        );

        assert_eq!(event.event_type, FilesystemEventType::Created);
        assert_eq!(event.path, PathBuf::from("/test/file.txt"));
    }

    #[test]
    fn test_database_event_creation() {
        let event = EventBuilder::database(
            DatabaseEventType::RecordInserted,
            "test_db".to_string(),
        );

        assert_eq!(event.event_type, DatabaseEventType::RecordInserted);
        assert_eq!(event.database, "test_db");
    }
}