/// Rune-based tool system for Crucible MCP
///
/// This module provides support for defining MCP tools using the Rune scripting language.
/// Tools can be dynamically loaded, validated, and executed with hot-reload support.
///
/// Enhanced features:
/// - Flexible organization patterns (simple direct tools + module-based tools)
/// - Consumer awareness without restrictions
/// - Configurable naming conventions
/// - Backwards compatibility with existing tools
/// - AST-based module discovery for organized tools

mod ast_analyzer;
mod discovery;
mod registry;
mod stdlib;
mod tool;

pub use ast_analyzer::{RuneAstAnalyzer, DiscoveredModule, AsyncFunctionInfo, ParameterInfo};
pub use discovery::{ToolDiscovery, DiscoveredTool, DiscoveredTools, ConsumerInfo};
pub use registry::ToolRegistry;
pub use stdlib::build_crucible_module;
pub use tool::{RuneTool, ToolMetadata};
