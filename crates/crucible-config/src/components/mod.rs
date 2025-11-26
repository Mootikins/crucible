//! Essential configuration components for Crucible
//!
//! Simple, focused configuration for the core components that actually need it.

pub mod cli;
pub mod embedding;
pub mod acp;
pub mod chat;

// Re-export essential component types
pub use cli::*;
pub use embedding::*;
pub use acp::*;
pub use chat::*;