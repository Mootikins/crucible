//! Entity-Attribute-Value + Graph (EAV+Graph) data structures and helpers.
//!
//! This module defines strongly-typed structs that mirror the new schema in
//! `schema_eav_graph.surql` and provides higher-level storage helpers used by the
//! ingestion pipeline.

pub mod adapter;
pub mod ingest;
pub mod schema;
pub mod store;
pub mod types;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod relation_tag_edge_case_tests;

pub use ingest::NoteIngestor;
pub use schema::apply_eav_graph_schema;
pub use store::EAVGraphStore;
pub use types::*;
