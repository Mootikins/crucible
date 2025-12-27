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
//! │  function search(query: string,             │
//! │                  limit: number?): {Result}  │
//! └─────────────────────────────────────────────┘
//!             │
//!             ▼
//! ┌─────────────────────────────────────────────┐
//! │  full_moon (parse → AST)                    │
//! │  Extract: param types → JSON Schema         │
//! └─────────────────────────────────────────────┘
//!             │
//!             ▼
//! ┌─────────────────────────────────────────────┐
//! │  mlua/Luau (execute at runtime)             │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_lua::{LuaExecutor, LuaToolRegistry, schema};
//!
//! // Extract schema from typed Luau source
//! let source = r#"
//!     function search(query: string, limit: number?): {SearchResult}
//!         return kb_search(query, limit or 10)
//!     end
//! "#;
//! let signatures = schema::extract_signatures(source)?;
//!
//! // Execute with mlua/Luau runtime
//! let executor = LuaExecutor::new()?;
//! let result = executor.execute_source(source, false, args).await?;
//! ```
//!
//! ## Feature Flags
//!
//! - `fennel` (default): Bundle the Fennel compiler (~160KB)
//! - `send`: Enable `Send+Sync` on Lua state for multi-threaded use

mod error;
mod executor;
#[cfg(feature = "fennel")]
mod fennel;
mod registry;
pub mod schema;
mod types;

pub use error::LuaError;
pub use executor::LuaExecutor;
#[cfg(feature = "fennel")]
pub use fennel::FennelCompiler;
pub use registry::LuaToolRegistry;
pub use schema::{extract_signatures, generate_input_schema, type_to_string, FunctionSignature, LuauType};
pub use types::{LuaExecutionResult, LuaTool, ToolParam, ToolResult};
