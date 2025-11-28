//! Essential configuration components for Crucible
//!
//! Simple, focused configuration for the core components that actually need it.

pub mod acp;
pub mod chat;
pub mod cli;
pub mod embedding;

// Re-export essential component types
pub use acp::*;
pub use chat::*;
pub use cli::*;
pub use embedding::*;
