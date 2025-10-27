//! Crucible CLI library
//!
//! This library provides the core functionality for the Crucible CLI,
//! exposing modules for configuration, interactive components, and output formatting.

pub mod agents;
pub mod cli;
pub mod commands;
pub mod common;
pub mod config;
pub mod error_recovery;
pub mod interactive;
pub mod output;
pub mod tui;
pub mod watcher;
