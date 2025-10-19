//! Crucible Daemon Library
//!
//! Terminal daemon for Crucible knowledge management system.

pub mod repl;
pub mod rune;
pub mod tools;
pub mod tui;

// Re-export main types for convenience
pub use repl::Repl;
