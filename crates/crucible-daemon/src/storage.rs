//! Daemon-owned storage backends.
//!
//! Houses the SQLite metadata/graph store and the LanceDB vector store.

pub mod lance;
pub mod sqlite;
