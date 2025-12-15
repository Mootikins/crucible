//\! Event handlers for database operations.
//\!
//\! This module provides handlers that subscribe to events from the EventBus
//\! and perform database operations in response. The handlers implement the
//\! event-driven architecture where file changes cascade through the system:
//\!
//\! ```text
//\! FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated
//\!                    ^              ^               ^
//\!               ParserHandler  StorageHandler  StorageHandler
//\! ```
//\!
//\! # Handlers
//\!
//\! - [`StorageHandler`]: Handles `NoteParsed`, `FileDeleted`, `FileMoved` events
//\!   to store/update/delete entities in the database. Emits `EntityStored`,
//\!   `EntityDeleted`, and `BlocksUpdated` events.
//\!
//\! - [`TagHandler`]: Handles `NoteParsed` events to extract and associate tags
//\!   with entities. Emits `TagAssociated` events.

pub mod storage_handler;
pub mod tag_handler;

pub use storage_handler::StorageHandler;
pub use tag_handler::TagHandler;
