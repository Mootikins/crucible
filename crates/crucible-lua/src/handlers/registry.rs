use crate::annotations::AnnotationParser;
use crate::error::LuaError;
use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::{Function, Lua, RegistryKey, Result as LuaResult, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};
use walkdir::WalkDir;

use super::conversion::session_event_to_lua;
use super::script_handler::{interpret_handler_result, LuaScriptHandler, ScriptHandlerResult};

/// Registry of discovered Lua handlers
///
/// Manages a collection of `LuaScriptHandler` instances discovered from Lua/Fennel
/// source files. Provides event matching and priority-ordered dispatch.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::LuaScriptHandlerRegistry;
/// use std::path::PathBuf;
///
/// // Discover handlers from directories
/// let paths = vec![PathBuf::from("./handlers")];
/// let registry = LuaScriptHandlerRegistry::discover(&paths)?;
///
/// // Check what handlers are available
/// println!("Found {} handlers", registry.len());
///
/// // Get handlers matching an event
/// for handler in registry.handlers_for(&event) {
///     let result = handler.execute(&lua, &event)?;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LuaScriptHandlerRegistry {
    pub(super) handlers: Vec<LuaScriptHandler>,
    /// Runtime-registered handlers (via crucible.on())
    pub(super) runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
    /// Stored Lua function references (handler_name -> RegistryKey)
    pub(super) handler_functions: Arc<Mutex<HashMap<String, RegistryKey>>>,
}

/// A handler registered at runtime via crucible.on()
#[derive(Debug, Clone)]
pub struct RuntimeHandler {
    /// Event type to match
    pub event_type: String,
    /// Handler function name (for debugging)
    pub name: String,
    /// Priority (lower = earlier)
    pub priority: i64,
    /// Optional glob pattern to filter events (e.g., tool name for pre_tool_call)
    pub pattern: Option<String>,
}

