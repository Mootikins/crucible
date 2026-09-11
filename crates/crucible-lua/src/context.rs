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
use crate::host_registry::Ns;
use crate::lua_util::get_or_create_namespace;
use crate::sessions::DaemonSessionApi;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::sync::Arc;

/// The declared type of each `cru.context` function.
///
/// One set of declarations covers the stub registration and the daemon-backed
/// one, because both answer at the same path and a plugin author cannot tell
/// which is mounted. The four daemon-backed functions answer the SAME pair
/// either way — `(result, nil)` on success, `(nil, message)` on failure — and
/// the stub simply always takes the failure branch with "no daemon
/// connected". So the pair, not the success value alone, is the type, and
/// BOTH halves are optional: exactly one of the two is nil on any call.
///
/// None of these raises. A caller reads the second value, never `pcall`.
const ESTIMATE_TOKENS: &str = "(text: string) -> number";
/// The shape `DaemonSessionApi::context_usage` documents and returns.
const USAGE: &str = "(session_id: string) -> \
     ({ messages: number, prompt_tokens: number, budget: number, percent: number }?, string?)";
/// `true` on success, not a value: compaction runs on the next agent turn.
const COMPACT: &str = "(session_id: string) -> (boolean?, string?)";
/// `opts` reaches Rust as a plain `Value`, so omitting it is legal and its
/// `nil` takes the "no filter" branch. The body reads `role`, `limit` and
/// `tools` and nothing else; `tools = true` adds the `tool_call` and
/// `tool_result` rows. The messages themselves are opaque JSON on the way
/// through, so `{ any }` is as far as this code can narrow them.
const MESSAGES: &str = "(session_id: string, \
     opts: { role: string?, limit: number?, tools: boolean? }?) -> ({ any }?, string?)";
/// `range` is NOT narrowed to a record, for one reason: the `indices` shape
/// has an `end` field, and `end` is a Luau keyword that a record type cannot
/// name. The three shapes the daemon accepts are in the comment on the
/// closure. This code serialises the table whole and validates nothing.
const REMOVE: &str = "(session_id: string, range: table) -> (number?, string?)";

