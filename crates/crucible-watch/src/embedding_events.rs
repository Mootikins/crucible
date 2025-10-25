//! Event-driven embedding integration for crucible-watch.
//!
//! This module provides the EmbeddingEvent structures and related functionality
//! to bridge file system events with the embedding processing pipeline, eliminating
//! inefficient polling and providing real-time, event-driven processing.

use crate::events::FileEventKind;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Represents an embedding request derived from file system events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingEvent {
    /// Unique identifier for this embedding request
    pub id: Uuid,

    /// Path to the file that triggered this embedding request
    pub file_path: PathBuf,

    /// Type of file system event that triggered this request
    pub trigger_event: FileEventKind,

    /// Content to be embedded (extracted from file)
    pub content: String,

    /// Document ID for embedding storage
    pub document_id: String,

    /// Timestamp when this embedding event was created
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Metadata about the embedding request
    pub metadata: EmbeddingEventMetadata,
}

impl EmbeddingEvent {
    /// Create a new embedding event
    pub fn new(
        file_path: PathBuf,
        trigger_event: FileEventKind,
        content: String,
        metadata: EmbeddingEventMetadata,
    ) -> Self {
        let id = Uuid::new_v4();
        let document_id = generate_document_id(&file_path, &content);
        let timestamp = chrono::Utc::now();

        Self {
            id,
            file_path,
            trigger_event,
            content,
            document_id,
            timestamp,
            metadata,
        }
    }

    /// Create a high priority embedding event
    pub fn with_priority(
        file_path: PathBuf,
        trigger_event: FileEventKind,
        content: String,
        priority: EmbeddingEventPriority,
    ) -> Self {
        let metadata = EmbeddingEventMetadata {
            priority,
            ..Default::default()
        };
        Self::new(file_path, trigger_event, content, metadata)
    }

    /// Get the content type for this event
    pub fn content_type(&self) -> &str {
        &self.metadata.content_type
    }

    /// Check if this event is batched
    pub fn is_batched(&self) -> bool {
        self.metadata.is_batched
    }

    /// Get the priority of this event
    pub fn priority(&self) -> EmbeddingEventPriority {
        self.metadata.priority.clone()
    }

    /// Convert this event to a batched event with the given batch ID
    pub fn to_batched(mut self, batch_id: Uuid) -> Self {
        self.metadata.is_batched = true;
        self.metadata.batch_id = Some(batch_id);
        self
    }
}

/// Metadata for embedding events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingEventMetadata {
    /// File size in bytes
    pub file_size: Option<u64>,

    /// File extension
    pub file_extension: Option<String>,

    /// Content type (markdown, text, etc.)
    pub content_type: String,

    /// Whether this is a batched event
    pub is_batched: bool,

    /// Batch identifier if this is part of a batch
    pub batch_id: Option<Uuid>,

    /// Priority level for processing
    pub priority: EmbeddingEventPriority,
}

impl Default for EmbeddingEventMetadata {
    fn default() -> Self {
        Self {
            file_size: None,
            file_extension: None,
            content_type: "text/plain".to_string(),
            is_batched: false,
            batch_id: None,
            priority: EmbeddingEventPriority::default(),
        }
    }
}

/// Priority levels for embedding events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EmbeddingEventPriority {
    /// Low priority - background processing
    Low = 1,
    /// Normal priority - standard processing
    Normal = 2,
    /// High priority - user-initiated changes
    High = 3,
    /// Critical priority - system-critical updates
    Critical = 4,
}

impl Default for EmbeddingEventPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Result of processing an embedding event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingEventResult {
    /// Event ID
    pub event_id: Uuid,

    /// Whether the embedding was successfully generated
    pub success: bool,

    /// Processing duration
    pub processing_time: Duration,

    /// Generated embedding dimensions (if successful)
    pub embedding_dimensions: Option<usize>,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Timestamp when processing completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

impl EmbeddingEventResult {
    /// Create a successful result
    pub fn success(
        event_id: Uuid,
        processing_time: Duration,
        embedding_dimensions: usize,
    ) -> Self {
        Self {
            event_id,
            success: true,
            processing_time,
            embedding_dimensions: Some(embedding_dimensions),
            error: None,
            completed_at: chrono::Utc::now(),
        }
    }

