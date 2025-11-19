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
//! // Examples have been removed as they are being updated to the new API.
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
pub mod permission;
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
pub use types::ToolRegistry;

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
#[must_use]
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

