//! Core traits for the file watching system.

use crate::{error::Result, events::FileEvent};
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
    pub filter: Option<crate::events::EventFilter>,

    /// Debouncing configuration.
    pub debounce: DebounceConfig,

    /// Event handler configuration.
    pub handler_config: HandlerConfig,

    /// Watch mode.
    pub mode: WatchMode,

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
            handler_config: HandlerConfig::default(),
            mode: WatchMode::Standard,
            backend_options: std::collections::HashMap::new(),
        }
    }

    /// Set recursive watching.
    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set event filter.
    pub fn with_filter(mut self, filter: crate::events::EventFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set debouncing configuration.
    pub fn with_debounce(mut self, debounce: DebounceConfig) -> Self {
        self.debounce = debounce;
        self
    }

    /// Set handler configuration.
    pub fn with_handler_config(mut self, config: HandlerConfig) -> Self {
        self.handler_config = config;
        self
    }

    /// Set watch mode.
    pub fn with_mode(mut self, mode: WatchMode) -> Self {
        self.mode = mode;
        self
    }

    /// Add a backend-specific option.
    pub fn with_backend_option(mut self, key: String, value: serde_json::Value) -> Self {
        self.backend_options.insert(key, value);
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

    /// Enable or disable deduplication.
    pub fn with_deduplication(mut self, enabled: bool) -> Self {
        self.deduplicate = enabled;
        self
    }
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self::new(100) // 100ms default debounce
    }
}

/// Event handler configuration.
#[derive(Debug, Clone)]
pub struct HandlerConfig {
    /// Channel buffer size for events.
    pub buffer_size: usize,

    /// Maximum number of concurrent handlers.
    pub max_concurrent: usize,

    /// Whether to preserve event order.
    pub preserve_order: bool,

    /// Handler timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl HandlerConfig {
    /// Create a new handler configuration.
    pub fn new() -> Self {
        Self {
            buffer_size: 1000,
            max_concurrent: 10,
            preserve_order: false,
            timeout_ms: Some(5000), // 5 second default timeout
        }
    }

    /// Set buffer size.
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set maximum concurrent handlers.
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Enable or disable order preservation.
    pub fn with_order_preservation(mut self, preserve: bool) -> Self {
        self.preserve_order = preserve;
        self
    }

    /// Set handler timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Watch mode determines how events are processed.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchMode {
    /// Standard watching with immediate event processing.
    Standard,

    /// Batched watching for better performance.
    Batched,

    /// Low-frequency watching for editor integrations.
    LowFrequency {
        /// Polling interval in milliseconds.
        interval_ms: u64,
    },

    /// Custom mode with backend-specific configuration.
    Custom(String),
}

impl WatchMode {
    /// Get the default polling interval for low-frequency mode.
    pub fn default_low_frequency_interval() -> u64 {
        5000 // 5 seconds
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
    fn priority(&self) -> u32 {
        100
    }

    /// Check if this handler can process the given event.
    fn can_handle(&self, _event: &FileEvent) -> bool {
        true
    }
}

/// Trait for event filtering and transformation.
#[async_trait]
#[allow(dead_code)]
pub trait EventProcessor: Send + Sync {
    /// Process a batch of events and return the transformed events.
    async fn process(&self, events: Vec<FileEvent>) -> Result<Vec<FileEvent>>;

    /// Get the processor name.
    fn name(&self) -> &'static str;
}