//! Factory functions for creating infrastructure implementations
//!
//! This module is the composition root where concrete types are assembled
//! and returned as trait objects.

pub mod agent;
pub mod embedding;
pub mod storage;

pub use agent::{create_agent, create_daemon_replay_agent, AgentInitParams, AgentType};
pub use embedding::embedding_provider_config_from_cli;
pub use storage::{get_storage, StorageHandle};
