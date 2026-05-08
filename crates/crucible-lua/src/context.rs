//! `cru.context` — conversation context manipulation.
//!
//! All async methods take an explicit `session_id`. Pure helpers
//! (`estimate_tokens`) take only their input and never touch the daemon.
//!
//! The functions here mirror three new methods on [`DaemonSessionApi`]:
//! `context_usage`, `compact`, `remove_messages`. `messages` is a thin
//! alias over `load_messages` kept here for namespace ergonomics so plugin
//! authors can stay inside `cru.context.*` when working with conversation
//! state.
//!
//! ## Lua surface
//!
//! ```lua
//! -- Pure helper, no daemon needed
//! local n = cru.context.estimate_tokens("hello world")  -- 3
//!
//! -- Daemon-backed
//! local usage = cru.context.usage(session_id)
//! cru.context.compact(session_id)
//! local msgs = cru.context.messages(session_id, { role = "user", limit = 10 })
//! local removed = cru.context.remove(session_id, { type = "last", n = 2 })
//! ```

use crate::error::LuaError;
use crate::lua_util::{get_or_create_namespace, register_in_namespaces};
use crate::sessions::DaemonSessionApi;
use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Register `cru.context` / `crucible.context` with stub functions.
///
/// Same shape as the real module — every async function returns
/// `(nil, "no daemon connected")`, `estimate_tokens` works fully (it's a
/// pure function). Used by the stub generator (`stubs.rs`) and by callers
/// that want a non-fatal placeholder before [`register_context_module`]
/// gets called with a real API.
pub fn register_context_module_stub(lua: &Lua) -> Result<(), LuaError> {
    let context = lua.create_table()?;

    context.set(
        "estimate_tokens",
        lua.create_function(|_, text: String| {
            Ok(crucible_core::traits::context_ops::estimate_tokens(&text))
        })?,
    )?;

    macro_rules! stub_async {
        ($name:expr, $args:ty) => {
            let f = lua.create_async_function(|lua, _args: $args| async move {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
            context.set($name, f)?;
        };
    }

    stub_async!("usage", String);
    stub_async!("compact", String);
    stub_async!("messages", (String, Value));
    stub_async!("remove", (String, Value));

    register_in_namespaces(lua, "context", context)?;
    Ok(())
}

/// Register `cru.context` / `crucible.context` with daemon-backed implementations.
pub fn register_context_module(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    let context = lua.create_table()?;

    // estimate_tokens(text) -> integer
    // Pure function — uses crucible_core's chars/4 heuristic.
    context.set(
        "estimate_tokens",
        lua.create_function(|_, text: String| {
            Ok(crucible_core::traits::context_ops::estimate_tokens(&text))
        })?,
    )?;

    // usage(session_id) -> ({ messages, prompt_tokens, budget, percent }, nil) or (nil, err)
    let a = Arc::clone(&api);
    let usage_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.context_usage(session_id).await {
                Ok(val) => {
                    let lua_val = lua.to_value(&val)?;
                    Ok((lua_val, Value::Nil))
                }
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    context.set("usage", usage_fn)?;

    // compact(session_id) -> (true, nil) or (nil, err)
    let a = Arc::clone(&api);
    let compact_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.compact(session_id).await {
                Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    context.set("compact", compact_fn)?;

    // messages(session_id, opts?) -> (messages_table, nil) or (nil, err)
    // opts: { role = "user"|"assistant"|"system", limit = N }
    // Thin alias over load_messages; identical semantics to cru.sessions.messages.
    let a = Arc::clone(&api);
    let messages_fn =
        lua.create_async_function(move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let (role_filter, limit) = match opts {
                    Value::Table(ref t) => {
                        (t.get::<String>("role").ok(), t.get::<usize>("limit").ok())
                    }
                    _ => (None, None),
                };
                match a.load_messages(session_id, role_filter, limit).await {
                    Ok(msgs) => {
                        let table = lua.create_table()?;
                        for (i, msg) in msgs.iter().enumerate() {
                            let lua_val = lua.to_value(msg)?;
                            table.set(i + 1, lua_val)?;
                        }
                        Ok((Value::Table(table), Value::Nil))
                    }
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
    context.set("messages", messages_fn)?;

    // remove(session_id, range) -> (count, nil) or (nil, err)
    // range: { type = "all" } | { type = "last"|"first", n = N } |
    //        { type = "indices", start = S, end = E }
    let a = Arc::clone(&api);
    let remove_fn =
        lua.create_async_function(move |lua, (session_id, range): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let json: serde_json::Value =
                    serde_json::to_value(&range).map_err(mlua::Error::external)?;
                match a.remove_messages(session_id, json).await {
                    Ok(n) => Ok((Value::Integer(n as i64), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
    context.set("remove", remove_fn)?;

    register_in_namespaces(lua, "context", context)?;

    Ok(())
}

/// Registry of Lua-defined output validators, keyed by name.
///
/// Storage is a `HashMap<String, RegistryKey>` behind a `Mutex` behind an
/// `Arc`. `RegistryKey` is `Send + Sync` (it's just an integer handle into
/// the Lua-owned registry), so the registry itself can be cheaply shared
/// across the daemon stream loop and the plugin's Lua state.
///
/// Resolving a key back into an `mlua::Function` requires the same `Lua`
/// the key was created in, which is why [`Self::run`] takes `&Lua` —
/// enforcing co-location at the type level. The plugin loader holds both
/// the `Lua` and an `Arc<LuaValidatorRegistry>`; the stream loop is given
/// access via the same plugin loader.
#[derive(Default)]
pub struct LuaValidatorRegistry {
    inner: Mutex<HashMap<String, RegistryKey>>,
}

impl LuaValidatorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if a validator with this name has been registered.
    pub fn has(&self, name: &str) -> bool {
        self.inner.lock().unwrap().contains_key(name)
    }

    /// Run a registered validator against `text`.
    ///
    /// Outer `Result` carries `mlua::Error` for "couldn't even invoke"
    /// (registry resolution failed, the Lua function panicked). Inner
    /// `Result<(), String>` carries the validator's verdict — `Ok(())`
    /// for pass, `Err(reason)` for failure. Unknown validator names
    /// surface as inner `Err`, not outer error, because the daemon
    /// stream loop wants to feed the reason back to the agent for retry
    /// rather than crash.
    pub fn run(
        &self,
        lua: &Lua,
        name: &str,
        text: &str,
    ) -> mlua::Result<Result<(), String>> {
        let guard = self.inner.lock().unwrap();
        let Some(key) = guard.get(name) else {
            return Ok(Err(format!("lua validator '{name}' not registered")));
        };
        let f: Function = lua.registry_value(key)?;
        // Lua validators may return `(bool)` or `(bool, string)`. Use
        // `MultiValue` to accept either shape.
        let result: mlua::MultiValue = f.call(text.to_string())?;
        let mut iter = result.into_iter();
        let ok = match iter.next() {
            Some(Value::Boolean(b)) => b,
            // Treat non-bool first return as failure with a descriptive
            // reason rather than panicking — plugin authors get a useful
            // message instead of an opaque type error.
            Some(other) => {
                return Ok(Err(format!(
                    "lua validator '{name}' returned non-boolean first value: {}",
                    other.type_name()
                )));
            }
            None => {
                return Ok(Err(format!(
                    "lua validator '{name}' returned no values"
                )));
            }
        };
        if ok {
            return Ok(Ok(()));
        }
        let reason = match iter.next() {
            Some(Value::String(s)) => s.to_str().map(|s| s.to_owned()).ok(),
            _ => None,
        };
        Ok(Err(reason.unwrap_or_else(|| {
            format!("lua validator '{name}' returned false")
        })))
    }

    fn insert(&self, name: String, key: RegistryKey) {
        self.inner.lock().unwrap().insert(name, key);
    }
}

/// Register `cru.context.register_validator(name, fn)` against the given
/// validator registry.
///
/// Idempotent w.r.t. `cru.context` — if `register_context_module` (or its
/// stub) has already mounted the table, this attaches alongside the
/// existing daemon-backed methods. Otherwise an empty `cru.context`
/// table is created so plugins can call `register_validator` standalone.
///
/// The registry is shared via `Arc` so the daemon stream loop can hold a
/// handle and dispatch by name without re-entering Lua's symbol table.
pub fn register_context_validators(
    lua: &Lua,
    registry: Arc<LuaValidatorRegistry>,
) -> Result<(), LuaError> {
    let context = ensure_cru_context_table(lua)?;

    let reg = Arc::clone(&registry);
    let f = lua.create_function(move |lua, (name, func): (String, Function)| {
        let key = lua.create_registry_value(func)?;
        reg.insert(name, key);
        Ok(())
    })?;
    context.set("register_validator", f)?;
    Ok(())
}

/// Look up `cru.context`, creating the namespace + sub-table if absent.
///
/// Mirrors the access pattern in `register_context_module` so calls in
/// either order produce the same end state. Both `cru.context` and
/// `crucible.context` are aliased to the same table — we ensure both.
fn ensure_cru_context_table(lua: &Lua) -> mlua::Result<Table> {
    let cru = get_or_create_namespace(lua, "cru")?;
    let crucible = get_or_create_namespace(lua, "crucible")?;
    let context: Table = match cru.get::<Value>("context")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            cru.set("context", t.clone())?;
            crucible.set("context", t.clone())?;
            t
        }
    };
    // Ensure crucible.context aliases the same table even when cru.context
    // existed first via `register_context_module` (which already mirrors it).
    if !matches!(crucible.get::<Value>("context")?, Value::Table(_)) {
        crucible.set("context", context.clone())?;
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::ResponsePart;
    use std::future::Future;
    use std::pin::Pin;

    /// Minimal stub. All methods unused by these tests `unimplemented!()`;
    /// the four exercised methods (`load_messages`, plus the three Wave 1
    /// defaults overridden below) return canned values.
    struct StubApi;

    impl DaemonSessionApi for StubApi {
        fn create_session(
            &self,
            _: String,
            _: Option<String>,
            _: Option<String>,
            _: Vec<String>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            unimplemented!()
        }
        fn get_session(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>
        {
            unimplemented!()
        }
        fn list_sessions(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            unimplemented!()
        }
        fn configure_agent(
            &self,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn send_message(
            &self,
            _: String,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            unimplemented!()
        }
        fn cancel(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
            unimplemented!()
        }
        fn pause(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn resume(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn end_session(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn respond_to_permission(
            &self,
            _: String,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn subscribe(
            &self,
            _: String,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                            String,
                        >,
                    > + Send,
            >,
        > {
            unimplemented!()
        }
        fn unsubscribe(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn load_messages(
            &self,
            _: String,
            role_filter: Option<String>,
            limit: Option<usize>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>
        {
            Box::pin(async move {
                let mut msgs = vec![
                    serde_json::json!({ "role": "system", "content": "sys" }),
                    serde_json::json!({ "role": "user", "content": "hi" }),
                    serde_json::json!({ "role": "assistant", "content": "hello" }),
                ];
                if let Some(role) = role_filter {
                    msgs.retain(|m| m.get("role").and_then(|r| r.as_str()) == Some(role.as_str()));
                }
                if let Some(n) = limit {
                    let start = msgs.len().saturating_sub(n);
                    msgs = msgs.split_off(start);
                }
                Ok(msgs)
            })
        }
        fn inject_context(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn collect_subagents(
            &self,
            _: Vec<String>,
            _: Option<f64>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            unimplemented!()
        }
        fn fork_session(
            &self,
            _: String,
            _: Option<u64>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            unimplemented!()
        }
        fn cache_stats(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            unimplemented!()
        }
        fn send_and_collect(
            &self,
            _: String,
            _: String,
            _: Option<f64>,
            _: Option<usize>,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>>
                    + Send,
            >,
        > {
            unimplemented!()
        }

        // The three new defaults — override with canned successes so we can
        // exercise the (value, nil) Lua return path.
        fn context_usage(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            Box::pin(async {
                Ok(serde_json::json!({
                    "messages": 5,
                    "prompt_tokens": 1234,
                    "budget": 200_000,
                    "percent": 0.617_f64,
                }))
            })
        }
        fn compact(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }
        fn remove_messages(
            &self,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
            Box::pin(async { Ok(2) })
        }
    }

    #[test]
    fn estimate_tokens_returns_chars_div_4() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        // "hello world" = 11 chars, ceil(11/4) = 3
        let n: usize = lua
            .load(r#"return cru.context.estimate_tokens("hello world")"#)
            .eval()
            .unwrap();
        assert_eq!(n, 3);

        let zero: usize = lua
            .load(r#"return cru.context.estimate_tokens("")"#)
            .eval()
            .unwrap();
        assert_eq!(zero, 0);
    }

    #[test]
    fn registers_in_both_cru_and_crucible_namespaces() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let cru_ok: bool = lua
            .load(r#"return type(cru.context.estimate_tokens) == "function""#)
            .eval()
            .unwrap();
        let crucible_ok: bool = lua
            .load(r#"return type(crucible.context.estimate_tokens) == "function""#)
            .eval()
            .unwrap();
        assert!(cru_ok);
        assert!(crucible_ok);
    }

    #[tokio::test]
    async fn usage_returns_table() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let prompt_tokens: i64 = lua
            .load(
                r#"
                local u, err = cru.context.usage("test-session")
                assert(err == nil, "unexpected error: " .. tostring(err))
                return u.prompt_tokens
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(prompt_tokens, 1234);
    }

    #[tokio::test]
    async fn compact_returns_true() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let ok: bool = lua
            .load(
                r#"
                local ok, err = cru.context.compact("test-session")
                assert(err == nil, "unexpected error: " .. tostring(err))
                return ok
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(ok);
    }

    #[tokio::test]
    async fn messages_filters_by_role() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let count: usize = lua
            .load(
                r#"
                local msgs, err = cru.context.messages("test-session", { role = "user" })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return #msgs
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn register_validator_stores_callback_and_runs() {
        let lua = Lua::new();
        let registry = Arc::new(LuaValidatorRegistry::new());
        register_context_validators(&lua, Arc::clone(&registry)).unwrap();

        lua.load(
            r#"
            cru.context.register_validator("ok", function(t)
                if t == "ok" then return true end
                return false, "expected 'ok' but got " .. t
            end)
            "#,
        )
        .exec()
        .unwrap();

        assert!(registry.has("ok"));

        let result = registry.run(&lua, "ok", "ok").unwrap();
        assert!(result.is_ok(), "expected validator to pass on 'ok' input");

        let result = registry.run(&lua, "ok", "bad").unwrap();
        let err = result.unwrap_err();
        assert!(
            err.contains("expected 'ok'"),
            "reason did not propagate: {err}"
        );
    }

    #[test]
    fn registry_run_unknown_name_returns_error() {
        let lua = Lua::new();
        let registry = LuaValidatorRegistry::new();
        let result = registry.run(&lua, "nonexistent", "anything").unwrap();
        let err = result.unwrap_err();
        assert!(
            err.contains("not registered"),
            "expected 'not registered' in: {err}"
        );
    }

    #[test]
    fn registry_run_validator_returning_only_true_passes() {
        // Lua validators may return just `true` (no second value).
        let lua = Lua::new();
        let registry = Arc::new(LuaValidatorRegistry::new());
        register_context_validators(&lua, Arc::clone(&registry)).unwrap();
        lua.load(r#"cru.context.register_validator("yes", function(_) return true end)"#)
            .exec()
            .unwrap();
        assert!(registry.run(&lua, "yes", "anything").unwrap().is_ok());
    }

    #[test]
    fn register_validator_default_reason_when_only_false_returned() {
        let lua = Lua::new();
        let registry = Arc::new(LuaValidatorRegistry::new());
        register_context_validators(&lua, Arc::clone(&registry)).unwrap();
        lua.load(r#"cru.context.register_validator("no", function(_) return false end)"#)
            .exec()
            .unwrap();
        let result = registry.run(&lua, "no", "x").unwrap();
        let err = result.unwrap_err();
        assert!(
            err.contains("returned false"),
            "expected default reason, got: {err}"
        );
    }

    #[test]
    fn register_validator_attaches_to_existing_context_module() {
        // Both ordering paths should mount on the same `cru.context` table.
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();
        let registry = Arc::new(LuaValidatorRegistry::new());
        register_context_validators(&lua, Arc::clone(&registry)).unwrap();

        let both_present: bool = lua
            .load(
                r#"
                return type(cru.context.estimate_tokens) == "function"
                   and type(cru.context.register_validator) == "function"
                   and type(crucible.context.register_validator) == "function"
                "#,
            )
            .eval()
            .unwrap();
        assert!(both_present);
    }

    #[tokio::test]
    async fn remove_returns_count() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let n: i64 = lua
            .load(
                r#"
                local n, err = cru.context.remove("test-session", { type = "last", n = 2 })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return n
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(n, 2);
    }
}