    /// Create a failed result
    pub fn failure(event_id: Uuid, processing_time: Duration, error: String) -> Self {
        Self {
            event_id,
            success: false,
            processing_time,
            embedding_dimensions: None,
            error: Some(error),
            completed_at: chrono::Utc::now(),
        }
    }
}

/// Configuration for the event-driven embedding integration
#[derive(Debug, Clone)]
pub struct EventDrivenEmbeddingConfig {
    /// Maximum batch size for processing multiple file changes
    pub max_batch_size: usize,

    /// Maximum time to wait before processing a batch (in milliseconds)
    pub batch_timeout_ms: u64,

    /// Maximum number of concurrent embedding requests
    pub max_concurrent_requests: usize,

    /// Maximum queue size for embedding events
    pub max_queue_size: usize,

    /// Retry configuration for failed embeddings
    pub max_retry_attempts: u32,

    /// Retry delay between attempts (in milliseconds)
    pub retry_delay_ms: u64,

    /// Whether to enable deduplication of identical events
    pub enable_deduplication: bool,

    /// Deduplication window in milliseconds
    pub deduplication_window_ms: u64,
}

impl Default for EventDrivenEmbeddingConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 16,
            batch_timeout_ms: 500,
            max_concurrent_requests: 8,
            max_queue_size: 1000,
            max_retry_attempts: 3,
            retry_delay_ms: 1000,
            enable_deduplication: true,
            deduplication_window_ms: 2000,
        }
    }
}

/// Generate a document ID from file path and content
pub fn generate_document_id(file_path: &PathBuf, content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash the file path and content to create a unique document ID
    file_path.hash(&mut hasher);
    content.hash(&mut hasher);

    // Combine with a prefix for easy identification
    format!("doc_{:016x}", hasher.finish())
}

/// Determine content type from file extension
pub fn determine_content_type(extension: Option<&str>) -> String {
    match extension {
        Some("md") => "text/markdown".to_string(),
        Some("txt") => "text/plain".to_string(),
        Some("rst") => "text/x-rst".to_string(),
        Some("adoc") => "text/x-asciidoc".to_string(),
        Some("html") => "text/html".to_string(),
        Some("json") => "application/json".to_string(),
        Some("yaml") | Some("yml") => "application/x-yaml".to_string(),
        Some("toml") => "application/x-toml".to_string(),
        Some("rs") => "text/x-rust".to_string(),
        Some("js") => "application/javascript".to_string(),
        Some("ts") => "application/typescript".to_string(),
        Some("py") => "text/x-python".to_string(),
        Some("sh") => "application/x-sh".to_string(),
        Some("css") => "text/css".to_string(),
        Some("scss") | Some("sass") => "text/x-sass".to_string(),
        _ => "text/plain".to_string(),
    }
}

/// Determine priority from file event kind and path
pub fn determine_event_priority(
    event_kind: &FileEventKind,
    file_path: &PathBuf,
) -> EmbeddingEventPriority {
    // Critical events
    if matches!(event_kind, FileEventKind::Created) {
        // Check if it's in a critical path (e.g., config files)
        if file_path.components().any(|c| {
            c.as_os_str().to_string_lossy().contains("config") ||
            c.as_os_str().to_string_lossy().contains("settings")
        }) {
            return EmbeddingEventPriority::Critical;
        }
        return EmbeddingEventPriority::High;
    }

    // High priority for modifications
    if matches!(event_kind, FileEventKind::Modified) {
        return EmbeddingEventPriority::Normal;
    }

    // Low priority for deletions (cleanup)
    if matches!(event_kind, FileEventKind::Deleted) {
        return EmbeddingEventPriority::Low;
    }

    // Default priority
    EmbeddingEventPriority::Normal
}

