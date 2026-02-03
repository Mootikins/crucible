//! Factory functions for creating infrastructure implementations
//!
//! This module is the composition root where concrete types are assembled
//! and returned as trait objects. This enforces dependency inversion at
//! the architectural level.

pub mod agent;
pub mod enrichment;
pub mod pipeline;
pub mod storage;
pub mod watch;

pub use agent::{
    create_agent, create_internal_agent, AgentInitParams, AgentType, InitializedAgent,
};
pub use enrichment::{create_default_enrichment_service, get_or_create_embedding_provider};
pub use pipeline::create_pipeline;
pub use storage::{create_daemon_storage, get_storage, StorageHandle};
#[cfg(feature = "storage-surrealdb")]
pub use storage::{
    create_surrealdb_enriched_note_store, create_surrealdb_storage, initialize_surrealdb_schema,
    shutdown_storage,
};
pub use watch::create_file_watcher;