impl LuaScriptHandlerRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            runtime_handlers: Arc::new(Mutex::new(Vec::new())),
            handler_functions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Discover handlers from the given paths
    ///
    /// Walks each path recursively, parsing `.lua` and `.fnl` files for handler
    /// annotations. Discovered handlers are sorted by priority (lowest first).
    ///
    /// # Arguments
    ///
    /// * `paths` - Directories or files to scan for handlers
    ///
    /// # Errors
    ///
    /// Returns an error if file reading fails. Missing paths are silently skipped.
    pub fn discover(paths: &[PathBuf]) -> Result<Self, std::io::Error> {
        let parser = AnnotationParser::new();
        let mut handlers = Vec::new();

        for path in paths {
            if !path.exists() {
                debug!("Handler discovery path does not exist: {:?}", path);
                continue;
            }

            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "lua" || ext == "fnl")
                })
            {
                let entry_path = entry.path();
                let source = match std::fs::read_to_string(entry_path) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Failed to read handler source {:?}: {}", entry_path, e);
                        continue;
                    }
                };

                match parser.parse_handlers(&source, entry_path) {
                    Ok(hooks) => {
                        for hook in hooks {
                            // Use with_source to avoid re-reading the file
                            let handler = LuaScriptHandler::with_source(hook, source.clone());
                            debug!(
                                "Discovered handler: {} (event={}, priority={})",
                                handler.metadata.name,
                                handler.metadata.event_type,
                                handler.metadata.priority
                            );
                            handlers.push(handler);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse handlers from {:?}: {}", entry_path, e);
                    }
                }
            }
        }

        // Sort by priority (lower priority values execute first)
        handlers.sort_by_key(|h| h.metadata.priority);

        debug!("Handler registry discovered {} handlers", handlers.len());
        Ok(Self {
            handlers,
            runtime_handlers: Arc::new(Mutex::new(Vec::new())),
            handler_functions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Get all handlers that match the given event
    ///
    /// Returns handlers in priority order (lowest priority value first).
    pub fn handlers_for(&self, event: &SessionEvent) -> Vec<&LuaScriptHandler> {
        self.handlers.iter().filter(|h| h.matches(event)).collect()
    }

    /// Get all handlers matching event type and identifier
    ///
    /// More flexible matching for cases where the event type and identifier
    /// are known separately (e.g., tool name for tool events).
    pub fn handlers_for_identifier(
        &self,
        event_type: &str,
        identifier: &str,
    ) -> Vec<&LuaScriptHandler> {
        self.handlers
            .iter()
            .filter(|h| h.matches_with_identifier(event_type, identifier))
            .collect()
    }

    /// Number of registered handlers
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Add a handler manually
    ///
    /// The handler is inserted in priority order.
    pub fn add(&mut self, handler: LuaScriptHandler) {
        self.handlers.push(handler);
        self.handlers.sort_by_key(|h| h.metadata.priority);
    }

    /// Remove all handlers
    pub fn clear(&mut self) {
        self.handlers.clear();
    }

    /// Get an iterator over all handlers
    pub fn iter(&self) -> impl Iterator<Item = &LuaScriptHandler> {
        self.handlers.iter()
    }

    /// Reload all handlers from disk
    ///
    /// Re-reads source files for all registered handlers.
    pub fn reload_all(&mut self) -> Result<(), LuaError> {
        for handler in &mut self.handlers {
            handler.reload()?;
        }
        Ok(())
    }

    /// Get a shareable reference to runtime handlers
    pub fn runtime_handlers(&self) -> Arc<Mutex<Vec<RuntimeHandler>>> {
        self.runtime_handlers.clone()
    }

    pub fn handler_functions(&self) -> Arc<Mutex<HashMap<String, RegistryKey>>> {
        self.handler_functions.clone()
    }

    /// Get runtime handlers matching an event type, sorted by priority.
    ///
    /// Returns handlers registered via `crucible.on()` that match the given event type,
    /// sorted by priority (lower priority values execute first).
    ///
    /// # Arguments
    ///
    /// * `event_type` - The event type to filter by (exact match)
    /// * `identifier` - Optional identifier to match against handler patterns (e.g., tool name)
    pub fn runtime_handlers_for(
        &self,
        event_type: &str,
        identifier: Option<&str>,
    ) -> Vec<RuntimeHandler> {
        let handlers = self
            .runtime_handlers
            .lock()
            .expect("runtime_handlers: poisoned while querying event handlers");
        let mut matching: Vec<RuntimeHandler> = handlers
            .iter()
            .filter(|h| {
                h.event_type == event_type
                    && match (&h.pattern, identifier) {
                        (Some(pattern), Some(id)) => glob_match(pattern, id),
                        (Some(_), None) => false, // handler requires pattern match but caller provides no identifier
                        (None, _) => true,        // no pattern = match all
                    }
            })
            .cloned()
            .collect();
        matching.sort_by_key(|h| h.priority);
        matching
    }

    /// Execute a runtime-registered handler by name
    ///
    /// Retrieves the stored function from the registry and executes it with the event.
    /// The handler receives (ctx, event) as parameters where ctx is an empty table.
    ///
    /// # Arguments
    ///
    /// * `lua` - The Lua context
    /// * `name` - Handler name (e.g., "runtime_handler_0")
    /// * `event` - The session event to pass to the handler
    ///
    /// # Returns
    ///
    /// Returns `Ok(ScriptHandlerResult)` on success, or `Err` if handler not found or execution fails.
    pub async fn execute_runtime_handler(
        &self,
        lua: &Lua,
        name: &str,
        event: &SessionEvent,
    ) -> LuaResult<ScriptHandlerResult> {
        // Get the handler Function while holding the lock, then drop it before await
        let handler: Function = {
            let handler_functions = self
                .handler_functions
                .lock()
                .expect("handler_functions: poisoned while executing Lua handler function");
            let key = handler_functions
                .get(name)
                .ok_or_else(|| mlua::Error::RuntimeError(format!("Handler not found: {}", name)))?;
            lua.registry_value(key)?
        };

        let ctx_table = lua.create_table()?;
        let event_table = session_event_to_lua(lua, event)?;

        let result: Value = handler.call_async((ctx_table, event_table)).await?;

        interpret_handler_result(&result)
    }

    /// Convert discovered handlers to core `Handler` trait objects.
    ///
    /// This enables Lua handlers to be registered with the core `Reactor`
    /// for unified event dispatch. Returns handlers that implement
    /// `crucible_core::events::Handler`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crucible_lua::LuaScriptHandlerRegistry;
    /// use crucible_core::events::Reactor;
    ///
    /// let paths = vec![PathBuf::from("./handlers")];
    /// let registry = LuaScriptHandlerRegistry::discover(&paths)?;
    ///
    /// let mut reactor = Reactor::new();
    /// for handler in registry.to_core_handlers()? {
    ///     reactor.register(handler)?;
    /// }
    /// ```
    pub fn to_core_handlers(
        &self,
    ) -> Result<Vec<Box<dyn crucible_core::events::Handler>>, crate::LuaError> {
        use crate::core_handler::{LuaHandler, LuaHandlerMeta};

        let mut core_handlers: Vec<Box<dyn crucible_core::events::Handler>> = Vec::new();

        for script_handler in &self.handlers {
            let meta = &script_handler.metadata;

            // Convert event_type:pattern to core event_pattern format
            let event_pattern = if meta.pattern == "*" {
                meta.event_type.clone()
            } else {
                format!("{}:{}", meta.event_type, meta.pattern)
            };

            let lua_meta = LuaHandlerMeta::new(&meta.source_path, &meta.handler_fn)
                .with_event_pattern(event_pattern)
                .with_priority(meta.priority as i32);

            let handler = LuaHandler::with_source(lua_meta, script_handler.source().to_string())?;
            core_handlers.push(Box::new(handler));
        }

        Ok(core_handlers)
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
