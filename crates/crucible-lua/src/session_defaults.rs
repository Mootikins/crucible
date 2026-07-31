//! Global session defaults — the values a NEW session starts from.
//!
//! ```lua
//! cru.defaults.system_prompt = "You are Crucible…"
//! cru.defaults.temperature   = 0.3
//! ```
//!
//! ```fennel
//! (set cru.defaults.system_prompt "You are Crucible…")
//! ```
//!
//! ## Why this is not `cru.o`
//!
//! [`crate::session_api`] deliberately rejects a `vim.o`-style global, and that
//! reasoning still holds: `vim.o` doubles as an alias for the *current* context,
//! and the daemon multiplexes sessions, so there is no current one to alias.
//!
//! This tier is the other half of `vim.o` — the global value new buffers
//! inherit — which is unambiguous under multiplexing. Named `defaults` rather
//! than `o` so it cannot be misread as "the session I'm in right now":
//!
//! | Neovim  | Crucible                    |
//! |---------|-----------------------------|
//! | `vim.o`  | `cru.defaults.x` (this)  |
//! | `vim.bo`| `session.x`                 |
//!
//! ## Why properties rather than an `opt` object
//!
//! Assignment (`cru.defaults.x = v`) reads the same in Lua and Fennel.
//! Neovim's `vim.opt.x:append(…)` does not — in Fennel it degrades to
//! `(: (. cru.opt :x) :append …)`. List-valued options would be the only
//! reason to want the method form, and there are none here, so the surface
//! stays plain assignment and callers use ordinary string operations:
//!
//! ```lua
//! cru.defaults.system_prompt = cru.defaults.system_prompt .. "\n\nBe terse."
//! ```
//!
//! ## Ordering
//!
//! Any file may set these at any time — last write wins, as with any Lua
//! global. The daemon reads them when it materializes a session's agent
//! config, so every file on the runtimepath has already run by then.

use mlua::{Lua, MetaMethod, Result as LuaResult, UserData, UserDataMethods, Value};
use std::sync::{Arc, RwLock};

/// Session settings that carry a global default. Every field is `Option`
/// because "unset" is meaningful: a `None` default leaves whatever the agent
/// card or session already specified untouched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionDefaultValues {
    pub system_prompt: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub thinking_budget: Option<i64>,
    /// Starting mode. Settable from an `on_session_start` hook, which is why
    /// it lives here rather than only on `SessionAgent`: the hook runs before
    /// the agent is built, so it is choosing what the agent starts as.
    pub mode: Option<String>,
}

/// Shared handle to the daemon's session defaults.
///
/// Cheap to clone; every session VM registers the same underlying store, so a
/// default set by one file is visible to the daemon regardless of which VM ran
/// it. This is plain process-local state, not an RPC surface — unlike
/// `session.x`, there is no per-session actor to route through.
#[derive(Debug, Clone, Default)]
pub struct SessionDefaults {
    inner: Arc<RwLock<SessionDefaultValues>>,
}

impl SessionDefaults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> SessionDefaultValues {
        self.inner
            .read()
            .expect("session defaults: poisoned")
            .clone()
    }

    /// Replace every value at once. Used by tests and by config seeding.
    pub fn set(&self, values: SessionDefaultValues) {
        *self.inner.write().expect("session defaults: poisoned") = values;
    }

    fn update<F: FnOnce(&mut SessionDefaultValues)>(&self, f: F) {
        f(&mut self.inner.write().expect("session defaults: poisoned"));
    }
}

impl UserData for SessionDefaults {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            let values = this.get();
            match key.as_str() {
                "system_prompt" => match values.system_prompt {
                    Some(s) => lua.create_string(&s).map(Value::String),
                    None => Ok(Value::Nil),
                },
                "temperature" => Ok(values.temperature.map(Value::Number).unwrap_or(Value::Nil)),
                "max_tokens" => Ok(values
                    .max_tokens
                    .map(|n| Value::Integer(n as i64))
                    .unwrap_or(Value::Nil)),
                "thinking_budget" => Ok(values
                    .thinking_budget
                    .map(Value::Integer)
                    .unwrap_or(Value::Nil)),
                // Naming an option that doesn't exist is a typo, and silent nil
                // is how a typo becomes a config that mysteriously does nothing.
                _ => Err(mlua::Error::runtime(format!(
                    "unknown default: cru.defaults.{key}"
                ))),
            }
        });

        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, val): (String, Value)| {
                match key.as_str() {
                    "system_prompt" => {
                        let prompt = match val {
                            Value::Nil => None,
                            other => Some(lua.unpack::<String>(other)?),
                        };
                        this.update(|v| v.system_prompt = prompt);
                        Ok(())
                    }
                    // Bounds mirror `session.temperature` exactly. A default
                    // that accepts what the per-session setter rejects would
                    // fail later, at the provider, with a worse message.
                    "temperature" => {
                        let temp = match val {
                            Value::Nil => None,
                            other => {
                                let t: f64 = lua.unpack(other)?;
                                if !(0.0..=2.0).contains(&t) {
                                    return Err(mlua::Error::runtime(
                                        "temperature must be 0.0-2.0",
                                    ));
                                }
                                Some(t)
                            }
                        };
                        this.update(|v| v.temperature = temp);
                        Ok(())
                    }
                    "max_tokens" => {
                        let tokens = match val {
                            Value::Nil => None,
                            Value::Integer(n) if n > 0 => Some(n as u32),
                            Value::Number(n) if n > 0.0 => Some(n as u32),
                            _ => {
                                return Err(mlua::Error::runtime(
                                    "max_tokens must be positive or nil",
                                ))
                            }
                        };
                        this.update(|v| v.max_tokens = tokens);
                        Ok(())
                    }
                    "thinking_budget" => {
                        let budget = match val {
                            Value::Nil => None,
                            other => Some(lua.unpack::<i64>(other)?),
                        };
                        this.update(|v| v.thinking_budget = budget);
                        Ok(())
                    }
                    _ => Err(mlua::Error::runtime(format!(
                        "unknown default: cru.defaults.{key}"
                    ))),
                }
            },
        );
    }
}

