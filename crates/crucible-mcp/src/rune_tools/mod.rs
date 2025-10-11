/// Rune-based tool system for Crucible MCP
///
/// This module provides support for defining MCP tools using the Rune scripting language.
/// Tools can be dynamically loaded, validated, and executed with hot-reload support.

mod registry;
mod tool;

pub use registry::ToolRegistry;
pub use tool::{RuneTool, ToolMetadata};
