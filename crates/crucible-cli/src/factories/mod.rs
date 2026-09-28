//! Factory functions for creating infrastructure implementations
//!
//! This module is the composition root where concrete types are assembled
//! and returned as trait objects.

pub mod embedding;
pub mod storage;

pub use embedding::embedding_provider_config_from_cli;
pub use storage::{get_storage, get_storage_with_summary, CliStorageHandle, KilnOpenSummary};