/// A [`crate::SessionConfigRpc`] whose reads and writes land in a
/// [`SessionDefaults`] rather than a live session.
///
/// This is what `session.x` means inside an `on_session_start` hook: the
/// session has no agent yet, so there is nothing to reconfigure — the hook is
/// choosing the values the agent will be *built* with. Seeded from the global
/// defaults, so a hook reads the inherited value before overriding it, exactly
/// like a Neovim `FileType` autocmd seeing the global option before setting
/// the buffer-local one:
///
/// ```lua
/// cru.on_session_start(function(session)
///   session.system_prompt = session.system_prompt .. "\n\nCite ticket IDs."
/// end)
/// ```
pub struct SessionDefaultsRpc {
    store: SessionDefaults,
}

impl SessionDefaultsRpc {
    pub fn new(store: SessionDefaults) -> Self {
        Self { store }
    }
}

impl crate::session_api::SessionConfigRpc for SessionDefaultsRpc {
    fn get_system_prompt(&self) -> Option<String> {
        self.store.get().system_prompt
    }

    fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
        self.store
            .update(|v| v.system_prompt = Some(prompt.to_string()));
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.store.get().temperature
    }

    fn set_temperature(&self, temp: f64) -> Result<(), String> {
        self.store.update(|v| v.temperature = Some(temp));
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.store.get().max_tokens
    }

    fn set_max_tokens(&self, tokens: Option<u32>) -> Result<(), String> {
        self.store.update(|v| v.max_tokens = tokens);
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        self.store.get().thinking_budget
    }

    fn get_mode(&self) -> String {
        self.store.get().mode.unwrap_or_else(|| "chat".to_string())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        self.store.update(|v| v.mode = Some(mode.to_string()));
        Ok(())
    }

    fn set_thinking_budget(&self, budget: i64) -> Result<(), String> {
        self.store.update(|v| v.thinking_budget = Some(budget));
        Ok(())
    }
}

