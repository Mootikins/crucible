//! Event System Runtime Wiring
//!
//! This module provides the unified event system initialization for Crucible CLI
//! using the Reactor pattern from `crucible_core::events`.
//!
//! It wires together:
//! - `Reactor` from `crucible-core` for unified event dispatch
//! - `StorageHandler` and `TagHandler` from `crucible-surrealdb` for database events
//! - `EmbeddingHandler` from `crucible-enrichment` for embedding generation
//! - `WatchManager` from `crucible-watch` for file system monitoring
//! - `RuneHandler` from `crucible-rune` for user scripts
//!
//! # Event Flow
//!
//! ```text
//! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingRequested -> EmbeddingGenerated
//!      ^            ^              ^               ^                  ^                     ^
//!    Watch       Parser         Storage         Storage           Embedding            Embedding
//! ```
//!
//! # Handler Dependencies
//!
//! Handlers declare dependencies for proper ordering:
//! - StorageHandler: no dependencies (runs first)
//! - TagHandler: depends on `storage_handler`
//! - EmbeddingHandler: depends on `storage_handler`, `tag_handler`
//! - Rune handlers: run after all built-in handlers (priority 500+)
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
