//! Daemon-owned storage backends.
//!
//! Houses the SQLite store: note metadata, the property/EAV store, the FTS5
//! text index, and the `notes.embedding` column that backs semantic search.

pub mod sqlite;
