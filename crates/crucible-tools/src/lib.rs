//! Crucible Tools - System and Rune tools for knowledge management
//!
//! This crate provides system-level and dynamic Rune scripting tools that offer
//! core functionality for the Crucible knowledge management system. It includes tools for:
//!
//! - **Vault operations**: Search, indexing, and metadata management
//! - **Database operations**: CRUD, semantic search, and maintenance
//! - **Search capabilities**: Advanced search with multiple criteria
//! - **Rune scripting**: Dynamic tool execution with the Rune scripting language
//! - **Tool discovery**: Automatic discovery and loading of Rune tools
//!
//! # Architecture
//!
//! The crate is organized into several key modules:
//!
//! - [`system_tools`]: Core tool framework and base implementations
//! - [`vault_tools`]: Vault-specific operations and file management
//! - [`database_tools`]: Database interactions and semantic operations
//! - [`search_tools`]: Advanced search and indexing capabilities
//! - [`registry`]: Static tool registration and discovery
//! - [`types`]: Tool-specific types and parameter structures
//! - [`analyzer`]: Rune AST analysis and tool metadata extraction
//! - [`context`]: Rune execution context and security management
//! - [`discovery`]: Automatic discovery and loading of Rune tools
//! - [`handler`]: Rune tool execution handlers
//! - [`loader`]: Dynamic loading and compilation of Rune scripts
//! - [`tool`]: Rune tool implementations and execution engine
//! - [`rune_registry`]: Rune tool registration and management
//! - [`stdlib`]: Standard library functions for Rune tools
//! - [`embeddings`]: Embedding generation and vector operations
//! - [`utils`]: Utility functions for tool operations
//! - [`rune_service`]: Simple Rune service interface
//!
//! # Quick Start
//!
//! ## Using Rune Tools
//!
//! ```rust
//! use crucible_tools::{RuneService, RuneServiceConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = RuneServiceConfig::default();
//!     let service = RuneService::new(config).await?;
//!
//!     // Discover tools from a directory
//!     service.discover_tools_from_directory("./tools").await?;
//!
//!     // List available tools
//!     let tools = service.list_tools().await?;
//!     println!("Found {} tools", tools.len());
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Using System Tools
//!
//! ```rust
//! use crucible_tools::{init, create_tool_manager};
//!
//! // Initialize the tool registry
//! let registry = init();
//!
//! // Create a tool manager
//! let manager = create_tool_manager();
//! let tools = manager.list_tools();
//! println!("Available tools: {}", tools.len());
//! ```
//!
//! # Features
//!
//! - **Rune Scripting**: Dynamic tool execution with the Rune scripting language
//! - **Tool Discovery**: Automatic discovery and loading of Rune tools
//! - **Static Tools**: Built-in system tools for common operations
//! - **Tool Macros**: `#[tool]` attribute macro for easy tool definition
//! - **Type Safety**: Strong typing with clear parameter schemas
//! - **Error Handling**: Comprehensive error handling and logging

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(clippy::pedantic)]

pub mod database_tools;
pub mod search_tools;
pub mod system_tools;
pub mod types;
pub mod vault_tools;

// Rune modules
pub mod analyzer;
pub mod context;
pub mod context_factory;
pub mod database;
pub mod discovery;
pub mod errors;
pub mod handler;
pub mod loader;
pub mod registry;
pub mod stdlib;
pub mod tool;
pub mod rune_registry;
pub mod rune_service;
pub mod utils;
pub mod embeddings;
pub mod validation;

// Phase 5.1 Migration modules
pub mod migration_bridge;
pub mod migration_manager;

// Re-export commonly used types and functions
pub use system_tools::Tool;
pub use system_tools::ToolManager;
pub use rune_service::RuneService;
pub use context_factory::ContextFactory;
pub use types::{RuneServiceConfig, ValidationResult, SystemInfo, ToolDefinition, ToolExecutionRequest, ToolExecutionResult, ToolExecutionContext, ContextRef, ServiceError, ServiceResult, ServiceHealth, ServiceMetrics, ServiceStatus, ToolService};

// Re-export migration types
pub use migration_bridge::{ToolMigrationBridge, MigrationConfig, MigratedTool, MigrationStats, MigrationValidation};
pub use migration_manager::{Phase51MigrationManager, MigrationManagerConfig, MigrationState, MigrationPhase, MigrationReport, MigrationError, MigrationErrorType, MigrationMode, ValidationMode};

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize the crucible-tools library
///
/// This function initializes the global tool registry and prepares
/// all built-in tools for use. It should be called once during
/// application startup.
///
/// # Example
///
/// ```rust
/// use crucible_tools::init;
///
/// // Initialize the library
/// let registry = init();
///
/// // Now tools can be discovered and used
/// let tool_names = registry.list_tools();
/// println!("Available tools: {:?}", tool_names);
/// ```
pub fn init() -> std::sync::Arc<registry::ToolRegistry> {
    tracing::info!("Initializing crucible-tools v{}", VERSION);
    let registry = registry::initialize_registry();
    tracing::info!(
        "Initialized {} tools across {} categories",
        registry.tools.len(),
        registry.categories.len()
    );
    registry
}

/// Create a pre-configured tool manager
///
/// This is a convenience function that creates a tool manager
/// with all built-in tools already registered.
///
/// # Example
///
/// ```rust
/// use crucible_tools::create_tool_manager;
///
/// let manager = create_tool_manager();
/// let tools = manager.list_tools();
/// println!("Available tools: {}", tools.len());
/// ```
pub fn create_tool_manager() -> system_tools::ToolManager {
    // Ensure the registry is initialized
    init();
    registry::create_tool_manager_from_registry()
}

/// Get library information
///
/// Returns information about the library version and configuration.
pub fn library_info() -> LibraryInfo {
    LibraryInfo {
        version: VERSION.to_string(),
        name: "crucible-tools".to_string(),
        description: "System and Rune tools for Crucible knowledge management".to_string(),
        features: vec![
            "vault_operations".to_string(),
            "database_operations".to_string(),
            "search_capabilities".to_string(),
            "rune_scripting".to_string(),
            "tool_discovery".to_string(),
            "tool_macros".to_string(),
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