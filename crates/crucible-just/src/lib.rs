//! Parse justfiles and generate MCP tool definitions
//!
//! This crate parses the JSON output from `just --dump --dump-format json`
//! and converts recipes into MCP-compatible tool definitions.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_just::JustTools;
//! use std::env;
//!
//! let tools = JustTools::new(env::current_dir().unwrap());
//! let mcp_tools = tools.list_tools().await?;
//! ```

mod error;
mod executor;
mod loader;
mod mcp;
mod tools;
mod types;

pub use error::*;
pub use executor::*;
pub use loader::*;
pub use mcp::*;
pub use tools::*;
pub use types::*;
