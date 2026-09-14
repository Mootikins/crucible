//! Native file watching with debouncing, filtering, and bounded event delivery.
//!
//! Each watch group owns one NotifyWatcher, sharing an OS watcher across its
//! paths. The manager dispatches changes to indexing and external-edit handlers.
//! Both the native and manager debounce stages are intentional: capture
//! suppression accounts for their combined delay.

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

pub use backends::NotifyWatcher;
pub use error::{Error, Result};
pub use events::{EventFilter, EventMetadata, FileEvent, FileEventKind};
pub use external_changes::{
    CaptureWindow, ExternalChange, ExternalChangeTracker, ExternalChangeWatch, Ownership,
};
pub use handlers::{ExternalChangeHandler, HandlerRegistry, IndexingHandler};
pub use manager::{WatchManager, WatchManagerConfig};

pub use traits::{DebounceConfig, EventHandler, WatchConfig, WatchHandle};
