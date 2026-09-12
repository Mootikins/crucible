use mlua::{Function, Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
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

/// A permission request passed to Lua hooks
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// Tool name (e.g., "write", "bash")
    pub tool_name: String,
    /// Tool arguments as JSON
    pub args: JsonValue,
    /// File path if applicable
    pub file_path: Option<String>,
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

/// The table a permission hook receives.
///
/// One function so `PermissionRequest` in `host_api` has something to be
/// checked against. The declaration was a hand-written constant beside a
/// hand-written table and nothing held the two together: a field added here
/// and missing there becomes a false type error at every correct read of it,
/// and a field dropped here leaves the declaration lying the other way.
/// `the_permission_payload_matches_its_declaration` compares them.
pub(crate) fn build_request_table(
    lua: &Lua,
    request: &crate::PermissionRequest,
) -> LuaResult<Table> {
    let request_table = lua.create_table()?;
    request_table.set("tool_name", request.tool_name.as_str())?;
    request_table.set("args", lua.to_value(&request.args)?)?;
    if let Some(ref path) = request.file_path {
        request_table.set("file_path", path.as_str())?;
    }
    if let Some(ref mode) = request.mode {
        request_table.set("mode", mode.as_str())?;
    }
    request_table.set("is_safe", request.is_safe)?;
    Ok(request_table)
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
///   answers for its own session and for no other. This is the synchronous
///   twin of the selection in
///   [`LuaScriptHandlerRegistry::for_hook`](super::registry::LuaScriptHandlerRegistry::for_hook),
///   and the only other place a scope is read.
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
    let hooks = registry.for_hook(PERMISSION_REQUEST_HOOK, Some(&request.tool_name), firing);
    if hooks.is_empty() {
        return Ok(PermissionHookResult::Prompt);
    }

    let request_table = build_request_table(lua, request)?;
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
            tool_name: "bash".to_string(),
            args: serde_json::json!({ "command": "ls" }),
            file_path: Some("/tmp/x".to_string()),
            mode: Some("plan".to_string()),
            is_safe: false,
        };
        let table = build_request_table(&lua, &request).expect("build the payload");

        let built: std::collections::BTreeSet<String> = table
            .pairs::<String, mlua::Value>()
            .flatten()
            .map(|(key, _)| key)
            .collect();

        let declared: std::collections::BTreeSet<String> =
            match crate::signature::LuaType::parse(crate::host_api::PERMISSION_REQUEST) {
                Ok(crate::signature::LuaType::Record(fields)) => {
                    fields.into_iter().map(|field| field.name).collect()
                }
                other => panic!("PermissionRequest must parse as a record, got {other:?}"),
            };

        assert_eq!(
            built, declared,
            "the payload table and `PermissionRequest` name different fields. \
             Add the field to both, or remove it from both."
        );
    }
}
