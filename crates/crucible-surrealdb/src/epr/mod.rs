//! Entity/Property/Relation (EPR) data structures and helpers.
//!
//! This module defines strongly-typed structs that mirror the new schema in
//! `schema_epr.surql`. The goal is to provide a small, composable API surface
//! that other crates can consume without worrying about raw SurrealDB records.

pub mod types;

pub use types::*;
