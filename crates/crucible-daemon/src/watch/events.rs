//! File event types and related structures.

#![allow(clippy::type_complexity)]

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
        let is_dir = path.is_dir();
        Self {
            id: Uuid::new_v4(),
            kind,
            path,
            timestamp: Utc::now(),
            is_dir,
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
#[derive(Default)]
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

impl std::fmt::Debug for EventFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventFilter")
            .field("extensions", &self.extensions)
            .field("exclude_extensions", &self.exclude_extensions)
            .field("include_dirs", &self.include_dirs)
            .field("exclude_dirs", &self.exclude_dirs)
            .field("min_size", &self.min_size)
            .field("max_size", &self.max_size)
            .field(
                "custom_filter",
                &self.custom_filter.as_ref().map(|_| "<function>"),
            )
            .finish()
    }
}

impl Clone for EventFilter {
    fn clone(&self) -> Self {
        Self {
            extensions: self.extensions.clone(),
            exclude_extensions: self.exclude_extensions.clone(),
            include_dirs: self.include_dirs.clone(),
            exclude_dirs: self.exclude_dirs.clone(),
            min_size: self.min_size,
            max_size: self.max_size,
            custom_filter: None, // Cannot clone closures
        }
    }
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
        // Check if the event path itself is inside an excluded directory
        // (either the path starts with the excluded dir, or is the excluded dir)
        let event_path = &event.path;
        if self.exclude_dirs.iter().any(|d| event_path.starts_with(d)) {
            return false;
        }

        // Also check parent-based include filtering
        if let Some(parent) = event.parent() {
            if !self.include_dirs.is_empty()
                && !self.include_dirs.iter().any(|d| parent.starts_with(d))
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn md_event(name: &str) -> FileEvent {
        FileEvent::new(FileEventKind::Modified, PathBuf::from(name))
    }

    fn event_with_size(name: &str, size: u64) -> FileEvent {
        let meta = EventMetadata {
            size: Some(size),
            permissions: None,
            mime_type: None,
            content_hash: None,
            backend: "test".into(),
            watch_id: "w1".into(),
        };
        FileEvent::with_metadata(FileEventKind::Created, PathBuf::from(name), meta)
    }

    // -- FileEvent --

    #[test]
    fn file_event_extension_returns_lowercase() {
        let ev = md_event("/foo/bar.MD");
        assert_eq!(ev.extension(), Some("md".to_string()));
    }

    #[test]
    fn file_event_extension_none_for_no_ext() {
        let ev = md_event("/foo/Makefile");
        assert_eq!(ev.extension(), None);
    }

    #[test]
    fn file_event_file_name() {
        let ev = md_event("/a/b/c.txt");
        assert_eq!(ev.file_name(), Some("c.txt".to_string()));
    }

    #[test]
    fn file_event_parent() {
        let ev = md_event("/a/b/c.txt");
        assert_eq!(ev.parent(), Some(PathBuf::from("/a/b")));
    }

    // -- FileEventKind --

    #[test]
    fn created_and_modified_affect_content() {
        assert!(FileEventKind::Created.affects_content());
        assert!(FileEventKind::Modified.affects_content());
        assert!(!FileEventKind::Deleted.affects_content());
    }

    #[test]
    fn deleted_and_moved_are_removal() {
        assert!(FileEventKind::Deleted.is_removal());
        assert!(FileEventKind::Moved {
            from: PathBuf::from("a"),
            to: PathBuf::from("b"),
        }
        .is_removal());
        assert!(!FileEventKind::Created.is_removal());
    }

    #[test]
    fn event_kind_as_str() {
        assert_eq!(FileEventKind::Created.as_str(), "created");
        assert_eq!(FileEventKind::Modified.as_str(), "modified");
        assert_eq!(FileEventKind::Deleted.as_str(), "deleted");
        assert_eq!(
            FileEventKind::Moved {
                from: PathBuf::new(),
                to: PathBuf::new(),
            }
            .as_str(),
            "moved"
        );
        assert_eq!(FileEventKind::Batch(vec![]).as_str(), "batch");
        assert_eq!(FileEventKind::Unknown("x".into()).as_str(), "unknown");
    }

    // -- EventMetadata --

    #[test]
    fn event_metadata_new_sets_backend_and_watch_id() {
        let m = EventMetadata::new("notify".into(), "watch-1".into());
        assert_eq!(m.backend, "notify");
        assert_eq!(m.watch_id, "watch-1");
        assert!(m.size.is_none());
    }

    // -- EventFilter --

    #[test]
    fn filter_by_extension_includes_only_matching() {
        let filter = EventFilter::new().with_extension("md");
        assert!(filter.matches(&md_event("/foo/note.md")));
        assert!(!filter.matches(&md_event("/foo/note.txt")));
    }

    #[test]
    fn filter_exclude_extension() {
        let filter = EventFilter::new().exclude_extension("log");
        assert!(filter.matches(&md_event("/foo/note.md")));
        assert!(!filter.matches(&md_event("/foo/app.log")));
    }

    #[test]
    fn filter_exclude_dir() {
        let filter = EventFilter::new().exclude_dir("/tmp/cache");
        assert!(filter.matches(&md_event("/home/user/note.md")));
        assert!(!filter.matches(&md_event("/tmp/cache/file.md")));
    }

    #[test]
    fn filter_include_dir() {
        let filter = EventFilter::new().include_dir("/notes");
        assert!(filter.matches(&md_event("/notes/sub/file.md")));
        assert!(!filter.matches(&md_event("/other/file.md")));
    }

    #[test]
    fn filter_size_limits() {
        let filter = EventFilter::new().with_size_limits(Some(100), Some(10_000));
        assert!(filter.matches(&event_with_size("ok.md", 500)));
        assert!(!filter.matches(&event_with_size("small.md", 10)));
        assert!(!filter.matches(&event_with_size("big.md", 100_000)));
    }

    #[test]
    fn filter_custom_fn() {
        let filter =
            EventFilter::new().with_custom_filter(|ev| ev.path.to_string_lossy().contains("keep"));
        assert!(filter.matches(&md_event("/keep/file.md")));
        assert!(!filter.matches(&md_event("/drop/file.md")));
    }

    #[test]
    fn filter_no_extension_rejected_when_extensions_required() {
        let filter = EventFilter::new().with_extension("md");
        assert!(!filter.matches(&md_event("/foo/Makefile")));
    }

    #[test]
    fn filter_clone_drops_custom_filter() {
        let filter = EventFilter::new().with_custom_filter(|_| true);
        let cloned = filter.clone();
        assert!(cloned.custom_filter.is_none());
    }

    #[test]
    fn empty_filter_allows_everything() {
        let filter = EventFilter::new();
        assert!(filter.matches(&md_event("/any/path.rs")));
    }
}
