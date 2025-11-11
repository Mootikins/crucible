//! # Crucible Sync
//!
//! CRDT-based synchronization for the Crucible knowledge management system.
//!
//! This crate provides:
//! - Yrs-based CRDT document synchronization
//! - Pluggable transport layer (WebSocket, memory, P2P)
//! - Multi-node architecture for different sync concerns
//! - Test-driven development approach
//!
//! ## Quick Start
//!
//! ```rust
//! use crucible_sync::{SyncInstance, Document};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let sync = SyncInstance::new("my-document").await?;
//!     sync.insert_text(0, "Hello, Crucible!").await?;
//!     Ok(())
//! }
//! ```

pub mod document;
pub mod sync;
pub mod transport;

// Re-export main types for convenience
pub use document::Document;
pub use sync::SyncInstance;


/// Core synchronization error types
#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error("Document error: {0}")]
    Document(#[from] document::DocumentError),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Operation not supported")]
    UnsupportedOperation,
}

/// Result type for sync operations
pub type SyncResult<T> = Result<T, SyncError>;
