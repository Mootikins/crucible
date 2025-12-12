//! Document CRUD operations
//!
//! This module re-exports document-related functions from kiln_integration.rs.
//! Future work: Move implementations here for better organization.

// Re-export from legacy kiln_integration module
pub use crate::kiln_integration::{retrieve_parsed_document, store_parsed_document};
