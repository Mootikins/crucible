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
//! // TODO: Add example once API stabilizes

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod backends;
mod change_detector;
pub mod config;
mod embedding_events;
pub mod error;
// TODO: event_driven_embedding_processor uses EmbeddingThreadPool which no longer exists
// mod event_driven_embedding_processor;
mod events;
mod file_scanner;
pub mod handlers;
mod manager;
mod message_channel_infrastructure;
pub mod traits;
pub mod types;
mod utils;

pub use backends::*;
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
pub use embedding_events::*;
pub use error::*;
// TODO: event_driven_embedding_processor uses EmbeddingThreadPool which no longer exists
// pub use event_driven_embedding_processor::*;
pub use events::*;
pub use file_scanner::{
    FileScanner, NoOpProgressReporter, ScanProgressReporter, ScanStatistics, WatchConfig,
    WatchResult,
};
pub use handlers::*;
pub use manager::*;
pub use message_channel_infrastructure::*;
pub use traits::{BackendCapabilities, EventHandler, FileWatcher, WatchHandle, WatchMode};
pub use types::*;

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

/// Re-export common types for convenience
pub mod prelude {
    pub use crate::{
        embedding_events::{
            create_embedding_metadata, determine_content_type, determine_event_priority,
            generate_document_id, EmbeddingEvent, EmbeddingEventMetadata, EmbeddingEventPriority,
            EmbeddingEventResult, EventDrivenEmbeddingConfig,
        },
        // Event-driven embedding components
        // TODO: event_driven_embedding_processor uses EmbeddingThreadPool which no longer exists
        // event_driven_embedding_processor::{
        //     EmbeddingEventHandler, EventDrivenEmbeddingProcessor, EventProcessorMetrics,
        // },
        Error,
        EventHandler,
        FileEvent,
        FileEventKind,
        FileWatcher,
        Result,
        WatchBackend,
        WatchManager,
    };
    // Re-export both WatchConfig types with clear names
    pub use crate::config::WatchConfig as ConfigWatchConfig;
    pub use crate::traits::WatchConfig as TraitWatchConfig;
    // Re-export both DebounceConfig types with clear names
    pub use crate::config::DebounceConfig as ConfigDebounceConfig;
    pub use crate::traits::DebounceConfig as TraitDebounceConfig;
}
