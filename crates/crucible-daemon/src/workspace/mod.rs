//! Workspace-scoped helpers used by `session.create`'s setup task.
//!
//! Currently exposes pure indexing helpers for the workspace root (git/files)
//! and the kiln root (markdown notes). Kept free of daemon state so callers
//! can invoke them from any context — including `tokio::task::spawn_blocking`.

pub mod indexer;
