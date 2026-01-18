//! Event System Runtime Wiring
//!
//! This module provides the unified event system initialization for Crucible CLI
//! using the Reactor pattern from `crucible_core::events`.
//!
//! It wires together:
//! - `Reactor` from `crucible-core` for unified event dispatch
//! - `EmbeddingHandler` from `crucible-enrichment` for embedding generation
//! - `WatchManager` from `crucible-watch` for file system monitoring
//! - `LuaHandler` from `crucible-lua` for user scripts
//!
//! Note: StorageHandler and TagHandler were removed in Phase 4 cleanup.
//! Storage is now handled through NoteStore trait implementations.
//!
//! # Event Flow
//!
//! ```text
//! FileChanged -> NoteParsed -> NoteStored -> EmbeddingRequested -> EmbeddingGenerated
//!      ^            ^             ^                 ^                     ^
//!    Watch       Parser        NoteStore       Embedding            Embedding
//! ```
//!
//! # Handler Dependencies
//!
//! Handlers declare dependencies for proper ordering:
//! - EmbeddingHandler: runs for parsed notes
//! - Lua handlers: run after all built-in handlers (priority 500+)
//!
//! # Usage
//!
//! ```rust,ignore
//! use crucible_cli::event_system::initialize_event_system;
//!
//! let config = CliConfig::load()?;
//! let handle = initialize_event_system(&config).await?;
//!
//! // System is now running - file changes trigger the event cascade
//!
//! // Graceful shutdown
//! handle.shutdown().await?;
//! ```

mod handle;
mod initialization;

pub use handle::EventSystemHandle;
pub use initialization::initialize_event_system;

#[cfg(test)]
mod tests;
