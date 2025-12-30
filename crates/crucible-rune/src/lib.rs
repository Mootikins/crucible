//! Rune scripting integration for Crucible plugins
//!
//! This crate provides dynamic plugin discovery and execution using the Rune
//! scripting language. Plugins are discovered from configured directories:
//!
//! - Global personal: `~/.config/crucible/plugins/`
//! - Kiln personal: `KILN/.crucible/plugins/` (gitignored)
//! - Kiln shared: `KILN/plugins/` (version-controlled)
//!
//! Plugins can provide tools (`#[tool(...)]`) and hooks (`#[hook(...)]`).
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

mod attribute_discovery;
pub mod builtin_hooks;
pub mod compaction;
pub mod core_handler;
pub mod dependency_graph;
mod discovery;
mod discovery_paths;
pub mod event_bus;
mod event_handler;
pub mod event_markdown;
mod event_pipeline;
mod event_ring;
mod events;
mod executor;
mod graph_module;
pub mod handler;
pub mod handler_chain;
pub mod handler_wiring;
mod hook_system;
mod hook_types;
pub mod linear_reactor;
pub mod logging_handler;
pub mod mcp_gateway;
pub mod mcp_module;
pub mod mcp_types;
pub mod note_events;
mod oq_module;
pub mod persistence_handler;
mod plugin_loader;
mod plugin_types;
pub mod reactor;
mod regex_module;
mod registry;
mod rune_types;
pub mod session;
mod shell_module;
mod struct_plugin;
pub mod tool_events;
mod types;

pub use attribute_discovery::{attr_parsers, AttributeDiscovery, FromAttributes};
pub use builtin_hooks::{
    create_event_emit_hook, create_recipe_enrichment_hook, create_test_filter_hook,
    create_tool_selector_hook, create_toon_transform_hook, register_builtin_hooks,
    BuiltinHooksConfig, EventEmitConfig, HookToggle, ToolSelectorConfig,
};
pub use compaction::{CompactionConfig, CompactionMetrics, CompactionReason, CompactionTrigger};
pub use core_handler::{RuneHandler, RuneHandlerMeta};
pub use dependency_graph::{
    DependencyError, DependencyGraph, DependencyResult, GraphNode, HandlerGraph,
};
pub use discovery::ToolDiscovery;
pub use discovery_paths::{DiscoveryConfig, DiscoveryPaths};
pub use event_bus::{
    Event, EventBus, EventContext, EventType, Handler, HandlerError, HandlerResult,
};
pub use event_handler::{EventHandler, EventHandlerConfig};
pub use event_markdown::{
    EventToMarkdown, MarkdownParseError, MarkdownParseResult, MarkdownToEvent,
};
pub use event_pipeline::EventPipeline;
pub use event_ring::{EventRing, OverflowCallback};
pub use events::{
    ContentBlock, CrucibleEvent, EnrichedRecipe, RecipeEnrichment, RecipeParameter, ToolResultEvent,
};
pub use executor::RuneExecutor;
pub use graph_module::{graph_module, graph_module_with_executor};
pub use handler::{
    BoxedRingHandler, RingHandler, RingHandlerContext, RingHandlerError, RingHandlerInfo,
    RingHandlerResult,
};
pub use handler_chain::{ChainResult, HandlerChain};
pub use handler_wiring::{
    wire_event_bus, wire_event_bus_default, EventBusRingHandler, HandlerWiringBuilder,
};
pub use hook_system::{BuiltinHook, Hook, HookManager, HookRegistry, RuneHookHandler};
pub use hook_types::RuneHook;
pub use linear_reactor::{LinearReactor, LinearReactorConfig};
pub use logging_handler::{EventFilter, LogLevel, LoggingConfig, LoggingHandler};
pub use mcp_gateway::{
    BoxedMcpExecutor, ContentBlock as GatewayContentBlock, GatewayError, McpGatewayManager,
    ToolCallResult, TransportConfig, UpstreamConfig, UpstreamMcpClient, UpstreamServerInfo,
    UpstreamTool,
};
pub use mcp_module::{
    extract_param_names, extract_param_types, generate_mcp_server_module, McpToolCaller,
};
pub use mcp_types::{build_args_json, json_to_rune, mcp_types_module, rune_to_json, McpResult};
pub use note_events::{
    BlockChange, BlockChangeOperation, BlockInfo, BlockType, InlineLinkInfo, NoteChangeType,
    NoteCreatedPayload, NoteEventEmitter, NoteMetadata, NoteModifiedPayload, NotePayload,
    WikilinkInfo,
};
pub use oq_module::oq_module;
pub use persistence_handler::PersistenceHandler;
pub use plugin_loader::PluginLoader;
pub use plugin_types::{HookConfig, PluginManifest, RegisteredHook};
pub use reactor::{
    BoxedReactor, Reactor, ReactorContext, ReactorError, ReactorMetadata, ReactorResult,
    ReactorSessionConfig, SessionEvent, ToolCall,
};
pub use regex_module::regex_module;
pub use registry::RuneToolRegistry;
pub use rune_types::{
    categorize_by_name_impl, crucible_module, session_event_module, RuneEventContext,
    RuneFileChangeKind, RuneNoteChangeType, RuneRecipe, RuneRecipeEnrichment, RuneSessionEvent,
};
pub use session::{Session, SessionBuilder, SessionHandle, SessionState};
pub use shell_module::{shell_module, shell_module_with_policy};
pub use struct_plugin::{
    PluginMetadata, StructPluginHandle, ToolDefinition, ToolParameter, WatchEvent, WatchEventKind,
};
pub use tool_events::{ContentBlock as ToolContentBlock, ToolEventEmitter, ToolSource};
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
