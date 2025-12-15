//! Crucible CLI library
//!
//! This library provides the core functionality for the Crucible CLI,
//! exposing modules for configuration, interactive components, and output formatting.
//!
//! NOTE: TUI modules (chat_tui, tui) removed during event architecture cleanup.
//! Interactive chat now stubbed pending event bus integration.

pub mod acp;
pub mod chat;
pub mod cli;
pub mod commands;
pub mod common;
pub mod config;
pub mod core_facade;
pub mod factories;
pub mod formatting;
pub mod interactive;
pub mod output;
pub mod progress;
pub mod sync;
