use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::{Function, Lua, RegistryKey, Result as LuaResult, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::conversion::session_event_to_lua;
use super::script_handler::{interpret_handler_result, ScriptHandlerResult};

/// Handlers registered at runtime by `crucible.on`.
///
/// Nothing is discovered from the filesystem: a handler exists because a
/// plugin called `crucible.on(event, opts, fn)` at load. The registry held a
/// second `Vec<LuaScriptHandler>` filled by annotation discovery, which was
/// removed along with that loader — reading the wrong one of the two is what
/// left the file-watch hook silently dead.
///
/// # Example
///
/// ```rust,ignore
/// // Registration happens from Lua, via the api this registry backs.
/// register_crucible_on_api(&lua, registry.runtime_handlers(), registry.handler_functions())?;
///
/// // Dispatch: select by event name, then execute each match.
/// for handler in registry.runtime_handlers_for("tool_result", Some(tool_name)) {
///     let outcome = registry
///         .execute_runtime_handler(&lua, &handler.name, &event, Some(session_id))
///         .await?;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LuaScriptHandlerRegistry {
    /// Runtime-registered handlers (via crucible.on())
    ///
    /// This Vec shrinks: `clear_plugin_handlers` drops a reloaded plugin's
    /// entries. Handler names must therefore come from `crucible_on.rs`'s
    /// monotonic allocator and never from this length — see the comment there.
    pub(super) runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
    /// Stored Lua function references (handler_name -> RegistryKey)
    ///
    /// The name is the dispatch key (`execute_runtime_handler`), so two
    /// registrants sharing a name is a misbinding, not a duplicate.
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
    /// Plugin that registered this handler, when it was registered during a
    /// plugin's load. Needed so a reload can drop that plugin's handlers —
    /// without it, every reload appends another copy of every handler and the
    /// stale ones keep firing against dead state.
    pub plugin: Option<String>,
}

impl LuaScriptHandlerRegistry {
    /// How many runtime handlers a plugin has registered via `crucible.on`.
    ///
    /// This is the count `plugin.list` reports. It used to come from the
    /// spec-table `handlers` field — which is parsed but never dispatched —
    /// so plugins using the real API showed 0 and plugins using the dead one
    /// showed a number that meant nothing.
    pub fn plugin_handler_count(&self, plugin: &str) -> usize {
        self.runtime_handlers
            .lock()
            .map(|handlers| {
                handlers
                    .iter()
                    .filter(|h| h.plugin.as_deref() == Some(plugin))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Drop every runtime handler registered by `plugin`, and its stored
    /// functions.
    ///
    /// Called before a plugin is (re)executed. `PluginRegistry` already does
    /// the equivalent for tools and commands; without it here, each reload
    /// appended another copy of every `crucible.on` handler and the stale ones
    /// kept firing — and since `pre_tool_call` fails closed, one stale handler
    /// raising against dead state would deny every tool call in every session.
    pub fn clear_plugin_handlers(&self, plugin: &str) {
        let Ok(mut handlers) = self.runtime_handlers.lock() else {
            return;
        };
        let mut dropped = Vec::new();
        handlers.retain(|h| {
            let keep = h.plugin.as_deref() != Some(plugin);
            if !keep {
                dropped.push(h.name.clone());
            }
            keep
        });
        if dropped.is_empty() {
            return;
        }
        if let Ok(mut functions) = self.handler_functions.lock() {
            for name in &dropped {
                functions.remove(name);
            }
        }
        tracing::debug!(plugin, count = dropped.len(), "cleared plugin handlers");
    }

    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            runtime_handlers: Arc::new(Mutex::new(Vec::new())),
            handler_functions: Arc::new(Mutex::new(HashMap::new())),
        }
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
    /// Retrieves the stored function from the registry and executes it with the
    /// event. The handler receives `(ctx, event)`; `ctx.session_id` carries the
    /// session the event belongs to when the dispatch site knows it.
    ///
    /// That field is what lets a handler registered once at plugin load serve
    /// many sessions (`oci` keys its containers by it). Registering handlers
    /// per-session instead is not an alternative: the registry is append-only,
    /// so per-session registration accumulates one stale copy per session for
    /// the daemon's lifetime.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ScriptHandlerResult)` on success, or `Err` if handler not found or execution fails.
    pub async fn execute_runtime_handler(
        &self,
        lua: &Lua,
        name: &str,
        event: &SessionEvent,
        session_id: Option<&str>,
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
        if let Some(id) = session_id {
            ctx_table.set("session_id", id)?;
        }
        let event_table = session_event_to_lua(lua, event)?;

        let result: Value = handler.call_async((ctx_table, event_table)).await?;

        interpret_handler_result(&result)
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
