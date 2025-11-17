//! Crucible CLI library
//!
//! This library provides the core functionality for the Crucible CLI,
//! exposing modules for configuration, interactive components, and output formatting.

pub mod acp;
pub mod cli;
pub mod commands;
pub mod common;
pub mod config;
pub mod core_facade;
pub mod interactive;
pub mod output;
pub mod tui;
// Streamlined for Phase 5: disabled agents, error_recovery, watcher (heavy dependencies)
