//! # Crucible File Watching System
//!
//! A comprehensive, production-ready file watching architecture for the Crucible ecosystem.
//! Provides configurable folder watching, multi-backend support, and seamless integration
//! with the embedding database and external tools.
//!
//! ## Features
//!
//! - **Configurable folder watching** using crucible-config system
//! - **Editor integration preparation** with low-frequency inode watching
//! - **Multi-backend support** (notify, polling, editor integration)
//! - **Performance optimization** with efficient debouncing and event queuing
//! - **Seamless integration** with existing Crucible systems
//!
//! ## Architecture Overview
//!
//! The system is built around a trait-based architecture that allows for multiple
//! file watching backends while maintaining a consistent interface:
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   Application   │───▶│   WatchManager   │───▶│   FileWatcher   │
//! │                 │    │                  │    │    Backend      │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//!         │                       │                       │
//!         ▼                       ▼                       ▼
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │ Event Handlers  │    │   Event Queue    │    │   File Events   │
//! │ (Indexing,      │    │   (Debouncing,   │    │ (Created,       │
//! │  Hot Reload)    │    │    Filtering)    │    │ Modified, etc.) │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//! ```
//!
//! ## Quick Start
//!

#![warn(clippy::all)]
#![deny(unsafe_code)]
#![allow(
    clippy::ptr_arg,
    clippy::field_reassign_with_default,
    clippy::stable_sort_primitive,
    clippy::assertions_on_constants,
    clippy::unnecessary_sort_by,
    missing_docs
)]

pub mod backends;
mod change_detector;
pub mod config;
pub mod error;
mod events;
mod file_scanner;
pub mod handlers;
mod manager;

pub mod traits;
pub mod types;
mod utils;

pub use backends::{
    BackendRegistry, EditorConfig, EditorFactory, EditorWatcher, NotifyFactory, NotifyWatcher,
    PollingFactory, PollingWatcher, WatcherFactory,
};
pub use change_detector::{
    CacheStatistics, ChangeDetector, ChangeDetectorConfig, ChangeDetectorStatistics,
};
pub use config::{
    AdvancedFilterConfig, BackpressureStrategy, ConfigValidator, CpuConfig, DebounceConfig,
    EventProcessingConfig, ExportConfig, ExportFormat, FileWatchingConfig, FilterConfig,
    FrequencyLimitConfig, GlobalWatchConfig, MemoryConfig, MonitoringConfig, TimeWindowConfig,
    ValidationError, WatchLoggingConfig, WatchManagerConfig, WatchModeConfig, WatchPath,
    WatchPerformanceConfig, WatchProfile,
};
pub use error::{Error, Result};
pub use events::{EventFilter, EventMetadata, FileEvent, FileEventKind};
pub use file_scanner::{FileScanner, ScanStatistics, WatchConfig, WatchResult};
pub use handlers::{
    CompositeHandler, HandlerRegistry, IndexingHandler, ObsidianSyncHandler, ParserHandler,
};
pub use manager::WatchManager;

pub use traits::{
    BackendCapabilities, DebounceConfig as TraitDebounceConfig, EventHandler, FileWatcher,
    WatchConfig as TraitWatchConfig, WatchHandle, WatchMode,
};
pub use types::{FileInfo, FilePermissions, FileType};

/// Available file watching backends.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub enum WatchBackend {
    /// High-performance backend using OS-specific file system notifications
    Notify,
    /// Cross-platform polling backend
    Polling,
    /// Low-frequency backend for editor integrations
    Editor,
}

pub mod prelude {
    pub use crate::watch::config::DebounceConfig as ConfigDebounceConfig;
    pub use crate::watch::config::WatchConfig as ConfigWatchConfig;
    pub use crate::watch::traits::DebounceConfig as TraitDebounceConfig;
    pub use crate::watch::traits::WatchConfig as TraitWatchConfig;
    pub use crate::watch::{
        Error, EventHandler, FileEvent, FileEventKind, FileWatcher, Result, WatchBackend,
        WatchManager,
    };
}
