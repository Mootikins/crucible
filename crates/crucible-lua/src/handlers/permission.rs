use mlua::{Function, IntoLua, Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::debug;

use super::hook_name::{HookName, StageId};
use super::registry::{
    bool_option, scope_from_opts, string_option, Firing, LuaScriptHandlerRegistry,
    RegistrationSpec, SessionScope,
};

/// The name a permission hook registers under in the shared store.
pub const PERMISSION_REQUEST_HOOK: HookName = HookName::Stage(StageId::PermissionRequest);

/// Result of permission hook execution
///
/// Represents the possible outcomes from a Lua permission hook:
/// - Allow: Skip prompt and allow the tool execution
/// - Deny: Skip prompt and deny the tool execution
/// - Prompt: Show normal permission prompt (hook returned nil or other)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionHookResult {
    /// Hook returned `{allow=true}` - skip prompt and allow
    Allow,
    /// Hook returned `{deny=true}` - skip prompt and deny
    Deny,
    /// Hook returned nil or other - show normal prompt
    Prompt,
}

/// The request that a Lua permission hook receives.
///
/// This is a Lua-side view, not a copy of a core type. The call is the core
/// [`CanonicalToolCall`](crucible_core::types::CanonicalToolCall). The view
/// adds only the facts of the gate that no core type holds: the arguments,
/// the read-only class and the session mode. Its one job is the conversion
/// to the hook table (`IntoLua` below). The core `PermRequest` is the prompt
/// that the daemon builds after the hooks answer, and it has no mode and no
/// read-only class.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// The canonical call. The hook reads its tool name, kind, command line,
    /// paths, URL, query and agent, so one hook decides a Crucible tool and
    /// the tool of each ACP agent.
    pub call: crucible_core::types::CanonicalToolCall,
    /// Tool arguments as JSON. The conversion also reads `file_path` from
    /// them.
    pub args: JsonValue,
    /// Whether the daemon classifies this tool as read-only.
    ///
    /// Safe tools normally short-circuit before the gate, so a hook would not
    /// see them — but an agent card's `ask` policy forces one through. A
    /// policy that keys off "is this mutating?" needs to know the difference
    /// rather than assume everything reaching it mutates.
    pub is_safe: bool,
    /// Session mode for the turn making this request ("ask" | "plan" |
    /// "auto"). Carried so the *policy* for a mode can live in Lua rather
    /// than being hard-coded in the daemon — the built-in `auto` auto-approve
    /// is itself just a default hook in `defaults/init.lua`. `None` where no
    /// mode is in scope (direct API callers, tests).
    pub mode: Option<String>,
}

impl PermissionRequest {
    /// The file that the call names: the `path` argument, else the `file`
    /// argument.
    fn file_path(&self) -> Option<&str> {
        self.args
            .get("path")
            .or_else(|| self.args.get("file"))
            .and_then(JsonValue::as_str)
    }
}

