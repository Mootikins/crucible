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
//! `WatchManager` drives one `Backend` per watch group. `Backend` is an enum
//! over the three backends, so every backend offers one interface:
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
    clippy::field_reassign_with_default,
    clippy::stable_sort_primitive,
    clippy::assertions_on_constants,
    clippy::unnecessary_sort_by,
    missing_docs
)]

pub mod backends;
pub mod error;
mod events;
pub mod external_changes;
pub mod handlers;
mod manager;

pub mod traits;
mod utils;

pub use backends::{
    Backend, EditorConfig, EditorWatcher, NotifyWatcher, PollingWatcher, WatchBackend,
};
pub use error::{Error, Result};
pub use events::{EventFilter, EventMetadata, FileEvent, FileEventKind};
pub use external_changes::{
    CaptureWindow, ExternalChange, ExternalChangeTracker, ExternalChangeWatch, Ownership,
};
pub use handlers::{ExternalChangeHandler, HandlerRegistry, IndexingHandler};
pub use manager::{WatchManager, WatchManagerConfig};

pub use traits::{BackendCapabilities, DebounceConfig, EventHandler, WatchConfig, WatchHandle};
