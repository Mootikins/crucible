//! Zellij Inbox - Agent inbox for Zellij
//!
//! A standalone CLI + plugin for displaying agents waiting for user input.

pub mod file;
pub mod parse;
pub mod render;
pub mod tui;
pub mod types;

pub use types::*;
