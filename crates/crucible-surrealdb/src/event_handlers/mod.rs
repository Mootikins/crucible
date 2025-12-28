//! Event handlers for database operations.
//!
//! This module provides handlers that subscribe to events from the Reactor
//! and perform database operations in response. The handlers implement the
//! event-driven architecture where file changes cascade through the system:
//!
//! ```text
//! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated
//!                    ^              ^               ^
//!               ParserHandler  StorageHandler  StorageHandler
//! ```
//!
//! # Handlers
//!
//! - [`StorageHandler`]: Handles `NoteParsed`, `FileDeleted`, `FileMoved` events
//!   to store/update/delete entities in the database. Emits `EntityStored`,
//!   `EntityDeleted`, and `BlocksUpdated` events.
//!
//! - [`TagHandler`]: Handles `NoteParsed` events to extract and associate tags
//!   with entities. Emits `TagAssociated` events.
//!
//! # Reactor Integration
//!
//! Use the adapter types to register handlers with the unified Reactor:
//!
//! ```rust,ignore
//! use crucible_surrealdb::event_handlers::{StorageHandlerAdapter, TagHandlerAdapter};
//! use crucible_core::events::Reactor;
//!
//! let mut reactor = Reactor::new();
//! reactor.register(Box::new(StorageHandlerAdapter::new(storage_handler)))?;
//! reactor.register(Box::new(TagHandlerAdapter::new(tag_handler)))?;
//! ```

pub mod core_adapters;
pub mod storage_handler;
pub mod tag_handler;

pub use core_adapters::{StorageHandlerAdapter, TagHandlerAdapter};
pub use storage_handler::StorageHandler;
pub use tag_handler::TagHandler;
