//! File event types and related structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a file system event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEvent {
    /// Unique identifier for this event.
    pub id: Uuid,

    /// Kind of file event.
    pub kind: FileEventKind,

    /// Path to the file or directory.
    pub path: PathBuf,

    /// Timestamp when the event occurred.
    pub timestamp: DateTime<Utc>,

    /// Whether this is a directory.
    pub is_dir: bool,

    /// Optional metadata about the event.
    pub metadata: Option<EventMetadata>,
}

impl FileEvent {
    /// Create a new file event.
    pub fn new(kind: FileEventKind, path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            path,
            timestamp: Utc::now(),
            is_dir: path.is_dir(),
            metadata: None,
        }
    }

    /// Create a new file event with metadata.
    pub fn with_metadata(kind: FileEventKind, path: PathBuf, metadata: EventMetadata) -> Self {
        let mut event = Self::new(kind, path);
        event.metadata = Some(metadata);
        event
    }

    /// Get the file extension if available.
    pub fn extension(&self) -> Option<String> {
        self.path.extension()?.to_str().map(|s| s.to_lowercase())
    }

    /// Get the file name as a string.
    pub fn file_name(&self) -> Option<String> {
        self.path.file_name()?.to_str().map(|s| s.to_string())
    }

    /// Get the parent directory.
    pub fn parent(&self) -> Option<PathBuf> {
        self.path.parent().map(|p| p.to_path_buf())
    }
}

/// Kinds of file events that can occur.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileEventKind {
    /// File or directory was created.
    Created,
    /// File or directory was modified.
    Modified,
    /// File or directory was deleted.
    Deleted,
    /// File or directory was moved/renamed.
    Moved {
        /// Original path before the move.
        from: PathBuf,
        /// New path after the move.
        to: PathBuf,
    },
    /// Multiple events occurred (used for batched operations).
    Batch(Vec<FileEvent>),
    /// Unknown event type.
    Unknown(String),
}

impl FileEventKind {
    /// Check if this event affects file content.
    pub fn affects_content(&self) -> bool {
        matches!(self, Self::Created | Self::Modified)
    }

    /// Check if this event represents a file removal.
    pub fn is_removal(&self) -> bool {
        matches!(self, Self::Deleted | Self::Moved { .. })
    }

    /// Get a string representation of the event kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Moved { .. } => "moved",
            Self::Batch(_) => "batch",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Additional metadata about file events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetadata {
    /// File size in bytes.
    pub size: Option<u64>,

    /// File permissions (octal representation).
    pub permissions: Option<u32>,

    /// MIME type if known.
    pub mime_type: Option<String>,

    /// Content hash if available.
    pub content_hash: Option<String>,

    /// Source backend that generated this event.
    pub backend: String,

    /// Watch configuration that triggered this event.
    pub watch_id: String,
}

impl EventMetadata {
    /// Create new metadata with required fields.
    pub fn new(backend: String, watch_id: String) -> Self {
        Self {
            size: None,
            permissions: None,
            mime_type: None,
            content_hash: None,
            backend,
            watch_id,
        }
    }
}

/// Event filtering criteria.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Include only these file extensions.
    pub extensions: Vec<String>,

    /// Exclude these file extensions.
    pub exclude_extensions: Vec<String>,

    /// Include only these directories.
    pub include_dirs: Vec<PathBuf>,

    /// Exclude these directories.
    pub exclude_dirs: Vec<PathBuf>,

    /// Minimum file size in bytes.
    pub min_size: Option<u64>,

    /// Maximum file size in bytes.
    pub max_size: Option<u64>,

    /// Custom filter function.
    pub custom_filter: Option<Box<dyn Fn(&FileEvent) -> bool + Send + Sync>>,
}

impl EventFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an extension to include.
    pub fn with_extension(mut self, ext: impl Into<String>) -> Self {
        self.extensions.push(ext.into());
        self
    }

    /// Add an extension to exclude.
    pub fn exclude_extension(mut self, ext: impl Into<String>) -> Self {
        self.exclude_extensions.push(ext.into());
        self
    }

    /// Add a directory to include.
    pub fn include_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.include_dirs.push(dir.into());
        self
    }

    /// Add a directory to exclude.
    pub fn exclude_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.exclude_dirs.push(dir.into());
        self
    }

    /// Set size constraints.
    pub fn with_size_limits(mut self, min: Option<u64>, max: Option<u64>) -> Self {
        self.min_size = min;
        self.max_size = max;
        self
    }

    /// Add a custom filter function.
    pub fn with_custom_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&FileEvent) -> bool + Send + Sync + 'static,
    {
        self.custom_filter = Some(Box::new(filter));
        self
    }

    /// Check if an event passes this filter.
    pub fn matches(&self, event: &FileEvent) -> bool {
        // Check extension filters
        if let Some(ext) = event.extension() {
            if !self.extensions.is_empty() && !self.extensions.contains(&ext) {
                return false;
            }
            if self.exclude_extensions.contains(&ext) {
                return false;
            }
        } else if !self.extensions.is_empty() {
            // No extension but we have required extensions
            return false;
        }

        // Check directory filters
        if let Some(parent) = event.parent() {
            if !self.include_dirs.is_empty() && !self.include_dirs.iter().any(|d| parent.starts_with(d)) {
                return false;
            }
            if self.exclude_dirs.iter().any(|d| parent.starts_with(d)) {
                return false;
            }
        }

        // Check size filters (if metadata is available)
        if let Some(metadata) = &event.metadata {
            if let Some(size) = metadata.size {
                if let Some(min) = self.min_size {
                    if size < min {
                        return false;
                    }
                }
                if let Some(max) = self.max_size {
                    if size > max {
                        return false;
                    }
                }
            }
        }

        // Apply custom filter
        if let Some(ref filter) = self.custom_filter {
            if !filter(event) {
                return false;
            }
        }

        true
    }
}