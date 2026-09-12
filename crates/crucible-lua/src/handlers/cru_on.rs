use mlua::{Lua, Result as LuaResult, Value};
use tracing::debug;

use super::hook_name::{hook_names, HookName};
use super::registry::{
    scope_from_opts, LuaScriptHandlerRegistry, RegistrationSpec, SessionScope,
};

/// Reject a hook name `cru.on` cannot register for.
///
/// An **error**, not a warning. `cru.on` runs at plugin load, and a plugin
/// whose hook can never fire is broken — the pre-existing `debug!` at
/// registration time told nobody, so a typo registered silently and the handler
/// simply never ran. This is a breaking change for a plugin with a typo, which is
/// the point.
///
/// A name that exists but has its OWN registration API is refused too, and the
/// message names that API. The four merged names share one store with `cru.on`
/// and nothing else: each carries a different payload, so a handler registered
/// here would read the wrong argument and fail at fire time.
fn resolve_hook_name(event_type: &str) -> Result<HookName, mlua::Error> {
    let name = super::hook_name::parse_or_suggest("cru.on", event_type, hook_names())?;
    match name.own_api() {
        None => Ok(name),
        Some(api) => Err(mlua::Error::RuntimeError(format!(
            "cru.on: `{event_type}` is registered with `{api}`, which passes it \
             the argument it expects"
        ))),
    }
}

