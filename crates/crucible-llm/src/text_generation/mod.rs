//! Text generation module

pub mod factory;
pub mod types;

// Re-export all types
pub use types::*;

// Re-export factory functions
pub use factory::{from_app_config, from_chat_config};
