//! Core traits for the file watching system.

use crate::watch::{error::Result, events::FileEvent};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Core trait for file watching backends.
#[async_trait]
pub trait FileWatcher: Send + Sync {
    /// Get the backend type identifier.
    fn backend_type(&self) -> &'static str;

    /// Set the event sender for this watcher.
    /// This must be called before adding any watches.
    fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>);

    /// Start watching the specified path with the given configuration.
    async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle>;

    /// Stop watching the specified path.
    async fn unwatch(&mut self, handle: WatchHandle) -> Result<()>;

    /// Get all active watches.
    fn active_watches(&self) -> Vec<WatchHandle>;

    /// Check if the backend is available on this platform.
    fn is_available(&self) -> bool;

    /// Get backend capabilities.
    fn capabilities(&self) -> BackendCapabilities;
}

/// Handle to an active watch.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WatchHandle {
    /// Unique identifier for this watch.
    pub id: String,

    /// Path being watched.
    pub path: PathBuf,
}

impl WatchHandle {
    /// Create a new watch handle.
    pub fn new(path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            path,
        }
    }
}

/// Configuration for a file watch.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Unique identifier for this watch configuration.
    pub id: String,

    /// Whether to watch recursively.
    pub recursive: bool,

    /// Event filter to apply.
    pub filter: Option<crate::watch::events::EventFilter>,

    /// Debouncing configuration.
    pub debounce: DebounceConfig,

    /// Additional backend-specific options.
    pub backend_options: std::collections::HashMap<String, serde_json::Value>,
}

impl WatchConfig {
    /// Create a new watch configuration.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            recursive: true,
            filter: None,
            debounce: DebounceConfig::default(),
            backend_options: std::collections::HashMap::new(),
        }
    }

    /// Replace the identifier.
    ///
    /// A watch group derives one id per path from a single template, so the
    /// backend's handle map keys them apart instead of overwriting.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set recursive watching.
    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set event filter.
    pub fn with_filter(mut self, filter: crate::watch::events::EventFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set debouncing configuration.
    pub fn with_debounce(mut self, debounce: DebounceConfig) -> Self {
        self.debounce = debounce;
        self
    }
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Debouncing configuration for file events.
#[derive(Debug, Clone)]
pub struct DebounceConfig {
    /// Debounce delay in milliseconds.
    pub delay_ms: u64,

    /// Maximum number of events to batch together.
    pub max_batch_size: usize,

    /// Whether to deduplicate identical events.
    pub deduplicate: bool,
}

impl DebounceConfig {
    /// Create a new debounce configuration.
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            max_batch_size: 100,
            deduplicate: true,
        }
    }

    /// Set maximum batch size.
    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self::new(100) // 100ms default debounce
    }
}

/// Backend capabilities.
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    /// Supports recursive watching.
    pub recursive: bool,

    /// Supports fine-grained event types.
    pub fine_grained_events: bool,

    /// Supports watching multiple paths.
    pub multiple_paths: bool,

    /// Supports hot reconfiguration.
    pub hot_reconfig: bool,

    /// Platform availability.
    pub platforms: Vec<String>,
}

impl BackendCapabilities {
    /// Create a capabilities instance with all features supported.
    pub fn full_support() -> Self {
        Self {
            recursive: true,
            fine_grained_events: true,
            multiple_paths: true,
            hot_reconfig: true,
            platforms: vec!["all".to_string()],
        }
    }

    /// Create a capabilities instance for basic support.
    pub fn basic() -> Self {
        Self {
            recursive: false,
            fine_grained_events: false,
            multiple_paths: true,
            hot_reconfig: false,
            platforms: vec!["all".to_string()],
        }
    }
}

/// Trait for handling file events.
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle a single file event.
    async fn handle(&self, event: FileEvent) -> Result<()>;

    /// Get the handler name.
    fn name(&self) -> &'static str;

    /// Get handler priority (higher numbers = higher priority).
    fn priority(&self) -> u32;

    /// Check if this handler can process the given event.
    fn can_handle(&self, event: &FileEvent) -> bool;
}
