//! # Crucible File Watching System
//!
//! A comprehensive, production-ready file watching architecture for the Crucible ecosystem.
//! Provides configurable folder watching, multi-backend support, and seamless integration
//! with the embedding database, Rune tools, and Obsidian API.
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
//! ```rust,no_run
//! use crucible_watch::{WatchManager, WatchConfig};
//! use crucible_config::Config;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config::load_from_file("config.yaml").await?;
//!     let manager = WatchManager::new(config).await?;
//!
//!     // Start watching configured folders
//!     manager.start().await?;
//!
//!     // Keep the watcher running
//!     tokio::signal::ctrl_c().await?;
//!     manager.shutdown().await?;
//!
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]

mod backends;
mod config;
mod embedding_events;
mod error;
mod event_driven_embedding_processor;
mod events;
mod handlers;
mod manager;
mod message_channel_infrastructure;
mod traits;
mod utils;

pub use backends::*;
pub use config::*;
pub use embedding_events::*;
pub use error::*;
pub use event_driven_embedding_processor::*;
pub use events::*;
pub use handlers::*;
pub use manager::*;
pub use message_channel_infrastructure::*;
pub use traits::{
    BackendCapabilities, DebounceConfig, EventHandler, FileWatcher, WatchHandle, WatchMode,
};

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
        event_driven_embedding_processor::{
            EmbeddingEventHandler, EventDrivenEmbeddingProcessor, EventProcessorMetrics,
        },
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_imports() {
        // Verify core types are exportable
        let _kind: FileEventKind = FileEventKind::Created;
        let _config = WatchConfig::default();
    }
}
