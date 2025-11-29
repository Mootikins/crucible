//! Database statistics
//!
//! This module re-exports statistics-related functions from kiln_integration.rs.
//! Future work: Move implementations here for better organization.

// Re-export from legacy kiln_integration module
pub use crate::kiln_integration::get_database_stats;