/// Register the cru.on() API for runtime handler registration
///
/// Supports two calling conventions:
///
/// ```lua
/// -- Simple (backward compatible):
/// cru.on("pre_tool_call", function(ctx, event) ... end)
///
/// -- With options. Handlers run in registration order; nothing reorders
/// -- them. See `LuaScriptHandlerRegistry::for_hook`.
/// cru.on("pre_tool_call", { pattern = "bash" }, function(ctx, event) ... end)
///
/// -- For one session, from inside that session. Registering it again
/// -- replaces it, so a resume leaves one handler and not two:
/// cru.on("pre_tool_call", { session = ctx.session_id, key = "ralph" }, handler)
///
/// -- Once, then gone. The host removes the row before it runs the body:
/// cru.on("turn:complete", { once = true }, handler)
/// ```
pub fn register_cru_on_api(lua: &Lua, registry: LuaScriptHandlerRegistry) -> LuaResult<()> {
    // Every registration API on this VM writes the SAME store, so the one the
    // host wires `cru.on` to is the one `cru.on_session_start` finds.
    super::install_registry(lua, registry.clone());
    // One wiring point for the pair. `cru.clear` unwrites the store `cru.on`
    // writes, so a VM that has one must have the other; ten hosts each
    // remembering a second call is the fault this avoids.
    super::register_cru_clear_api(lua, registry.clone())?;
    let on_fn = lua.create_function(move |lua, args: mlua::MultiValue| {
        let args_vec: Vec<Value> = args.into_vec();
        if args_vec.len() < 2 {
            return Err(mlua::Error::RuntimeError(
                "cru.on requires at least 2 arguments: (event_type, handler) or (event_type, opts, handler)".into(),
            ));
        }

        let event_type: String = match &args_vec[0] {
            Value::String(s) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "cru.on: first argument must be a string (event type)".into(),
                ))
            }
        };

        let name = resolve_hook_name(&event_type)?;

        let (pattern, timeout_ms, scope, key, once, handler) = match &args_vec[1] {
            Value::Function(f) => {
                // cru.on(event_type, handler) — backward compatible
                (None, None, SessionScope::Global, None, false, f.clone())
            }
            Value::Table(opts) => {
                // cru.on(event_type, opts, handler)
                if args_vec.len() < 3 {
                    return Err(mlua::Error::RuntimeError(
                        "cru.on: when second argument is a table, third argument must be the handler function".into(),
                    ));
                }
                let handler = match &args_vec[2] {
                    Value::Function(f) => f.clone(),
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "cru.on: third argument must be a function".into(),
                        ))
                    }
                };
                let pattern: Option<String> = opts.get("pattern").ok();
                // A handler that legitimately runs long — a container build,
                // a large model call — says so here. Absent, the name it
                // registers for decides. See `handler_budget`.
                let timeout_ms: Option<u64> = opts.get("timeout_ms").ok();
                // A handler that retires itself after one call. The host
                // removes the row before it runs the body; see
                // `LuaScriptHandlerRegistry::retire_if_once`.
                let once: bool = opts.get("once").unwrap_or(false);
                // Which sessions, and what this registration calls itself.
                // The host resolves the session id; see `scope_from_opts`.
                let (scope, key) = scope_from_opts(lua, "cru.on", name, opts)?;
                (pattern, timeout_ms, scope, key, once, handler)
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "cru.on: second argument must be a function or options table".into(),
                ))
            }
        };

        let id = registry.register(
            lua,
            RegistrationSpec {
                name,
                pattern: pattern.clone(),
                scope: scope.clone(),
                key,
                once,
                timeout_ms,
                required: false,
            },
            handler,
        )?;

        debug!(
            "Registered runtime handler {} for event '{}' (pattern={:?}, scope={:?})",
            id, event_type, pattern, scope
        );
        Ok(())
    })?;

    crate::lua_util::get_or_create_namespace(lua, "cru")?.set("on", on_fn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name the enums declare registers.
    ///
    /// The old form of this test walked every `.rs` file under `crates/` and
    /// compared the literals it found against a hand-written `&[&str]`. It was
    /// satisfiable without adding the entry, because its needle accepted a bare
    /// constant declaration. Names are variants now, so the compiler holds the
    /// two directions together and this is the whole of what is left.
    #[test]
    fn a_valid_hook_name_is_accepted() {
        for name in hook_names() {
            resolve_hook_name(name).expect("every listed name must validate");
        }
    }

    /// The bug this closes: `cru.on("pre_toolcall", …)` registered happily,
    /// logged at `debug`, and never fired.
    #[test]
    fn a_misspelt_hook_name_is_rejected_with_a_suggestion() {
        let err = resolve_hook_name("pre_toolcall").expect_err("a typo must not register");
        let msg = err.to_string();
        assert!(msg.contains("pre_toolcall"), "{msg}");
        assert!(msg.contains("did you mean `pre_tool_call`"), "{msg}");
    }

    /// Nothing close enough to suggest gets the whole valid set instead.
    #[test]
    fn an_unrecognisable_hook_name_lists_the_valid_set() {
        let err = resolve_hook_name("on_everything_please").expect_err("must not register");
        let msg = err.to_string();
        assert!(msg.contains("pre_tool_call"), "{msg}");
        assert!(msg.contains("tool:display_complete"), "{msg}");
    }

    /// The four merged names share the store, not the calling convention. A
    /// handler registered here would be called with a session handle or a
    /// permission request, read the wrong argument, and fail at fire time.
    #[test]
    fn a_merged_name_is_refused_and_names_its_own_api() {
        for (name, api) in [
            ("permission:request", "cru.permissions.on_request"),
            ("session:start", "cru.on_session_start"),
            ("session:end", "cru.on_session_end"),
            ("provider:auth", "cru.on_provider_auth"),
        ] {
            let err = resolve_hook_name(name).expect_err("cru.on must refuse a merged name");
            let msg = err.to_string();
            assert!(msg.contains(name), "{msg}");
            assert!(msg.contains(api), "{msg}");
        }
    }

    /// And the suggestion list must not advertise them either.
    #[test]
    fn the_valid_set_omits_every_merged_name() {
        let listed: Vec<&str> = hook_names().collect();
        for name in [
            "permission:request",
            "session:start",
            "session:end",
            "provider:auth",
        ] {
            assert!(
                !listed.contains(&name),
                "`{name}` is offered by `cru.on` but refused by it"
            );
        }
    }
}
