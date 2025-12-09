//! Rune scripting integration for Crucible tools
//!
//! This crate provides dynamic tool discovery and execution using the Rune
//! scripting language. Tools are discovered from configured directories
//! (global `~/.crucible/runes/` and kiln-specific `{kiln}/runes/`).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::{RuneToolRegistry, RuneDiscoveryConfig};
//!
//! // Create config with default directories
//! let config = RuneDiscoveryConfig::with_defaults(Some(kiln_path));
//!
//! // Discover and register tools
//! let registry = RuneToolRegistry::discover_from(config).await?;
//!
//! // List available tools
//! for tool in registry.list_tools().await {
//!     println!("{}: {}", tool.name, tool.description);
//! }
//!
//! // Execute a tool
//! let result = registry.execute("rune_my_tool", args).await?;
//! ```

mod discovery;
mod event_handler;
mod event_pipeline;
mod events;
mod executor;
mod plugin_loader;
mod plugin_types;
mod regex_module;
mod registry;
mod rune_types;
mod types;

pub use discovery::ToolDiscovery;
pub use event_handler::{EventHandler, EventHandlerConfig};
pub use event_pipeline::EventPipeline;
pub use events::{
    ContentBlock, CrucibleEvent, EnrichedRecipe, RecipeEnrichment, RecipeParameter,
    ToolResultEvent,
};
pub use executor::RuneExecutor;
pub use plugin_loader::PluginLoader;
pub use plugin_types::{HookConfig, PluginManifest, RegisteredHook};
pub use regex_module::regex_module;
pub use registry::RuneToolRegistry;
pub use rune_types::crucible_module;
pub use types::{RuneDiscoveryConfig, RuneExecutionResult, RuneTool};

use thiserror::Error;

/// Errors that can occur in the Rune tool system
#[derive(Error, Debug)]
pub enum RuneError {
    /// Tool not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Discovery error
    #[error("Discovery error: {0}")]
    Discovery(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// Rune context error
    #[error("Context error: {0}")]
    Context(String),

    /// Compilation error
    #[error("Compile error: {0}")]
    Compile(String),

    /// Execution error
    #[error("Execution error: {0}")]
    Execution(String),

    /// Value conversion error
    #[error("Conversion error: {0}")]
    Conversion(String),
}

// Allow conversion from rune ContextError
impl From<rune::ContextError> for RuneError {
    fn from(e: rune::ContextError) -> Self {
        RuneError::Context(e.to_string())
    }
}
