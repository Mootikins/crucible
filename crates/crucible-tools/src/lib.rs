//! Crucible Tools - Simple async function composition for knowledge management
//!
//! This crate provides a collection of async function tools for the Crucible knowledge management system.
//! The architecture focuses on simple async function composition without complex enterprise patterns.
//!
//! ## Quick Start
//!
//! ### Using Individual Tool Functions
//!
//! ```rust
//! use crucible_tools::{search_tools, kiln_tools};
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Get tool functions directly
//!     let search_fn = search_tools::search_documents();
//!     let kiln_fn = kiln_tools::get_kiln_stats();
//!
//!     // Execute tools with the unified ToolFunction signature
//!     let search_result = search_fn(
//!         "search_documents".to_string(),
//!         json!({"query": "machine learning", "top_k": 10}),
//!         Some("user123".to_string()),
//!         Some("session456".to_string()),
//!     ).await?;
//!
//!     let kiln_stats = kiln_fn(
//!         "get_kiln_stats".to_string(),
//!         json!({}),
//!         Some("user123".to_string()),
//!         Some("session456".to_string()),
//!     ).await?;
//!
//!     println!("Search successful: {}", search_result.success);
//!     println!("Kiln has {} notes", kiln_stats.data.unwrap()["total_notes"]);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Using the Unified Tool Interface
//!
//! ```rust
//! use crucible_tools::{execute_tool, load_all_tools};
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load all tools into the registry
//!     load_all_tools().await?;
//!
//!     // Execute any tool by name
//!     let result = execute_tool(
//!         "system_info".to_string(),
//!         json!({}),
//!         Some("user123".to_string()),
//!         Some("session456".to_string()),
//!     ).await?;
//!
//!     if result.success {
//!         println!("Tool executed successfully: {:?}", result.data);
//!     } else {
//!         println!("Tool execution failed: {:?}", result.error);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Available Tools
//!
//! The library provides 25 tools across 4 categories:
//!
//! - **System Tools** (5): `system_info`, `execute_command`, `list_files`, `read_file`, `get_environment`
//! - **Kiln Tools** (8): `search_by_properties`, `search_by_tags`, `search_by_folder`, `create_note`, `update_note`, `delete_note`, `get_kiln_stats`, `list_tags`
//! - **Database Tools** (7): `semantic_search`, `search_by_content`, `search_by_filename`, `update_note_properties`, `index_document`, `get_document_stats`, `sync_metadata`
//! - **Search Tools** (5): `search_documents`, `rebuild_index`, `get_index_stats`, `optimize_index`, `advanced_search`

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(clippy::pedantic)]

pub mod database_tools;
pub mod kiln_tools;
pub mod search_tools;
pub mod system_tools;
pub mod types;

// Kiln parsing modules - Phase 1A TDD Implementation
pub mod kiln_change_detection;
pub mod kiln_parser;
pub mod kiln_scanner;
pub mod kiln_types;

// Real kiln operations - Phase 1B Implementation
pub mod kiln_operations;

// ===== PUBLIC API EXPORTS =====
// Simple async function composition interface

// Core types for tool composition
pub use types::{
    ToolDefinition, ToolError, ToolExecutionContext, ToolExecutionRequest, ToolFunction, ToolResult,
};

// Unified tool interface
pub use types::{
    execute_tool, initialize_tool_registry, list_registered_tools, register_tool_function,
};

// Tool loading utilities
pub use types::{load_all_tools, tool_loader_info, ToolLoaderInfo};

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the crucible-tools library
///
/// This function initializes the simplified tool registry. Tool loading is handled
/// asynchronously by the `load_all_tools()` function when needed.
///
/// # Example
///
/// ```rust
/// use crucible_tools::init;
///
/// // Initialize the library
/// init();
/// ```
pub fn init() {
    tracing::info!("Initializing crucible-tools v{}", VERSION);
    tracing::info!("Simple async function composition interface ready");
    tracing::info!("Tools will be loaded on-demand via load_all_tools()");
}

/// Get library information
///
/// Returns information about the library version and available features.
pub fn library_info() -> LibraryInfo {
    LibraryInfo {
        version: VERSION.to_string(),
        name: "crucible-tools".to_string(),
        description: "Simple async function composition for Crucible knowledge management"
            .to_string(),
        features: vec![
            "simple_composition".to_string(),
            "direct_async_functions".to_string(),
            "database_tools".to_string(),
            "search_tools".to_string(),
            "kiln_tools".to_string(),
            "system_tools".to_string(),
            "unified_interface".to_string(),
            "25_tools_registered".to_string(),
            "direct_tool_registration".to_string(),
        ],
    }
}

/// Library information structure
#[derive(Debug, Clone)]
pub struct LibraryInfo {
    /// Library version
    pub version: String,
    /// Library name
    pub name: String,
    /// Library description
    pub description: String,
    /// Available features
    pub features: Vec<String>,
}

// Include comprehensive tests
#[cfg(test)]
pub mod tests;