/// Register the cru.permissions.on_request() API for permission hooks
///
/// This allows Lua scripts to register callbacks that fire before permission prompts:
///
/// ```lua
/// -- Filter at registration instead of `if request.tool_name == "bash"`:
/// cru.permissions.on_request(function(request) ... end, { pattern = "bash" })
///
/// cru.permissions.on_request(function(request)
///     -- request.tool_name, request.args, request.file_path, request.mode
///     if request.mode == "auto" then
///         return {allow=true}  -- Auto mode approves everything
///     end
///     if request.tool_name == "bash" and string.match(request.args.command, "^npm ") then
///         return {allow=true}  -- Auto-allow npm commands
///     end
///     return nil  -- Show normal prompt
/// end)
/// ```
pub fn register_permission_hook_api(
    lua: &Lua,
    registry: LuaScriptHandlerRegistry,
) -> LuaResult<()> {
    // Same store as `cru.on`; see `register_cru_on_api`.
    super::install_registry(lua, registry.clone());
    let permissions = crate::lua_util::get_or_create_module(lua, "permissions")?;

    let on_request_fn =
        lua.create_function(move |lua, (handler, opts): (Function, Option<Table>)| {
            let (pattern, scope, key, once) = match &opts {
                Some(o) => {
                    let (scope, key) = scope_from_opts(
                        lua,
                        "cru.permissions.on_request",
                        PERMISSION_REQUEST_HOOK,
                        o,
                    )?;
                    (
                        string_option("cru.permissions.on_request", o, "pattern")?,
                        scope,
                        key,
                        bool_option("cru.permissions.on_request", o, "once")? == Some(true),
                    )
                }
                None => (None, SessionScope::Global, None, false),
            };

            let id = registry.register(
                lua,
                RegistrationSpec {
                    name: PERMISSION_REQUEST_HOOK,
                    pattern,
                    scope,
                    key,
                    once,
                    timeout_ms: None,
                    required: false,
                },
                handler,
            )?;

            debug!("Registered permission hook {id}");
            Ok(())
        })?;

    permissions.set("on_request", on_request_fn)?;
    // `Ns::over` on the live table, then a declaration for the one function on
    // it. The payload is the table `execute_permission_hooks` builds below, so
    // the two are read from the same file — the closest a hand-written payload
    // type gets to being checked.
    crate::host_registry::declare_value(
        lua,
        "cru.permissions.on_request",
        "(handler: (request: PermissionRequest) -> PermissionDecision, \
          opts: { pattern: string?, session: string?, \
          key: string?, once: boolean? }?) -> ()",
    )
    .map_err(|e| mlua::Error::external(e.to_string()))?;
    Ok(())
}

/// The shape of the table [`PermissionRequest::into_lua`] builds.
///
/// This exists to give the Luau declaration a schema to read, not to be
/// serialized itself: no code builds one. The declaration used to be a hand
/// Luau string, `host_api::PERMISSION_REQUEST`, kept beside the hand table
/// below with nothing holding the two together — a field added to the table
/// and missing from the string became a false type error at every correct
/// read of it, and a field dropped from the table left the string lying the
/// other way. Now both come from this one struct: the declaration is
/// `LuaType::of_schema::<PermissionRequestPayload>().to_luau()`, and
/// `the_permission_payload_matches_its_declaration` below still compares the
/// built table's keys against it, so a field added to one and not the other
/// still fails a test — the difference is that the "other" is now a Rust
/// field list, not a second string to remember to edit.
#[derive(serde::Serialize, utoipa::ToSchema)]
#[allow(dead_code)] // read only through `ToSchema`; nothing builds one
pub(crate) struct PermissionRequestPayload {
    tool_name: String,
    /// Arbitrary JSON: the tool's own arguments.
    args: serde_json::Value,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    is_safe: bool,
}

/// The table a permission hook receives.
impl IntoLua for &PermissionRequest {
    fn into_lua(self, lua: &Lua) -> LuaResult<Value> {
        let call = &self.call;
        let request_table = lua.create_table()?;
        request_table.set("tool_name", call.tool.as_str())?;
        request_table.set("args", lua.to_value(&self.args)?)?;
        request_table.set("kind", call.kind.as_str())?;
        request_table.set("paths", lua.to_value(&call.paths)?)?;
        for (key, value) in [
            ("command", &call.command),
            ("url", &call.url),
            ("query", &call.query),
            ("agent", &call.agent),
        ] {
            if let Some(value) = value {
                request_table.set(key, value.as_str())?;
            }
        }
        if let Some(path) = self.file_path() {
            request_table.set("file_path", path)?;
        }
        if let Some(ref mode) = self.mode {
            request_table.set("mode", mode.as_str())?;
        }
        request_table.set("is_safe", self.is_safe)?;
        Ok(Value::Table(request_table))
    }
}

