//! Event-driven embedding integration for crucible-watch.
//!
//! This module provides the EmbeddingEvent structures and related functionality
//! to bridge file system events with the embedding processing pipeline, eliminating
//! inefficient polling and providing real-time, event-driven processing.
//!
//! # Deprecation Notice
//!
//! The types in this module are deprecated in favor of `SessionEvent` variants in
//! `crucible_core::events`:
//!
//! - `EmbeddingEvent` → Use `SessionEvent::EmbeddingRequested`
//! - `EmbeddingEventResult` → Use `SessionEvent::EmbeddingStored` or `SessionEvent::EmbeddingFailed`
//! - `EmbeddingEventPriority` → Use `crucible_core::events::Priority`
//!
//! The `SessionEvent` system provides a unified event model across all Crucible
//! components, enabling better integration with the event bus and handler system.

use crate::FileEventKind;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Represents an embedding request derived from file system events.
///
/// # Deprecation
///
/// This type is deprecated. Use `SessionEvent::EmbeddingRequested` from `crucible_core::events`
/// instead. The `SessionEvent` system provides unified event handling across all Crucible
/// components.
///
/// ## Migration
///
/// ```ignore
/// // Old code:
/// let event = EmbeddingEvent::new(path, trigger, content, metadata);
///
/// // New code:
/// use crucible_core::events::{SessionEvent, Priority};
/// let event = SessionEvent::EmbeddingRequested {
///     entity_id: format!("note:{}", path.display()),
///     block_id: None,
///     priority: Priority::Normal,
/// };
/// ```
#[deprecated(
    since = "0.1.0",
    note = "Use SessionEvent::EmbeddingRequested from crucible_core::events instead"
)]
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

    /// Note ID for embedding storage
    pub document_id: String,

    /// Timestamp when this embedding event was created
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Metadata about the embedding request
    pub metadata: EmbeddingEventMetadata,
}

#[allow(deprecated)]
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

/// Metadata for embedding events.
///
/// # Deprecation
///
/// This type is deprecated. Metadata should be encoded in the `entity_id` or
/// handled through separate storage queries when using `SessionEvent::EmbeddingRequested`.
#[deprecated(
    since = "0.1.0",
    note = "Use SessionEvent::EmbeddingRequested from crucible_core::events instead"
)]
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

#[allow(deprecated)]
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

/// Priority levels for embedding events.
///
/// # Deprecation
///
/// This type is deprecated. Use `crucible_core::events::Priority` instead, which provides
/// the same priority levels (`Low`, `Normal`, `High`, `Critical`) and integrates with
/// the unified `SessionEvent` system.
#[deprecated(since = "0.1.0", note = "Use crucible_core::events::Priority instead")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum EmbeddingEventPriority {
    /// Low priority - background processing
    Low = 1,
    /// Normal priority - standard processing
    #[default]
    Normal = 2,
    /// High priority - user-initiated changes
    High = 3,
    /// Critical priority - system-critical updates
    Critical = 4,
}

#[allow(deprecated)]

/// Result of processing an embedding event.
///
/// # Deprecation
///
/// This type is deprecated. Use `SessionEvent::EmbeddingStored` for successful results
/// or `SessionEvent::EmbeddingFailed` for failures. These variants from `crucible_core::events`
/// provide a unified event model.
///
/// ## Migration
///
/// ```ignore
/// // Old code:
/// let result = EmbeddingEventResult::success(event_id, processing_time, dimensions);
///
/// // New code:
/// use crucible_core::events::SessionEvent;
/// let event = SessionEvent::EmbeddingStored {
///     entity_id: entity_id.to_string(),
///     block_id: None,
///     dimensions: 256,
///     model: "nomic-embed-text".to_string(),
/// };
/// ```
#[deprecated(
    since = "0.1.0",
    note = "Use SessionEvent::EmbeddingStored or SessionEvent::EmbeddingFailed from crucible_core::events instead"
)]
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

#[allow(deprecated)]
impl EmbeddingEventResult {
    /// Create a successful result
    pub fn success(event_id: Uuid, processing_time: Duration, embedding_dimensions: usize) -> Self {
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

/// Configuration for the event-driven embedding integration.
///
/// # Deprecation
///
/// This type is deprecated along with the `EmbeddingEvent` system. New code should
/// use the `SessionEvent` variants for embedding-related events. Configuration for
/// embedding behavior should be managed through the embedding provider configuration.
#[deprecated(
    since = "0.1.0",
    note = "Use SessionEvent-based embedding pipeline instead"
)]
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

#[allow(deprecated)]
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

/// Generate a note ID from file path and content
pub fn generate_document_id(file_path: &PathBuf, content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash the file path and content to create a unique note ID
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

/// Determine priority from file event kind and path.
///
/// # Deprecation
///
/// This function is deprecated along with `EmbeddingEventPriority`. Use
/// `crucible_core::events::Priority` directly instead.
#[allow(deprecated)]
#[deprecated(
    since = "0.1.0",
    note = "Use crucible_core::events::Priority directly instead"
)]
pub fn determine_event_priority(
    event_kind: &FileEventKind,
    file_path: &PathBuf,
) -> EmbeddingEventPriority {
    // Critical events
    if matches!(event_kind, FileEventKind::Created) {
        // Check if it's in a critical path (e.g., config files)
        if file_path.components().any(|c| {
            c.as_os_str().to_string_lossy().contains("config")
                || c.as_os_str().to_string_lossy().contains("settings")
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

/// Create embedding metadata from file path and event.
///
/// # Deprecation
///
/// This function is deprecated along with `EmbeddingEventMetadata`. Use
/// `SessionEvent::EmbeddingRequested` which doesn't require this metadata.
#[allow(deprecated)]
#[deprecated(
    since = "0.1.0",
    note = "Use SessionEvent::EmbeddingRequested from crucible_core::events instead"
)]
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
