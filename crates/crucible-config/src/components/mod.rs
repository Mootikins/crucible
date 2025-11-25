//! Component-based configuration modules
//!
//! This module provides fine-grained configuration components for different
//! aspects of the Crucible system. Each component has its own configuration
//! struct with intelligent defaults based on system capabilities.

pub mod cli;
pub mod embedding;
pub mod storage;
pub mod processing;
pub mod networking;
pub mod services;
pub mod monitoring;
pub mod acp;
pub mod chat;

// Re-export all component types for convenience
pub use cli::*;
pub use embedding::*;
pub use storage::*;
pub use processing::*;
pub use networking::*;
pub use services::*;
pub use monitoring::*;
pub use acp::*;
pub use chat::*;