//! Crucible Daemon Library
//!
//! Data layer coordination daemon for Crucible knowledge management system.
//! Provides filesystem watching, parsing, database synchronization, and event publishing.

pub mod config;
pub mod coordinator;
pub mod events;
pub mod handlers;
pub mod services;
pub mod surrealdb_service;

// Re-export main types
pub use config::DaemonConfig;
pub use coordinator::DataCoordinator;
pub use events::*;
pub use services::*;