/// Create embedding metadata from file path and event
pub fn create_embedding_metadata(
    file_path: &PathBuf,
    event_kind: &FileEventKind,
    file_size: Option<u64>,
) -> EmbeddingEventMetadata {
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase());

    let content_type = determine_content_type(extension.as_deref());
    let priority = determine_event_priority(event_kind, file_path);

    EmbeddingEventMetadata {
        file_size,
        file_extension: extension,
        content_type,
        priority,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_document_id() {
        let path1 = PathBuf::from("/test/doc1.md");
        let path2 = PathBuf::from("/test/doc2.md");
        let content = "test content";

        let id1 = generate_document_id(&path1, content);
        let id2 = generate_document_id(&path2, content);

        assert_ne!(id1, id2);
        assert!(id1.starts_with("doc_"));
        assert!(id2.starts_with("doc_"));
    }

    #[test]
    fn test_determine_content_type() {
        assert_eq!(determine_content_type(Some("md")), "text/markdown");
        assert_eq!(determine_content_type(Some("txt")), "text/plain");
        assert_eq!(determine_content_type(Some("rst")), "text/x-rst");
        assert_eq!(determine_content_type(Some("unknown")), "text/plain");
        assert_eq!(determine_content_type(None), "text/plain");
    }

    #[test]
    fn test_determine_event_priority() {
        let config_path = PathBuf::from("/app/config/settings.md");
        let normal_path = PathBuf::from("/app/docs/normal.md");

        // Created events
        assert_eq!(
            determine_event_priority(&FileEventKind::Created, &config_path),
            EmbeddingEventPriority::Critical
        );
        assert_eq!(
            determine_event_priority(&FileEventKind::Created, &normal_path),
            EmbeddingEventPriority::High
        );

        // Modified events
        assert_eq!(
            determine_event_priority(&FileEventKind::Modified, &normal_path),
            EmbeddingEventPriority::Normal
        );

        // Deleted events
        assert_eq!(
            determine_event_priority(&FileEventKind::Deleted, &normal_path),
            EmbeddingEventPriority::Low
        );
    }

    #[test]
    fn test_embedding_event_creation() {
        let path = PathBuf::from("/test/doc.md");
        let content = "# Test Document\nThis is a test.";
        let metadata = EmbeddingEventMetadata::default();

        let event = EmbeddingEvent::new(
            path.clone(),
            FileEventKind::Created,
            content.to_string(),
            metadata,
        );

        assert_ne!(event.id, Uuid::nil());
        assert_eq!(event.file_path, path);
        assert_eq!(event.trigger_event, FileEventKind::Created);
        assert_eq!(event.content, content);
        assert!(!event.document_id.is_empty());
        assert!(event.timestamp <= chrono::Utc::now());
        assert!(!event.is_batched());
        assert_eq!(event.priority(), EmbeddingEventPriority::Normal);
    }

    #[test]
    fn test_embedding_event_batched() {
        let path = PathBuf::from("/test/doc.md");
        let content = "# Test Document";
        let batch_id = Uuid::new_v4();

        let event = EmbeddingEvent::with_priority(
            path,
            FileEventKind::Modified,
            content.to_string(),
            EmbeddingEventPriority::High,
        ).to_batched(batch_id);

        assert!(event.is_batched());
        assert_eq!(event.metadata.batch_id, Some(batch_id));
        assert_eq!(event.priority(), EmbeddingEventPriority::High);
    }

    #[test]
    fn test_embedding_event_result() {
        let event_id = Uuid::new_v4();
        let processing_time = Duration::from_millis(100);

        let success = EmbeddingEventResult::success(event_id, processing_time, 384);
        assert!(success.success);
        assert_eq!(success.event_id, event_id);
        assert_eq!(success.processing_time, processing_time);
        assert_eq!(success.embedding_dimensions, Some(384));
        assert!(success.error.is_none());

        let failure = EmbeddingEventResult::failure(
            event_id,
            processing_time,
            "Test error".to_string(),
        );
        assert!(!failure.success);
        assert_eq!(failure.event_id, event_id);
        assert_eq!(failure.processing_time, processing_time);
        assert!(failure.embedding_dimensions.is_none());
        assert_eq!(failure.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_create_embedding_metadata() {
        let path = PathBuf::from("/test/document.md");
        let file_size = Some(1024);

        let metadata = create_embedding_metadata(&path, &FileEventKind::Created, file_size);

        assert_eq!(metadata.file_size, Some(1024));
        assert_eq!(metadata.file_extension, Some("md".to_string()));
        assert_eq!(metadata.content_type, "text/markdown");
        assert_eq!(metadata.priority, EmbeddingEventPriority::High); // Created event
        assert!(!metadata.is_batched);
        assert!(metadata.batch_id.is_none());
    }

    #[test]
    fn test_event_driven_embedding_config_default() {
        let config = EventDrivenEmbeddingConfig::default();

        assert_eq!(config.max_batch_size, 16);
        assert_eq!(config.batch_timeout_ms, 500);
        assert_eq!(config.max_concurrent_requests, 8);
        assert_eq!(config.max_retry_attempts, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!(config.enable_deduplication);
        assert_eq!(config.deduplication_window_ms, 2000);
    }
}