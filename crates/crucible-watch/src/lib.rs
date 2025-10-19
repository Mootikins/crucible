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

mod error;
mod events;
mod traits;
mod backends;
mod manager;
mod config;
mod handlers;
mod utils;

pub use error::*;
pub use events::*;
pub use traits::*;
pub use backends::*;
pub use manager::*;
pub use config::*;
pub use handlers::*;

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
        FileWatcher, FileEvent, FileEventKind, WatchManager,
        WatchBackend, EventHandler, Error, Result,
    };
    // Re-export both WatchConfig types with clear names
    pub use crate::traits::WatchConfig as TraitWatchConfig;
    pub use crate::config::WatchConfig as ConfigWatchConfig;
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