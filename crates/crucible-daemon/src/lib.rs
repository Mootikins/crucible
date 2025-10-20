//! Crucible Daemon Library
//!
//! Data layer coordination daemon for Crucible knowledge management system.
//! Provides filesystem watching, parsing, database synchronization, and event publishing.

pub mod coordinator;
pub mod events;
pub mod config;
pub mod services;
pub mod handlers;

// Re-export main types
pub use coordinator::DataCoordinator;
pub use events::*;
pub use config::DaemonConfig;
pub use services::*;
