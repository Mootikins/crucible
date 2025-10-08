//! Obsidian HTTP client module
//!
//! This module provides an HTTP client for interacting with the Obsidian plugin API.
//! The API is served by the MCP Obsidian plugin running on localhost:27123 (default port).

mod client;
mod config;
mod error;
mod types;

pub use client::ObsidianClient;
pub use config::{ClientConfig, ClientConfigBuilder, RetryConfig};
pub use error::{ObsidianError, Result};
pub use types::*;
