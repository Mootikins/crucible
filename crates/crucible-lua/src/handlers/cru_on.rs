use mlua::{Lua, RegistryKey, Result as LuaResult, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::hook_name::{hook_names, HookName};
use super::registry::RuntimeHandler;

use crucible_core::fuzzy::levenshtein;

/// Reject a hook name nothing dispatches.
///
/// An **error**, not a warning. `crucible.on` runs at plugin load, and a plugin
/// whose hook can never fire is broken — the pre-existing `debug!` at
/// registration time told nobody, so a typo registered silently and the handler
/// simply never ran. This is a breaking change for a plugin with a typo, which is
/// the point.
fn validate_hook_name(event_type: &str) -> Result<(), mlua::Error> {
    if HookName::parse(event_type).is_some() {
        return Ok(());
    }
    let suggestion = hook_names()
        .min_by_key(|n| levenshtein(n, event_type))
        .filter(|n| levenshtein(n, event_type) <= 3);
    Err(mlua::Error::RuntimeError(match suggestion {
        Some(s) => format!("cru.on: unknown event `{event_type}` — did you mean `{s}`?"),
        None => format!(
            "cru.on: unknown event `{event_type}`. Valid: {}",
            hook_names().collect::<Vec<_>>().join(", ")
        ),
    }))
}

/// Register the cru.on() API for runtime handler registration
///
/// Supports two calling conventions:
///
/// ```lua
/// -- Simple (backward compatible):
/// cru.on("pre_tool_call", function(ctx, event) ... end)
///
/// -- With options (pattern + priority):
/// cru.on("pre_tool_call", { pattern = "bash", priority = 50 }, function(ctx, event) ... end)
/// ```
pub fn register_cru_on_api(
    lua: &Lua,
    runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
    handler_functions: Arc<Mutex<HashMap<String, RegistryKey>>>,
) -> LuaResult<()> {
    let handlers = runtime_handlers.clone();
    let functions = handler_functions.clone();

    // Monotonic source of runtime-handler names, scoped to this
    // `runtime_handlers`/`handler_functions` pair — every caller registers the
    // API exactly once against a freshly built store, so per-closure is
    // per-registry. A staging point: it belongs beside the Vec and the map, and
    // moves there when those three collapse into one owning store. `AtomicU64`
    // rather than `Cell` because the daemon enables the `send` feature, so this
    // closure must be `Send + Sync`.
    //
    // NEVER derive a name from `guard.len()`. `clear_plugin_handlers` shrinks
    // that Vec, so after a reload a length-derived name collides with one
    // another registrant — another plugin, or the user's `init.lua`, which is
    // evaluated into this same registry and holds the highest indices — still
    // owns in `handler_functions`. Dispatch is by name, so the collision
    // rebinds the survivor's handler to the reloaded plugin's body rather than
    // merely duplicating an entry, and with `pre_tool_call` failing closed a
    // body raising against the wrong event shape denies every matching tool
    // call in every session.
    let next_handler_id = AtomicU64::new(0);

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

        validate_hook_name(&event_type)?;

        let (pattern, priority, timeout_ms, handler) = match &args_vec[1] {
            Value::Function(f) => {
                // cru.on(event_type, handler) — backward compatible
                (None, 100i64, None, f.clone())
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
                let priority: i64 = opts.get("priority").unwrap_or(100);
                // A handler that legitimately runs long — a container build,
                // a large model call — says so here. Absent, the name it
                // registers for decides. See `handler_budget`.
                let timeout_ms: Option<u64> = opts.get("timeout_ms").ok();
                (pattern, priority, timeout_ms, handler)
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "cru.on: second argument must be a function or options table".into(),
                ))
            }
        };

        let mut guard = handlers
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock handlers: {}", e)))?;

        // Set by the loader around a plugin's execution so handlers can be
        // attributed and later dropped on reload. Rust-side app data, not a
        // Lua global: a plugin must not be able to name another plugin as the
        // owner, nor grant itself the interception right read just below.
        let context = crate::plugin_context::current_plugin_context(lua);
        let plugin: Option<String> = context.as_ref().map(|c| c.name.clone());

        // `Relaxed` suffices: the handlers mutex taken above brackets the whole
        // allocate-push-insert sequence, so it supplies the ordering.
        let name = format!(
            "runtime_handler_{}",
            next_handler_id.fetch_add(1, Ordering::Relaxed)
        );
        // Still inside the handlers lock, deliberately: handlers is the outer
        // lock (`clear_plugin_handlers` orders them the same way) and is held
        // across this mutation, so a dispatch racing a reload — the daemon
        // reads the registry without the loader mutex — can never see a
        // `RuntimeHandler` whose function is missing. Do not split these into
        // two critical sections.
        let mut func_guard = functions
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock functions: {}", e)))?;

        // Defense in depth: unreachable while names come from the monotonic
        // allocator above. Were it reached, an overwrite would orphan the live
        // body and silently point its owner's handler at this one for the
        // daemon's lifetime — refuse instead. Checked before anything is
        // mutated, so a refused registration leaves no handler without a
        // function (which `pre_tool_call`, failing closed, would turn into a
        // denied tool call).
        if func_guard.contains_key(&name) {
            return Err(mlua::Error::RuntimeError(format!(
                "handler name collision: '{name}' was already registered \
                 (registering plugin: {plugin:?})"
            )));
        }

        // Stored before the push for the same reason: nothing lands in
        // `runtime_handlers` until its function is in hand.
        let key = lua.create_registry_value(handler)?;
        guard.push(RuntimeHandler {
            event_type: event_type.clone(),
            name: name.clone(),
            priority,
            pattern: pattern.clone(),
            plugin: plugin.clone(),
            // Absent means "not loading a plugin" — a user's own init.lua,
            // which carries the operator's own authority. A LOADING plugin
            // holds only what its installation granted: the gate used to read
            // a Lua global with `.unwrap_or(true)`, so it failed OPEN for
            // every daemon-loaded plugin and was forgeable besides.
            may_intercept: context.is_none_or(|c| c.may_intercept),
            timeout_ms,
        });
        func_guard.insert(name.clone(), key);

        debug!(
            "Registered runtime handler '{}' for event '{}' (priority={}, pattern={:?})",
            name, event_type, priority, pattern
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
            validate_hook_name(name).expect("every listed name must validate");
        }
    }

    /// The bug this closes: `cru.on("pre_toolcall", …)` registered happily,
    /// logged at `debug`, and never fired.
    #[test]
    fn a_misspelt_hook_name_is_rejected_with_a_suggestion() {
        let err = validate_hook_name("pre_toolcall").expect_err("a typo must not register");
        let msg = err.to_string();
        assert!(msg.contains("pre_toolcall"), "{msg}");
        assert!(msg.contains("did you mean `pre_tool_call`"), "{msg}");
    }

    /// Nothing close enough to suggest gets the whole valid set instead.
    #[test]
    fn an_unrecognisable_hook_name_lists_the_valid_set() {
        let err = validate_hook_name("on_everything_please").expect_err("must not register");
        let msg = err.to_string();
        assert!(msg.contains("pre_tool_call"), "{msg}");
        assert!(msg.contains("tool:display_complete"), "{msg}");
    }
}
