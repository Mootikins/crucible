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
/// register_cru_on_api(&lua, registry.runtime_handlers(), registry.handler_functions())?;
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
    /// entries. Handler names must therefore come from `cru_on.rs`'s
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
    /// Whether the registering plugin's installation lets it intercept, or
    /// `None` for a handler registered outside every plugin load — a user's
    /// own `init.lua`, which carries the operator's authority.
    ///
    /// Decided at registration from what the plugin declared, not at the call
    /// site: authorization is a property of the plugin the operator installed.
    /// The dispatcher re-enters exactly this, so a handler firing three turns
    /// later still runs as its own plugin — which is what `cru.storage` keys
    /// on and what `intercepts_tools` is read from.
    pub may_intercept_grant: Option<bool>,
    /// What the registration asked for with `{ timeout_ms = … }`, in
    /// milliseconds. `None` takes the budget of the name it registered for.
    pub timeout_ms: Option<u64>,
}

impl RuntimeHandler {
    /// Whether this handler may take a tool call over — return
    /// `{ handled = true, … }` or a transform from `pre_tool_call`.
    ///
    /// A handler without it may observe and may `cancel`; its `handled` and
    /// transform results are refused, because `handled` returns before the
    /// permission gate. A handler with nothing recorded was registered outside
    /// a plugin load and is trusted, having the same authority as the
    /// configuration that registered it.
    pub fn may_intercept(&self) -> bool {
        self.may_intercept_grant.unwrap_or(true)
    }
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
    /// Returns `Ok(ScriptHandlerResult)` on success, or `Err` if execution
    /// fails. An unknown `name` is NOT an error: the handler was unregistered
    /// between the dispatch snapshot and execution (plugin reload mid-call)
    /// and has no opinion — `PassThrough` is returned.
    pub async fn execute_runtime_handler(
        &self,
        lua: &Lua,
        name: &str,
        event: &SessionEvent,
        session_id: Option<&str>,
    ) -> LuaResult<ScriptHandlerResult> {
        let event_table = session_event_to_lua(lua, event)?;
        self.execute_handler_with_payload(lua, name, Value::Table(event_table), session_id)
            .await
    }

    /// Run the handler `name` with `payload` as its event argument. This is the
    /// one body behind [`Self::execute_runtime_handler`] and the JSON-payload
    /// stages in `before_execute.rs`.
    pub(super) async fn execute_handler_with_payload(
        &self,
        lua: &Lua,
        name: &str,
        payload: Value,
        session_id: Option<&str>,
    ) -> LuaResult<ScriptHandlerResult> {
        // The owner and the grant recorded when the handler registered. A
        // deferred call keeps the identity registration fixed, so a handler
        // that calls `cru.storage` from a later turn still reaches its own
        // plugin's namespace and holds no more authority than its plugin does.
        let (context, event_type, timeout_ms) = {
            let handlers = self
                .runtime_handlers
                .lock()
                .expect("runtime_handlers: poisoned while executing Lua handler function");
            match handlers.iter().find(|h| h.name == name) {
                Some(h) => (
                    h.plugin
                        .as_ref()
                        .map(|plugin| crate::plugin_context::PluginContext {
                            name: plugin.clone(),
                            may_intercept: h.may_intercept(),
                        }),
                    h.event_type.clone(),
                    h.timeout_ms,
                ),
                None => (None, String::new(), None),
            }
        };
        let budget = super::hook_name::budget_for(&event_type, timeout_ms);

        // Get the handler Function while holding the lock, then drop it before await
        let handler: Function = {
            let handler_functions = self
                .handler_functions
                .lock()
                .expect("handler_functions: poisoned while executing Lua handler function");
            let Some(key) = handler_functions.get(name) else {
                // Unregistered between the dispatch snapshot and execution — a
                // plugin reload clears its names while a call is in flight. An
                // absent handler has no opinion; erroring instead lands in
                // `pre_tool_call`'s fail-closed arm and denies the tool call
                // on behalf of a handler that no longer exists.
                tracing::debug!(handler = %name, "handler unregistered mid-dispatch; passing through");
                return Ok(ScriptHandlerResult::PassThrough);
            };
            lua.registry_value(key)?
        };

        let ctx_table = lua.create_table()?;
        if let Some(id) = session_id {
            ctx_table.set("session_id", id)?;
        }

        let previous = crate::plugin_context::set_plugin_context(lua, context);
        // Two mechanisms, because one is not enough. The tokio timeout ends a
        // handler that AWAITS — a sleep, an http call, a shell command — by
        // cancelling the future at an await point. It cannot end
        // `while true do end`, which never yields and never gives the runtime
        // back; the VM deadline does that, from inside Lua's own instruction
        // hook. Neither covers the other's case.
        let call = {
            let _budget =
                crate::handler_budget::enter(lua, budget, format!("the `{event_type}` handler"));
            match tokio::time::timeout(budget, handler.call_async::<Value>((ctx_table, payload)))
                .await
            {
                Ok(call) => call,
                Err(_elapsed) => Err(mlua::Error::runtime(format!(
                    "the `{event_type}` handler exceeded its {} ms time budget and was cancelled",
                    budget.as_millis()
                ))),
            }
        };
        // Restored before the `?`: a context left behind by a raising handler
        // would attribute the next registration to the wrong plugin.
        crate::plugin_context::set_plugin_context(lua, previous);

        interpret_handler_result(&call?)
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
