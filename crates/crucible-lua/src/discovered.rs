//! The shapes a plugin's exports take once discovered.
//!
//! `DiscoveredTool`, `DiscoveredHandler`, `DiscoveredCommand` and
//! `DiscoveredService` are what a plugin's **spec table** is parsed into
//! (`lifecycle/spec.rs`).
//!
//! This module used to also hold `AnnotationParser`, which scraped LDoc-style
//! `-- @tool` / `-- @handler` doc comments out of `.lua` and `.fnl` files as a
//! second way to declare the same things. It is gone: `parse_tools`,
//! `parse_commands` and `parse_views` had no callers on any live path, and the
//! one route that did reach `parse_handlers` — a per-session scan of a kiln's
//! `handlers/` directory — was a weaker duplicate of what a plugin already does
//! with `crucible.on`. A plugin is the single import mechanism; these are the
//! shapes its declarations land in.

use crate::types::{LuaTool, ToolParam};

/// Discovered tool from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredTool {
    pub name: String,
    pub description: String,
    pub params: Vec<DiscoveredParam>,
    pub return_type: Option<String>,
    pub source_path: String,
    pub is_fennel: bool,
}

/// Discovered parameter from annotations
#[derive(Debug, Clone)]
pub struct DiscoveredParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub optional: bool,
}

/// Discovered handler from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredHandler {
    pub name: String,
    pub event_type: String,
    pub pattern: String,
    pub priority: i64,
    pub description: String,
    pub source_path: String,
    pub handler_fn: String,
    pub is_fennel: bool,
}

/// Discovered slash command from Lua/Fennel source
#[derive(Debug, Clone)]
pub struct DiscoveredCommand {
    /// Command name (without leading /)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameters the command accepts
    pub params: Vec<DiscoveredParam>,
    /// Hint shown after command name in UI
    pub input_hint: Option<String>,
    /// Path to source file containing the handler
    pub source_path: String,
    /// Name of the handler function in the source
    pub handler_fn: String,
    /// Whether this is a Fennel source
    pub is_fennel: bool,
}

/// Discovered long-running service from plugin spec table.
///
/// Services are background tasks that run for the lifetime of the plugin.
/// They are spawned after `setup()` completes and stopped on plugin unload
/// or daemon shutdown.
#[derive(Debug, Clone)]
pub struct DiscoveredService {
    /// Service name (must be unique within plugin)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Source path of the plugin defining this service
    pub source_path: String,
    /// Name of the service function in the Lua spec table
    pub service_fn: String,
}

impl From<DiscoveredTool> for LuaTool {
    fn from(tool: DiscoveredTool) -> Self {
        LuaTool {
            name: tool.name,
            description: tool.description,
            params: tool
                .params
                .into_iter()
                .map(|p| ToolParam {
                    name: p.name,
                    param_type: p.param_type,
                    description: p.description,
                    required: !p.optional,
                    default: None,
                })
                .collect(),
            source_path: tool.source_path,
            is_fennel: tool.is_fennel,
        }
    }
}