/// Execute permission hooks and return the result
///
/// Executes every registered permission hook in registration order. The first
/// hook to return `{allow=true}` or `{deny=true}` wins. If all hooks return
/// nil, returns `Prompt`.
///
/// # Arguments
/// * `lua` - The Lua state
/// * `registry` - The shared registration store
/// * `request` - The permission request to evaluate
/// * `firing` - The session whose turn asked, so a session-scoped hook
///   answers for its own session and for no other. Passed straight to
///   [`LuaScriptHandlerRegistry::for_hook`](super::registry::LuaScriptHandlerRegistry::for_hook),
///   which is the one place a scope is read; this gate reads none itself.
///
/// # Returns
/// * `PermissionHookResult::Allow` - Hook returned `{allow=true}`
/// * `PermissionHookResult::Deny` - Hook returned `{deny=true}`
/// * `PermissionHookResult::Prompt` - All hooks returned nil or no hooks registered
///
/// Note: deliberately sync (not async). Permission decisions must be fast and cannot
/// call async APIs. The MutexGuards from the caller are not Send across await points.
///
/// # The time budget
///
/// Being synchronous is exactly why a `tokio::time::timeout` around this call
/// is inert: there is no await point at which a future could be cancelled. The
/// budget is the VM deadline instead, which interrupts the running Lua from
/// inside the instruction hook. It covers the whole loop, so one hook cannot
/// spend the budget of the hooks after it and the caller still gets an answer
/// within [`PERMISSION_BUDGET`](crate::handler_budget::PERMISSION_BUDGET).
///
/// The daemon used to measure the elapsed time AFTER this returned and discard
/// a late answer. That interrupted nothing: a hook running `while true do end`
/// held the thread and the permission request never came back at all.
pub fn execute_permission_hooks(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    request: &PermissionRequest,
    firing: Firing<'_>,
) -> LuaResult<PermissionHookResult> {
    // Registration order, and the first non-nil answer wins. The shipped
    // defaults load before any user file, so a shipped hook is asked FIRST —
    // which is why `runtime/defaults/init.luau` answers `nil` for every mode
    // but `plan`, leaving the decision to whatever registered after it. The
    // pattern filters on the tool name, as `cru.on`'s does.
    let hooks = registry.for_hook(PERMISSION_REQUEST_HOOK, Some(&request.call.tool), firing);
    if hooks.is_empty() {
        return Ok(PermissionHookResult::Prompt);
    }

    let request_table = request.into_lua(lua)?;
    // The session this gate belongs to, so a hook that registers another
    // handler for it resolves the id from the host. Held for the whole loop,
    // as the source bracket inside it is held for each hook.
    let _session = crate::plugin_context::enter_session(lua, firing.session());

    let _budget = crate::handler_budget::enter(
        lua,
        crate::handler_budget::PERMISSION_BUDGET,
        "the permission hook",
    );

    for hook in hooks {
        // The source the registration recorded, re-entered around the call: a
        // hook reaching `cru.storage` must find its own plugin's namespace.
        let handler: Function = hook.take_body(lua)?;
        let previous = crate::plugin_context::set_source(lua, hook.source.clone());
        let result = handler.call::<Value>(request_table.clone());
        crate::plugin_context::set_source(lua, previous);

        match result? {
            Value::Nil => {
                debug!("Permission hook {} returned nil, continuing", hook.id);
            }
            Value::Table(t) => {
                if t.get::<bool>("allow").unwrap_or(false) {
                    debug!("Permission hook {} returned allow=true", hook.id);
                    return Ok(PermissionHookResult::Allow);
                }
                if t.get::<bool>("deny").unwrap_or(false) {
                    debug!("Permission hook {} returned deny=true", hook.id);
                    return Ok(PermissionHookResult::Deny);
                }
                debug!(
                    "Permission hook {} returned table without allow/deny",
                    hook.id
                );
            }
            _ => {
                debug!(
                    "Permission hook {} returned unexpected type, treating as prompt",
                    hook.id
                );
            }
        }
    }

    Ok(PermissionHookResult::Prompt)
}

#[cfg(test)]
mod payload_contract {
    use super::*;

