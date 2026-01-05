//! Crucible Benchmarks and Contract Tests
//!
//! This crate contains:
//! - Contract tests verifying storage backends conform to trait contracts
//! - Benchmarks comparing storage and graph query performance across backends
//!
//! # Running Tests
//!
//! ```bash
//! # Test SQLite backend
//! cargo test -p crucible-benchmarks --features sqlite
//!
//! # Test SurrealDB backend
//! cargo test -p crucible-benchmarks --features surrealdb
//! ```
//!
//! # Running Benchmarks
//!
//! ```bash
//! # All backends
//! cargo bench -p crucible-benchmarks --features sqlite,surrealdb,lance
//!
//! # Individual backends
//! cargo bench -p crucible-benchmarks --features sqlite
//! cargo bench -p crucible-benchmarks --features surrealdb
//! ```

pub mod fixtures;
