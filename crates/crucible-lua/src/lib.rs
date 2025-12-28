//! Luau scripting integration for Crucible
//!
//! This crate provides Luau (Lua with gradual types) scripting alongside Rune:
//! - **LLM-friendly**: Simple syntax, massive training data
//! - **Type-driven schemas**: Extract tool schemas from Luau type annotations
//! - **Threading**: `send` feature enables Send+Sync
//! - **Fennel**: Optional Lisp syntax with macros (compiles to Lua)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  tool.lua (with Luau type annotations)      │
//! │                                             │
//! │  --- Search the knowledge base              │
//! │  -- @tool                                   │
//! │  -- @param query string Search query        │
//! │  function search(query, limit)              │
//! └─────────────────────────────────────────────┘
//!             │
//!             ├──────────────────────────────────┐
//!             ▼                                  ▼
//! ┌─────────────────────────┐   ┌─────────────────────────┐
//! │  Annotations Parser     │   │  full_moon (AST)        │
//! │  LDoc-style comments    │   │  Luau type annotations  │
//! └─────────────────────────┘   └─────────────────────────┘
//!             │                              │
//!             └──────────────┬───────────────┘
//!                            ▼
//!             ┌─────────────────────────────────┐
//!             │  Tool/Hook/Plugin Registry      │
//!             │  JSON Schema generation         │
//!             └─────────────────────────────────┘
//!                            │
//!                            ▼
//!             ┌─────────────────────────────────┐
//!             │  mlua/Luau Runtime              │
//!             │  + data, shell, json modules    │
//!             └─────────────────────────────────┘
//! ```
//!
//! ## Annotation Format
//!
//! Tools, hooks, and plugins are discovered via LDoc-style annotations:
//!
//! ```lua
//! --- Search the knowledge base
//! -- @tool desc="Search for notes"
//! -- @param query string The search query
//! -- @param limit number? Maximum results (optional)
//! function search(query, limit)
//!     return crucible.search(query, limit or 10)
//! end
//!
//! --- Filter tool results
//! -- @hook event="tool:after" pattern="search_*" priority=50
//! function filter_results(ctx, event)
//!     return event
//! end
//! ```
//!
//! ## Feature Flags
//!
//! - `fennel` (default): Bundle the Fennel compiler (~160KB)
//! - `send`: Enable `Send+Sync` on Lua state for multi-threaded use

pub mod annotations;
mod error;
mod executor;
#[cfg(feature = "fennel")]
mod fennel;
mod graph;
mod json_query;
mod registry;
pub mod schema;
mod shell;
mod types;

pub use annotations::{AnnotationParser, DiscoveredHook, DiscoveredPlugin, DiscoveredTool};
pub use error::LuaError;
pub use executor::LuaExecutor;
#[cfg(feature = "fennel")]
pub use fennel::FennelCompiler;
pub use graph::{register_graph_module, register_graph_module_with_executor};
pub use json_query::{
    detect_format, encode_to_format, json_to_lua, lua_to_json, parse_auto, parse_with_format,
    register_oq_module, Format,
};
pub use registry::LuaToolRegistry;
pub use schema::{
    extract_signatures, generate_input_schema, type_to_string, FunctionSignature, LuauType,
};
pub use shell::{register_shell_module, ExecResult, ShellPolicy};
pub use types::{LuaExecutionResult, LuaTool, ToolParam, ToolResult};
