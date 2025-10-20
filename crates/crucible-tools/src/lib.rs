//! Crucible Tools - Static system tools for knowledge management
//!
//! This crate provides static, system-level tools that offer core functionality
//! for the Crucible knowledge management system. It includes tools for:
//!
//! - **Vault operations**: Search, indexing, and metadata management
//! - **Database operations**: CRUD, semantic search, and maintenance
//! - **Search capabilities**: Advanced search with multiple criteria
//! - **Service integration**: Clean integration with the service layer
//!
//! # Architecture
//!
//! The crate is organized into several key modules:
//!
//! - [`system_tools`]: Core tool framework and base implementations
//! - [`vault_tools`]: Vault-specific operations and file management
//! - [`database_tools`]: Database interactions and semantic operations
//! - [`search_tools`]: Advanced search and indexing capabilities
//! - [`service`]: Service layer integration and tool execution
//! - [`registry`]: Static tool registration and discovery
//! - [`types`]: Tool-specific types and parameter structures
//!
//! # Quick Start
//!
//! ```rust
//! use crucible_tools::{ToolServiceFactory, ExecutionContextBuilder};
//!
//! // Create a tool service
//! let service = ToolServiceFactory::create_default();
//!
//! // Create execution context
//! let context = ExecutionContextBuilder::new()
//!     .vault_path("/path/to/vault")
//!     .user_id("user123")
//!     .build();
//!
//! // Execute a tool
//! let result = service.execute_tool(
//!     "search_by_properties",
//!     serde_json::json!({
//!         "properties": {
//!             "status": "active"
//!         }
//!     }),
//!     context
//! ).await?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Features
//!
//! - **Static Registration**: Tools are registered at startup for fast discovery
//! - **Service Integration**: Clean integration with the broader service architecture
//! - **Type Safety**: Strong typing with clear parameter schemas
//! - **Error Handling**: Comprehensive error handling and logging
//! - **Extensible**: Easy to add new tools and customize behavior
//!
//! # Configuration
//!
//! The crate supports flexible configuration through [`ToolServiceConfig`]:
//!
//! ```rust
//! use crucible_tools::{ToolServiceFactory, ToolServiceConfig};
//! use std::collections::HashMap;
//!
//! let mut config = ToolServiceConfig::default();
//! // Enable only specific categories
//! config.enabled_categories.clear();
//! config.enabled_categories.insert(ToolCategory::Search, true);
//!
//! let service = ToolServiceFactory::create_with_config(config);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(clippy::pedantic)]

pub mod database_tools;
pub mod registry;
pub mod search_tools;
pub mod service;
pub mod system_tools;
pub mod types;
pub mod vault_tools;

// Re-export commonly used types and functions
pub use service::{
    ConfigurableToolService, ExecutionContextBuilder, SystemToolService, ToolService,
    ToolServiceConfig, ToolServiceFactory,
};
pub use system_tools::Tool;
pub use system_tools::ToolManager;
pub use types::ToolDefinition;
pub use types::ToolExecutionContext;
pub use types::ToolExecutionResult;
pub use types::ToolCategory;
pub use registry::{initialize_registry, get_registry, create_tool_manager_from_registry, discovery};

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

/// Initialize with custom configuration
///
/// This function allows you to initialize the library with custom
/// tool registration hooks or additional tools.
///
/// # Example
///
/// ```rust
/// use crucible_tools::{init_with_config, system_tools::ToolManager};
///
/// // Register custom tools
/// let mut manager = ToolManager::new();
/// // ... register custom tools ...
///
/// // Initialize with custom tools
/// let registry = init_with_config(|registry| {
///     // Custom registration logic here
/// });
/// ```
pub fn init_with_config<F>(config_fn: F) -> std::sync::Arc<registry::ToolRegistry>
where
    F: FnOnce(&mut registry::ToolRegistry),
{
    tracing::info!("Initializing crucible-tools v{} with custom configuration", VERSION);
    let mut registry = registry::ToolRegistry::new();

    // Register built-in tools
    registry::register_built_in_tools(&mut registry);

    // Apply custom configuration
    config_fn(&mut registry);

    let registry = std::sync::Arc::new(registry);

    // Store in global registry
    // Note: This would need to be adapted for the OnceLock pattern
    tracing::info!(
        "Initialized {} tools across {} categories with custom configuration",
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
        description: "Static system tools for Crucible knowledge management".to_string(),
        features: vec![
            "vault_operations".to_string(),
            "database_operations".to_string(),
            "search_capabilities".to_string(),
            "service_integration".to_string(),
            "static_registration".to_string(),
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
mod tests;