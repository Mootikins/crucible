//! Storage Contract Tests
//!
//! This crate contains contract tests that verify storage backend implementations
//! conform to the EavGraphStorage trait contracts. Both SQLite and SurrealDB
//! implementations must pass these tests identically.
//!
//! # Running Tests
//!
//! ```bash
//! # Test SQLite backend
//! cargo test -p crucible-storage-tests --features sqlite
//!
//! # Test SurrealDB backend
//! cargo test -p crucible-storage-tests --features surrealdb
//! ```
//!
//! This crate has no library code - it exists only to hold contract tests.