/// Register `cru.context` with stub functions.
///
/// Same shape as the real module — every async function returns
/// `(nil, "no daemon connected")`, `estimate_tokens` works fully (it's a
/// pure function). Used by the stub generator (`stubs.rs`) and by callers
/// that want a non-fatal placeholder before [`register_context_module`]
/// gets called with a real API.
pub fn register_context_module_stub(lua: &Lua) -> Result<(), LuaError> {
    // Over the mounted table, never a fresh one. See the note on
    // [`ensure_cru_context_table`].
    let mut context = Ns::over(lua, "cru.context", ensure_cru_context_table(lua)?);

    context.func("estimate_tokens", ESTIMATE_TOKENS, |_, text: String| {
        Ok(crucible_core::traits::context_ops::estimate_tokens(&text))
    })?;

    macro_rules! stub_async {
        ($name:expr, $decl:expr, $args:ty) => {
            context.async_func($name, $decl, |lua, _args: $args| async move {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
        };
    }

    stub_async!("usage", USAGE, String);
    stub_async!("compact", COMPACT, String);
    stub_async!("messages", MESSAGES, (String, Value));
    stub_async!("remove", REMOVE, (String, Value));

    Ok(())
}

/// Register `cru.context` with daemon-backed implementations.
pub fn register_context_module(lua: &Lua, api: Arc<dyn DaemonSessionApi>) -> Result<(), LuaError> {
    // Over the mounted table, never a fresh one. `DaemonPluginLoader` calls
    // `register_context_validators` when it builds the VM and this function
    // later, from `upgrade_with_sessions`. A fresh table here dropped
    // `cru.context.register_validator` at that moment, so every validator a
    // plugin registered at init became unreachable the instant the daemon
    // API arrived. See the note on [`ensure_cru_context_table`].
    let mut ns = Ns::over(lua, "cru.context", ensure_cru_context_table(lua)?);

    // Pure function — uses crucible_core's chars/4 heuristic.
    ns.func("estimate_tokens", ESTIMATE_TOKENS, |_, text: String| {
        Ok(crucible_core::traits::context_ops::estimate_tokens(&text))
    })?;

    let a = Arc::clone(&api);
    ns.async_func("usage", USAGE, move |lua, session_id: String| {
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

    let a = Arc::clone(&api);
    ns.async_func("compact", COMPACT, move |lua, session_id: String| {
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

    // Thin alias over load_messages; identical semantics to cru.session.messages.
    let a = Arc::clone(&api);
    ns.async_func(
        "messages",
        MESSAGES,
        move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let (role_filter, limit, include_tools) = match opts {
                    Value::Table(ref t) => (
                        t.get::<String>("role").ok(),
                        t.get::<usize>("limit").ok(),
                        t.get::<bool>("tools").unwrap_or(false),
                    ),
                    _ => (None, None, false),
                };
                match a
                    .load_messages(session_id, role_filter, limit, include_tools)
                    .await
                {
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
        },
    )?;

    // range: { type = "all" } | { type = "last"|"first", n = N } |
    //        { type = "indices", start = S, end = E }
    let a = Arc::clone(&api);
    ns.async_func(
        "remove",
        REMOVE,
        move |lua, (session_id, range): (String, Value)| {
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
        },
    )?;

    Ok(())
}

/// Look up `cru.context`, creating the namespace + sub-table if absent.
///
/// Mirrors the access pattern in `register_context_module` so calls in
/// either order produce the same end state.
fn ensure_cru_context_table(lua: &Lua) -> mlua::Result<Table> {
    let cru = get_or_create_namespace(lua, "cru")?;
    let context: Table = match cru.get::<Value>("context")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            cru.set("context", t.clone())?;
            t
        }
    };
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::ResponsePart;
    use std::future::Future;
    use std::pin::Pin;

    /// Mounting the daemon API must not evict what is already on
    /// `cru.context`.
    ///
    /// It did. `register_context_module` built a FRESH table and mounted it
    /// over `cru.context`, so anything registered earlier became nil the
    /// instant the daemon API arrived. The loader deliberately mounts in that
    /// order, so the eviction was the normal path, not an edge case.
    ///
    /// A plugin's own key stands in for what used to be
    /// `cru.context.register_validator` here: the guarantee is about the
    /// table, not about any one member of it.
    #[test]
    fn the_daemon_api_does_not_evict_what_is_already_mounted() {
        let lua = Lua::new();
        register_context_module_stub(&lua).expect("the stub mounts first");

        lua.load(r#"cru.context.mine = function() return 7 end"#)
            .exec()
            .expect("a plugin adds its own key at init");

        register_context_module(&lua, Arc::new(StubApi)).expect("the daemon API arrives");

        let still_there: bool = lua
            .load("return type(cru.context.mine) == 'function'")
            .eval()
            .expect("the surface answers");
        assert!(
            still_there,
            "mounting the daemon API must not evict an existing key"
        );

        let has_api: bool = lua
            .load("return type(cru.context.usage) == 'function'")
            .eval()
            .expect("the surface answers");
        assert!(has_api, "and the daemon API must be there too");
    }

    /// Minimal stub. All methods unused by these tests `unimplemented!()`;
    /// the four exercised methods (`load_messages`, plus the three Wave 1
    /// defaults overridden below) return canned values.
    struct StubApi;

    impl DaemonSessionApi for StubApi {
        // The tests here do not call these. A required method with no body
        // makes a missing override a compile error, not a silent stub.
        fn complete(
            &self,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            unimplemented!()
        }

        fn undo(
            &self,
            _: String,
            _: usize,
        ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
            unimplemented!()
        }

        fn can_undo(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
            unimplemented!()
        }

        fn undo_depth(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
            unimplemented!()
        }

        fn undo_history(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            unimplemented!()
        }

        fn review_list_hunks(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            unimplemented!()
        }

        fn review_set_state(
            &self,
            _: String,
            _: String,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }

        fn review_comment(
            &self,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            unimplemented!()
        }

        fn review_resolve_comment(
            &self,
            _: String,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }

        fn request_interaction(
            &self,
            _: String,
            _: serde_json::Value,
            _: u64,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            unimplemented!()
        }
        fn create_session(
            &self,
            _: serde_json::Value,
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
        fn cancel(&self, _: String) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
            unimplemented!()
        }
        fn pause(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            unimplemented!()
        }
        fn resume(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
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
            include_tools: bool,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            Box::pin(async move {
                let mut msgs = vec![
                    serde_json::json!({ "role": "system", "content": "sys" }),
                    serde_json::json!({ "role": "user", "content": "hi" }),
                    serde_json::json!({ "role": "assistant", "content": "hello" }),
                ];
                // The tool row appears only when the alias passed the flag, so
                // the Lua test reads the row count to see that `tools` arrived.
                if include_tools {
                    msgs.push(serde_json::json!({
                        "role": "tool_call", "id": "c1", "name": "bash", "args": { "command": "ls" },
                    }));
                }
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
            _: bool,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>,
                    > + Send,
            >,
        > {
            unimplemented!()
        }

        // Override with canned successes so we can
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
        fn compact(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
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
            .load(r#"return type(cru.context.estimate_tokens) == "function""#)
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

    #[tokio::test]
    async fn messages_includes_tool_rows_when_asked() {
        let lua = Lua::new();
        let api: Arc<dyn DaemonSessionApi> = Arc::new(StubApi);
        register_context_module(&lua, api).unwrap();

        let last_role: String = lua
            .load(
                r#"
                local msgs, err = cru.context.messages("test-session", { tools = true })
                assert(err == nil, "unexpected error: " .. tostring(err))
                assert(#msgs == 4, "expected 4 rows, got " .. #msgs)
                return msgs[#msgs].role
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(last_role, "tool_call");
    }
}
