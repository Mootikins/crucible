//! Crucible CLI library
//!
//! This library provides the core functionality for the Crucible CLI,
//! exposing modules for configuration, interactive components, and output formatting.
//!
pub mod chat;
pub mod cli;
pub mod commands;
pub mod common;
pub mod config;
pub mod context_enricher;
pub mod core_facade;

pub mod factories;
pub mod formatting;
pub mod interactive;
pub mod kiln_discover;
pub mod kiln_validate;
pub mod output;
pub mod progress;
pub mod provider_detect;
pub mod search;
pub mod session_logger;
pub mod sync;
#[allow(dead_code, unused_imports)] // WIP TUI code — clean up as features stabilize
pub mod tui;
