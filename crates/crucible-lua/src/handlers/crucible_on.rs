use mlua::{Lua, RegistryKey, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::debug;

use super::hook_name::{hook_names, HookName};
use super::registry::RuntimeHandler;

/// Levenshtein distance, for the "did you mean" hint. Fifteen lines beats a
/// dependency.
fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut cur = vec![0usize; b_chars.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b_chars.len()]
}

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
        Some(s) => format!("crucible.on: unknown event `{event_type}` — did you mean `{s}`?"),
        None => format!(
            "crucible.on: unknown event `{event_type}`. Valid: {}",
            hook_names().collect::<Vec<_>>().join(", ")
        ),
    }))
}

/// Register the crucible.on() API for runtime handler registration
///
/// Supports two calling conventions:
///
/// ```lua
/// -- Simple (backward compatible):
/// crucible.on("pre_tool_call", function(ctx, event) ... end)
///
/// -- With options (pattern + priority):
/// crucible.on("pre_tool_call", { pattern = "bash", priority = 50 }, function(ctx, event) ... end)
/// ```
pub fn register_crucible_on_api(
    lua: &Lua,
    runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
    handler_functions: Arc<Mutex<HashMap<String, RegistryKey>>>,
) -> LuaResult<()> {
    let crucible: Table = match lua.globals().get("crucible") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            lua.globals().set("crucible", t.clone())?;
            t
        }
    };

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
                "crucible.on requires at least 2 arguments: (event_type, handler) or (event_type, opts, handler)".into(),
            ));
        }

        let event_type: String = match &args_vec[0] {
            Value::String(s) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "crucible.on: first argument must be a string (event type)".into(),
                ))
            }
        };

        validate_hook_name(&event_type)?;

        let (pattern, priority, handler) = match &args_vec[1] {
            Value::Function(f) => {
                // crucible.on(event_type, handler) — backward compatible
                (None, 100i64, f.clone())
            }
            Value::Table(opts) => {
                // crucible.on(event_type, opts, handler)
                if args_vec.len() < 3 {
                    return Err(mlua::Error::RuntimeError(
                        "crucible.on: when second argument is a table, third argument must be the handler function".into(),
                    ));
                }
                let handler = match &args_vec[2] {
                    Value::Function(f) => f.clone(),
                    _ => {
                        return Err(mlua::Error::RuntimeError(
                            "crucible.on: third argument must be a function".into(),
                        ))
                    }
                };
                let pattern: Option<String> = opts.get("pattern").ok();
                let priority: i64 = opts.get("priority").unwrap_or(100);
                (pattern, priority, handler)
            }
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "crucible.on: second argument must be a function or options table".into(),
                ))
            }
        };

        let mut guard = handlers
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock handlers: {}", e)))?;

        // Set by the loader around a plugin's execution so handlers can be
        // attributed and later dropped on reload.
        let plugin: Option<String> = lua
            .globals()
            .get::<Option<String>>("__crucible_loading_plugin__")
            .ok()
            .flatten();

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
            // which carries the operator's own authority.
            may_intercept: lua
                .globals()
                .get::<mlua::Table>("cru")
                .and_then(|c| c.get::<Option<bool>>("_current_plugin_may_intercept"))
                .ok()
                .flatten()
                .unwrap_or(true),
        });
        func_guard.insert(name.clone(), key);

        debug!(
            "Registered runtime handler '{}' for event '{}' (priority={}, pattern={:?})",
            name, event_type, priority, pattern
        );
        Ok(())
    })?;

    // Both namespaces, per `lua_util::register_in_namespaces`. Shipped scripts
    // are written against `cru.*`; `crucible.*` stays for existing configs.
    crucible.set("on", on_fn.clone())?;
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

    /// The bug this closes: `crucible.on("pre_toolcall", …)` registered happily,
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

    #[test]
    fn levenshtein_measures_what_it_claims() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("pre_toolcall", "pre_tool_call"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }
}
