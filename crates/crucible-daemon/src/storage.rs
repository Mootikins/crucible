//! Daemon-owned storage backends.
//!
//! Currently houses the SQLite metadata/graph store. LanceDB remains a
//! separate crate so its heavy transitive deps (arrow, datafusion, ...)
//! don't recompile whenever daemon code changes.

pub mod sqlite;