    fn payload(lua: &Lua, request: &PermissionRequest) -> Table {
        match request.into_lua(lua).expect("build the payload") {
            Value::Table(table) => table,
            other => panic!("the payload must be a table, got {other:?}"),
        }
    }

    fn request_with(args: JsonValue, mode: Option<&str>, is_safe: bool) -> PermissionRequest {
        PermissionRequest {
            call: crucible_core::types::CanonicalToolCall::crucible_tool("write", &args),
            args,
            mode: mode.map(str::to_string),
            is_safe,
        }
    }

    /// The conversion gives each value of the view to its table key.
    /// `file_path` is the `path` argument, else the `file` argument, else
    /// absent.
    #[test]
    fn the_payload_carries_each_value_of_the_request() {
        let lua = Lua::new();
        let table = payload(
            &lua,
            &request_with(
                serde_json::json!({ "path": "a.md", "file": "b.md" }),
                Some("auto"),
                true,
            ),
        );
        assert_eq!(table.get::<String>("tool_name").unwrap(), "write");
        assert_eq!(table.get::<String>("file_path").unwrap(), "a.md");
        assert_eq!(table.get::<String>("mode").unwrap(), "auto");
        assert!(table.get::<bool>("is_safe").unwrap());
        let args: Table = table.get("args").unwrap();
        assert_eq!(args.get::<String>("file").unwrap(), "b.md");

        let table = payload(
            &lua,
            &request_with(serde_json::json!({ "file": "b.md" }), None, false),
        );
        assert_eq!(table.get::<String>("file_path").unwrap(), "b.md");
        assert!(table.get::<Option<String>>("mode").unwrap().is_none());
        assert!(!table.get::<bool>("is_safe").unwrap());

        let table = payload(
            &lua,
            &request_with(serde_json::json!({ "command": "ls" }), None, false),
        );
        assert!(table.get::<Option<String>>("file_path").unwrap().is_none());
    }

    /// The payload table and its declared type must name the SAME fields.
    ///
    /// B1 asked for a payload record checked at registration the way a
    /// function's signature is. That mechanism was never built — the type is a
    /// hand-written string in `host_api::PAYLOAD_TYPES` and the table is
    /// hand-written here — so nothing noticed when the two drifted. Adding a
    /// field to the Rust and not the declaration produces a false type error
    /// at every correct read of it; dropping one leaves the declaration lying
    /// the other way. This is the narrower thing that IS testable: the field
    /// names, compared.
    #[test]
    fn the_permission_payload_matches_its_declaration() {
        let lua = Lua::new();
        // Every optional field populated, so the table carries its whole
        // surface rather than the subset a particular request happens to fill.
        let request = crate::PermissionRequest {
            call: crucible_core::types::CanonicalToolCall {
                url: Some("https://a.test".to_string()),
                query: Some("q".to_string()),
                agent: Some("codex".to_string()),
                ..crucible_core::types::CanonicalToolCall::crucible_tool(
                    "bash",
                    &serde_json::json!({ "command": "ls" }),
                )
            },
            args: serde_json::json!({ "command": "ls", "path": "/tmp/x" }),
            mode: Some("plan".to_string()),
            is_safe: false,
        };
        let table = payload(&lua, &request);

        let built: std::collections::BTreeSet<String> = table
            .pairs::<String, mlua::Value>()
            .flatten()
            .map(|(key, _)| key)
            .collect();

        let declared: std::collections::BTreeSet<String> =
            match crate::signature::LuaType::of_schema::<PermissionRequestPayload>() {
                crate::signature::LuaType::Record(fields) => {
                    fields.into_iter().map(|field| field.name).collect()
                }
                other => {
                    panic!("PermissionRequestPayload's schema must read as a record, got {other:?}")
                }
            };

        assert_eq!(
            built, declared,
            "the payload table and `PermissionRequestPayload`'s schema name different fields. \
             Add the field to both, or remove it from both."
        );
    }
}
