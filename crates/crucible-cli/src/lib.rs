//! Crucible CLI library
//!
//! This library provides the core functionality for the Crucible CLI,
//! exposing modules for configuration, commands, and output formatting.
//!
pub mod cli;
pub mod commands;
pub(crate) mod common;
pub mod config;

pub mod factories;
pub(crate) mod formatting;
pub(crate) mod kiln_attach;
pub(crate) mod kiln_discover;
pub(crate) mod kiln_validate;
pub mod output;
pub(crate) mod provider_detect;
pub mod session;
pub(crate) mod status_line;
#[cfg(test)]
pub(crate) mod test_daemon;
pub mod tui;
