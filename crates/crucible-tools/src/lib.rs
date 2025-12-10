//! Crucible Tools - MCP-compatible tools for knowledge management
//!
//! This crate provides 10 focused tools for the Crucible knowledge management system,
//! designed following SOLID principles and MCP (Model Context Protocol) compatibility.
//!
//! ## Tool Categories
//!
//! - **NoteTools** (6): create_note, read_note, read_metadata, update_note, delete_note, list_notes
//! - **SearchTools** (3): semantic_search, text_search, property_search
//! - **KilnTools** (3): get_kiln_info, get_kiln_roots, get_kiln_stats
//! - **CrucibleMcpServer** (12): Unified MCP server exposing all tools via stdio transport
//!
//! ## Architecture
//!
//! Each tool category is implemented as a separate struct with async methods decorated
//! with `#[tool]` attributes for MCP compatibility. Tools operate directly on the
//! filesystem for maximum simplicity and transparency.
//!
//! ## Example
//!
//! ```rust
//! use crucible_tools::{NoteTools, SearchTools, KilnTools};
//!
//! // Initialize tool modules
//! let note_tools = NoteTools::new("/path/to/kiln".to_string());
//! let kiln_tools = KilnTools::new("/path/to/kiln".to_string());
//!
//! // Use tools (requires async runtime)
//! // let result = note_tools.create_note(...).await;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(clippy::pedantic)]

pub mod clustering;
pub mod config_bridge;
pub mod extended_mcp_server;
pub mod kiln;
pub mod mcp_server;
pub mod notes;
pub mod output_filter;
pub mod search;
pub mod toon_response;
pub mod utils;

// ===== PUBLIC API EXPORTS =====

pub use clustering::ClusteringTools;
pub use config_bridge::{
    create_discovery_paths, create_event_handler_config, create_rune_discovery_config,
    to_rune_discovery_config,
};
pub use extended_mcp_server::{ExtendedMcpServer, ExtendedMcpService};
pub use kiln::KilnTools;
pub use mcp_server::CrucibleMcpServer;
pub use notes::NoteTools;
pub use search::SearchTools;

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the crucible-tools library
///
/// This function logs initialization information about the tool system.
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
    tracing::info!("12 tools available: 6 NoteTools, 3 SearchTools, 3 KilnTools");
}

/// Get library information
///
/// Returns information about the library version and available features.
#[must_use]
pub fn library_info() -> LibraryInfo {
    LibraryInfo {
        version: VERSION.to_string(),
        name: "crucible-tools".to_string(),
        description: "MCP-compatible tools for Crucible knowledge management".to_string(),
        features: vec![
            "mcp_compatible".to_string(),
            "note_tools".to_string(),
            "search_tools".to_string(),
            "kiln_tools".to_string(),
            "filesystem_based".to_string(),
            "10_focused_tools".to_string(),
            "solid_principles".to_string(),
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