/// Register `cru.defaults` (and the `crucible.defaults` alias) on `lua`.
///
/// Both namespaces, per [`crate::lua_util::register_in_namespaces`]. Shipped
/// scripts are written against `cru.*`; `crucible.*` stays for existing
/// configs. The same store backs both, so it does not matter which name a
/// file uses.
pub fn register_session_defaults(lua: &Lua, defaults: SessionDefaults) -> LuaResult<()> {
    for namespace in ["cru", "crucible"] {
        crate::lua_util::get_or_create_namespace(lua, namespace)?
            .set("defaults", defaults.clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with_defaults() -> (Lua, SessionDefaults) {
        let lua = Lua::new();
        let defaults = SessionDefaults::new();
        register_session_defaults(&lua, defaults.clone()).unwrap();
        (lua, defaults)
    }

    #[test]
    fn assignment_reaches_the_store() {
        let (lua, defaults) = lua_with_defaults();
        lua.load(r#"cru.defaults.system_prompt = "be terse""#)
            .exec()
            .unwrap();
        assert_eq!(defaults.get().system_prompt.as_deref(), Some("be terse"));
    }

    #[test]
    fn reads_back_what_was_written() {
        let (lua, _) = lua_with_defaults();
        let out: String = lua
            .load(
                r#"cru.defaults.system_prompt = "hello"
                   return cru.defaults.system_prompt"#,
            )
            .eval()
            .unwrap();
        assert_eq!(out, "hello");
    }

    /// The append idiom the API is shaped for: plain string ops, no `opt:append()`.
    #[test]
    fn a_later_file_can_append_to_an_earlier_default() {
        let (lua, defaults) = lua_with_defaults();
        lua.load(r#"cru.defaults.system_prompt = "base""#)
            .exec()
            .unwrap();
        lua.load(r#"cru.defaults.system_prompt = cru.defaults.system_prompt .. " + more""#)
            .exec()
            .unwrap();
        assert_eq!(defaults.get().system_prompt.as_deref(), Some("base + more"));
    }

    #[test]
    fn last_write_wins() {
        let (lua, defaults) = lua_with_defaults();
        lua.load(r#"cru.defaults.temperature = 0.1"#)
            .exec()
            .unwrap();
        lua.load(r#"cru.defaults.temperature = 0.9"#)
            .exec()
            .unwrap();
        assert_eq!(defaults.get().temperature, Some(0.9));
    }

    #[test]
    fn nil_clears_a_default() {
        let (lua, defaults) = lua_with_defaults();
        lua.load(r#"cru.defaults.system_prompt = "x""#)
            .exec()
            .unwrap();
        lua.load(r#"cru.defaults.system_prompt = nil"#)
            .exec()
            .unwrap();
        assert_eq!(defaults.get().system_prompt, None);
    }

    #[test]
    fn temperature_bounds_match_the_per_session_setter() {
        let (lua, _) = lua_with_defaults();
        let err = lua
            .load(r#"cru.defaults.temperature = 5.0"#)
            .exec()
            .unwrap_err();
        assert!(err.to_string().contains("0.0-2.0"), "got: {err}");
    }

    /// A typo must not be a silently-inert config line.
    #[test]
    fn an_unknown_default_is_an_error_on_write_and_read() {
        let (lua, _) = lua_with_defaults();
        let write = lua
            .load(r#"cru.defaults.sytsem_prompt = "typo""#)
            .exec()
            .unwrap_err();
        assert!(
            write.to_string().contains("unknown default"),
            "got: {write}"
        );
        let read = lua
            .load(r#"return cru.defaults.nope"#)
            .eval::<Value>()
            .unwrap_err();
        assert!(read.to_string().contains("unknown default"), "got: {read}");
    }

    /// The surface is assignment-only specifically so it survives the Fennel
    /// round-trip. `vim.opt.x:append(…)` would compile to
    /// `(: (. cru.opt :x) :append …)`; these compile to `set`/`..`, which
    /// is why there is no `opt` object. Compiled and RUN, not eyeballed — a
    /// form that parses but assigns to the wrong place would still pass a
    /// compile-only check.
    #[cfg(feature = "fennel")]
    #[test]
    fn the_surface_reads_and_works_in_fennel() {
        let (lua, defaults) = lua_with_defaults();

        let lua_src = crate::fennel::compile_fennel(
            r#"(set cru.defaults.system_prompt "from fennel")
               (set cru.defaults.temperature 0.4)"#,
        )
        .expect("fennel source must compile");
        lua.load(&lua_src).exec().expect("compiled fennel must run");

        assert_eq!(
            defaults.get().system_prompt.as_deref(),
            Some("from fennel"),
            "assignment must reach the store through the Fennel round-trip"
        );
        assert_eq!(defaults.get().temperature, Some(0.4));

        // The append idiom, in Fennel.
        let append = crate::fennel::compile_fennel(
            r#"(set cru.defaults.system_prompt
                    (.. cru.defaults.system_prompt " + more"))"#,
        )
        .expect("fennel append must compile");
        lua.load(&append).exec().expect("compiled fennel must run");
        assert_eq!(
            defaults.get().system_prompt.as_deref(),
            Some("from fennel + more")
        );
    }

    /// The store is shared by handle, so a default set on one VM is visible to
    /// the daemon and to every other VM registered against the same store.
    #[test]
    fn the_store_is_shared_across_vms() {
        let defaults = SessionDefaults::new();
        let a = Lua::new();
        let b = Lua::new();
        register_session_defaults(&a, defaults.clone()).unwrap();
        register_session_defaults(&b, defaults.clone()).unwrap();

        a.load(r#"cru.defaults.system_prompt = "from A""#)
            .exec()
            .unwrap();
        let seen: String = b
            .load(r#"return cru.defaults.system_prompt"#)
            .eval()
            .unwrap();
        assert_eq!(seen, "from A");
    }
}

#[cfg(test)]
mod session_start_hook_tests {
    use super::*;
    use crate::session_api::SessionConfigRpc;

    /// Setting the mode in an `on_session_start` hook must work, and must not
    /// take the rest of the hook down with it.
    ///
    /// `SessionConfigRpc`'s setters used to default to `Ok(())`, so
    /// `session.mode = "plan"` was silently inert. Defaulting them to an error
    /// made the assignment *raise*, which aborts the whole hook body — so the
    /// `system_prompt` line after it, which had always worked, stopped
    /// running. Silence was wrong; taking the rest of the hook with it is
    /// worse. `mode` is a real session default now.
    #[test]
    fn setting_mode_does_not_abort_the_rest_of_the_hook() {
        let store = SessionDefaults::new();
        let rpc = SessionDefaultsRpc::new(store.clone());

        rpc.set_mode("plan").expect("mode is a session default");
        rpc.set_system_prompt("after the mode line")
            .expect("the line after must still run");

        assert_eq!(store.get().mode.as_deref(), Some("plan"));
        assert_eq!(
            store.get().system_prompt.as_deref(),
            Some("after the mode line")
        );
        assert_eq!(rpc.get_mode(), "plan", "and reads back");
    }
}
