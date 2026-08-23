//! Shared types of the file watching system: handles, configs, capabilities
//! and the `EventHandler` trait.

use crate::watch::{error::Result, events::FileEvent};
use async_trait::async_trait;
use std::path::PathBuf;

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
    ///
    /// The notify backend builds its debouncer from the first watch it gets.
    /// The polling backend waits for its poll interval instead.
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
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self::new(100) // 100ms default debounce
    }
}

/// What a backend can do. `WatchBackend::capabilities` is the one table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Supports recursive watching.
    pub recursive: bool,

    /// Supports fine-grained event types.
    pub fine_grained_events: bool,

    /// Supports watching multiple paths.
    pub multiple_paths: bool,

    /// Supports hot reconfiguration.
    pub hot_reconfig: bool,

    /// Platforms the backend runs on, as `std::env::consts::OS` names, or `"all"`.
    pub platforms: &'static [&'static str],
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
