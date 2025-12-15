//! Event System Runtime Wiring
//!
//! This module provides the unified event system initialization for Crucible CLI.
//! It wires together:
//! - `EventBus` from `crucible-rune` for event dispatch
//! - `StorageHandler` and `TagHandler` from `crucible-surrealdb` for database events
//! - `EmbeddingHandler` from `crucible-enrichment` for embedding generation
//! - `WatchManager` from `crucible-watch` for file system monitoring
//! - Rune handlers from kiln `.crucible/handlers/` directory
//!
//! # Event Flow
//!
//! ```text
//! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingRequested -> EmbeddingGenerated
//!      ^            ^              ^               ^                  ^                     ^
//!    Watch       Parser         Storage         Storage           Embedding            Embedding
//! ```
//!
//! # Handler Priorities
//!
//! - 100: StorageHandler (entity persistence)
//! - 110: TagHandler (tag association)
//! - 200: EmbeddingHandler (embedding generation)
//! - 500+: Rune handlers (custom logic)
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
