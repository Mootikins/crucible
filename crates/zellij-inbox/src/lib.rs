//! Zellij Inbox - Agent inbox for Zellij
//!
//! A standalone CLI + plugin for displaying agents waiting for user input.

pub mod file;
pub mod parse;
pub mod render;
pub mod types;

#[cfg(target_arch = "wasm32")]
mod plugin;

pub use types::*;
