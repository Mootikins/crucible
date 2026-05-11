//! `cru.grammar` — GBNF grammar bindings for constrained LLM generation.
//!
//! Plugins build a [`Grammar`](crucible_core::types::Grammar) handle from
//! either a raw GBNF string or one of a small set of presets, then attach
//! it to a session. The daemon forces the next agent turn to emit text
//! matching the grammar (provided the backend supports GBNF — non-llama
//! backends hard-error today).
//!
//! ```lua
//! local g = cru.grammar.new([[root ::= "yes" | "no"]])
//! cru.grammar.set_session_grammar(session_id, g)
//! -- ... next turn output is constrained to "yes" or "no" ...
//! cru.grammar.clear_session_grammar(session_id)
//! ```
//!
//! Presets are functions (not module-level constants) so each call returns
//! a fresh handle. The userdata is opaque — call `:to_string()` for the
//! raw GBNF text or `:name()` for the human-readable label.
//!
//! ## Architecture
//!
//! Stub registration (`register_grammar_module`) installs only the
//! constructor + presets — they're pure (no daemon needed). Session
//! set/get/clear go through [`DaemonGrammarApi`], implemented daemon-side.
//! This keeps the Lua crate free of session-manager and agent-cache
//! concerns.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_core::types::Grammar;
use mlua::{AnyUserData, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Lua-visible handle to a GBNF grammar. Opaque; access via methods.
#[derive(Clone)]
pub struct LuaGrammar {
    pub grammar: Grammar,
}

impl LuaGrammar {
    pub fn new(grammar: Grammar) -> Self {
        Self { grammar }
    }
}

impl UserData for LuaGrammar {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("to_string", |_, this, ()| Ok(this.grammar.content.clone()));
        methods.add_method("content", |_, this, ()| Ok(this.grammar.content.clone()));
        methods.add_method("name", |_, this, ()| Ok(this.grammar.name.clone()));

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match this.grammar.name.as_deref() {
                Some(n) => format!("cru.grammar({})", n),
                None => "cru.grammar(<unnamed>)".to_string(),
            })
        });
    }
}

/// Daemon-side bridge for session-attached grammars. Implemented by the
/// daemon so the Lua crate stays free of `AgentManager` knowledge.
///
/// All methods are `async` because grammar attachment may need to invalidate
/// agent caches and emit broadcast events, both of which sit behind awaits.
pub trait DaemonGrammarApi: Send + Sync + 'static {
    /// Attach a grammar to a session. Returns `Err(msg)` if the session's
    /// backend doesn't support GBNF — non-negotiable per the design plan:
    /// silent fallback is worse than failing fast.
    fn set_session_grammar(
        &self,
        session_id: String,
        grammar: Grammar,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Detach a grammar from a session. Idempotent — clearing an already-
    /// empty session is `Ok(())`.
    fn clear_session_grammar(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Fetch the currently-attached grammar, if any.
    fn get_session_grammar(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Grammar>, String>> + Send>>;
}

/// Register `cru.grammar.*` without a daemon — constructor + presets work
/// (they're pure), but session set/get/clear return `(nil, "no daemon
/// connected")`. Mirrors `register_team_module_stub`.
pub fn register_grammar_module(lua: &Lua) -> Result<(), LuaError> {
    let grammar = lua.create_table()?;
    install_constructors(lua, &grammar)?;

    let no_daemon_set = lua.create_async_function(|lua, _args: (String, Value)| async move {
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    grammar.set("set_session_grammar", no_daemon_set)?;

    let no_daemon_clear = lua.create_async_function(|lua, _sid: String| async move {
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    grammar.set("clear_session_grammar", no_daemon_clear)?;

    let no_daemon_get = lua.create_async_function(|lua, _sid: String| async move {
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    grammar.set("get_session_grammar", no_daemon_get)?;

    register_in_namespaces(lua, "grammar", grammar)?;
    Ok(())
}

/// Register `cru.grammar.*` backed by a real [`DaemonGrammarApi`].
pub fn register_grammar_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonGrammarApi>,
) -> Result<(), LuaError> {
    register_grammar_module(lua)?;

    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let grammar: Table = cru.get("grammar")?;

    // set_session_grammar(session_id, grammar_userdata) -> (true, nil) | (nil, err)
    {
        let api = Arc::clone(&api);
        let set_fn = lua.create_async_function(move |lua, (sid, g): (String, Value)| {
            let api = Arc::clone(&api);
            async move {
                let parsed = match value_to_grammar(&g) {
                    Ok(g) => g,
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                match api.set_session_grammar(sid, parsed).await {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
        grammar.set("set_session_grammar", set_fn)?;
    }

    // clear_session_grammar(session_id) -> (true, nil) | (nil, err)
    {
        let api = Arc::clone(&api);
        let clear_fn = lua.create_async_function(move |lua, sid: String| {
            let api = Arc::clone(&api);
            async move {
                match api.clear_session_grammar(sid).await {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
        grammar.set("clear_session_grammar", clear_fn)?;
    }

    // get_session_grammar(session_id) -> (grammar_userdata|nil, err)
    {
        let api = Arc::clone(&api);
        let get_fn = lua.create_async_function(move |lua, sid: String| {
            let api = Arc::clone(&api);
            async move {
                match api.get_session_grammar(sid).await {
                    Ok(Some(g)) => {
                        let ud = lua.create_userdata(LuaGrammar::new(g))?;
                        Ok((Value::UserData(ud), Value::Nil))
                    }
                    Ok(None) => Ok((Value::Nil, Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
        grammar.set("get_session_grammar", get_fn)?;
    }

    Ok(())
}

/// Install `cru.grammar.new` and `cru.grammar.presets.*`.
///
/// These are pure (don't touch the daemon) so the same code runs in stub
/// and full registrations.
fn install_constructors(lua: &Lua, grammar: &Table) -> Result<(), LuaError> {
    // new(content: string) -> userdata
    //
    // Raises a Lua error on empty / whitespace-only input so plugin authors
    // get fail-fast feedback rather than a runtime "your model wandered"
    // surprise. We deliberately do NOT parse GBNF here — that's llama.cpp's
    // job and a full parser would balloon the crate. Empty-string is the
    // only rejection threshold we can apply without re-implementing GBNF.
    let new_fn = lua.create_function(|_lua, content: String| {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(mlua::Error::runtime(
                "cru.grammar.new: grammar content must be non-empty",
            ));
        }
        if !has_root_rule(trimmed) {
            return Err(mlua::Error::runtime(
                "cru.grammar.new: GBNF requires a 'root' rule",
            ));
        }
        Ok(LuaGrammar::new(Grammar::new(content)))
    })?;
    grammar.set("new", new_fn)?;

    // named(name: string, content: string) -> userdata
    let named_fn = lua.create_function(|_lua, (name, content): (String, String)| {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(mlua::Error::runtime(
                "cru.grammar.named: grammar content must be non-empty",
            ));
        }
        if !has_root_rule(trimmed) {
            return Err(mlua::Error::runtime(
                "cru.grammar.named: GBNF requires a 'root' rule",
            ));
        }
        Ok(LuaGrammar::new(Grammar::named(name, content)))
    })?;
    grammar.set("named", named_fn)?;

    // presets.*  Functions, not module-level constants — Lua plugins that
    // mutate fields on a "shared" preset would foot-gun themselves.
    let presets = lua.create_table()?;

    macro_rules! install_preset {
        ($name:expr, $fn:path) => {
            let f = lua.create_function(|_, ()| Ok(LuaGrammar::new($fn())))?;
            presets.set($name, f)?;
        };
    }

    install_preset!("yes_no", crucible_core::types::grammar::presets::yes_no);
    install_preset!("json", crucible_core::types::grammar::presets::json_object);
    install_preset!(
        "json_object",
        crucible_core::types::grammar::presets::json_object
    );
    install_preset!(
        "simple_tool_call",
        crucible_core::types::grammar::presets::simple_tool_call
    );
    install_preset!(
        "l0_l1_tools",
        crucible_core::types::grammar::presets::l0_l1_tools
    );
    install_preset!(
        "l0_l1_tools_with_thinking",
        crucible_core::types::grammar::presets::l0_l1_tools_with_thinking
    );
    install_preset!(
        "tool_or_prose",
        crucible_core::types::grammar::presets::tool_or_prose
    );

    grammar.set("presets", presets)?;
    Ok(())
}

/// Decode a Lua value into a [`Grammar`]. Accepts:
/// - a `LuaGrammar` userdata (the common case)
/// - a raw string (`cru.grammar.set_session_grammar(id, "root ::= ...")`)
fn value_to_grammar(v: &Value) -> Result<Grammar, String> {
    match v {
        Value::UserData(ud) => ud
            .borrow::<LuaGrammar>()
            .map(|g| g.grammar.clone())
            .map_err(|_| "grammar argument must be cru.grammar userdata or a string".to_string()),
        Value::String(s) => {
            let content = s
                .to_str()
                .map_err(|e| format!("invalid grammar string: {e}"))?
                .to_string();
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return Err("grammar string must be non-empty".to_string());
            }
            if !has_root_rule(trimmed) {
                return Err("GBNF requires a 'root' rule".to_string());
            }
            Ok(Grammar::new(content))
        }
        other => Err(format!(
            "grammar argument must be userdata or string, got {}",
            other.type_name()
        )),
    }
}

/// Cheap structural check: does any line look like a `root` rule
/// (`root ::= ...` or `root\s*::=`)? Not a full GBNF parser — that's
/// llama.cpp's job. This just catches the most common "I forgot the
/// root rule" mistake before it reaches the model.
fn has_root_rule(content: &str) -> bool {
    content
        .lines()
        .map(|l| l.trim_start())
        .any(|l| l.starts_with("root ") || l.starts_with("root\t") || l.starts_with("root::="))
}

/// Convenience alias matching the [`team`](super::team) module style.
pub type GrammarHandle = AnyUserData;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Records calls made through [`DaemonGrammarApi`] so tests can assert
    /// what the Lua bindings dispatched without spinning up the real daemon.
    struct StubGrammarApi {
        last_set: Mutex<Option<(String, Grammar)>>,
        last_clear: Mutex<Option<String>>,
        canned_get: Mutex<Option<Grammar>>,
        /// If `Some`, `set_session_grammar` returns this error instead of recording.
        force_error: Mutex<Option<String>>,
    }

    impl StubGrammarApi {
        fn new() -> Self {
            Self {
                last_set: Mutex::new(None),
                last_clear: Mutex::new(None),
                canned_get: Mutex::new(None),
                force_error: Mutex::new(None),
            }
        }

        fn with_canned_get(g: Grammar) -> Self {
            let s = Self::new();
            *s.canned_get.lock().unwrap() = Some(g);
            s
        }

        fn with_forced_error(err: &str) -> Self {
            let s = Self::new();
            *s.force_error.lock().unwrap() = Some(err.to_string());
            s
        }
    }

    impl DaemonGrammarApi for StubGrammarApi {
        fn set_session_grammar(
            &self,
            session_id: String,
            grammar: Grammar,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            let forced = self.force_error.lock().unwrap().clone();
            *self.last_set.lock().unwrap() = Some((session_id, grammar));
            Box::pin(async move {
                if let Some(e) = forced {
                    Err(e)
                } else {
                    Ok(())
                }
            })
        }

        fn clear_session_grammar(
            &self,
            session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            *self.last_clear.lock().unwrap() = Some(session_id);
            Box::pin(async { Ok(()) })
        }

        fn get_session_grammar(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Grammar>, String>> + Send>> {
            let canned = self.canned_get.lock().unwrap().clone();
            Box::pin(async move { Ok(canned) })
        }
    }

    fn make_lua_stub() -> Lua {
        let lua = Lua::new();
        register_grammar_module(&lua).expect("stub register");
        lua
    }

    fn make_lua_with_api(api: Arc<dyn DaemonGrammarApi>) -> Lua {
        let lua = Lua::new();
        register_grammar_module_with_api(&lua, api).expect("full register");
        lua
    }

    // ── constructors (pure) ────────────────────────────────────────────

    #[test]
    fn new_returns_userdata_for_valid_gbnf() {
        let lua = make_lua_stub();
        let ok: bool = lua
            .load(
                r#"
                local g = cru.grammar.new([[root ::= "yes" | "no"]])
                return type(g) == "userdata"
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn new_rejects_empty_grammar() {
        let lua = make_lua_stub();
        let err = lua
            .load(r#"return cru.grammar.new("")"#)
            .eval::<mlua::Value>()
            .unwrap_err();
        assert!(err.to_string().contains("non-empty"), "got: {err}");
    }

    #[test]
    fn new_rejects_grammar_without_root_rule() {
        let lua = make_lua_stub();
        let err = lua
            .load(r#"return cru.grammar.new("hello ::= 'hi'")"#)
            .eval::<mlua::Value>()
            .unwrap_err();
        assert!(err.to_string().contains("root"), "got: {err}");
    }

    #[test]
    fn named_attaches_name() {
        let lua = make_lua_stub();
        let name: Option<String> = lua
            .load(
                r#"
                local g = cru.grammar.named("yn", [[root ::= "yes"]])
                return g:name()
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(name.as_deref(), Some("yn"));
    }

    #[test]
    fn preset_json_returns_working_grammar() {
        let lua = make_lua_stub();
        let (content, name): (String, Option<String>) = lua
            .load(
                r#"
                local g = cru.grammar.presets.json()
                return g:to_string(), g:name()
                "#,
            )
            .eval()
            .unwrap();
        assert!(content.contains("root ::="), "got: {content}");
        assert_eq!(name.as_deref(), Some("json_object"));
    }

    #[test]
    fn preset_json_object_alias_returns_same_grammar() {
        let lua = make_lua_stub();
        let (a, b): (String, String) = lua
            .load(
                r#"
                return cru.grammar.presets.json():to_string(),
                       cru.grammar.presets.json_object():to_string()
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn presets_return_fresh_userdata_each_call() {
        // Lua plugins that mutate fields on a shared preset would foot-gun
        // themselves. Each call must return an independent userdata.
        let lua = make_lua_stub();
        let same_ref: bool = lua
            .load(
                r#"
                local a = cru.grammar.presets.yes_no()
                local b = cru.grammar.presets.yes_no()
                return rawequal(a, b)
                "#,
            )
            .eval()
            .unwrap();
        assert!(!same_ref);
    }

    #[test]
    fn all_advertised_presets_exist() {
        let lua = make_lua_stub();
        let count: usize = lua
            .load(
                r#"
                local n = 0
                for _, name in ipairs({
                    "yes_no", "json", "json_object",
                    "simple_tool_call", "l0_l1_tools",
                    "l0_l1_tools_with_thinking", "tool_or_prose",
                }) do
                    if type(cru.grammar.presets[name]) == "function" then
                        n = n + 1
                    end
                end
                return n
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 7);
    }

    // ── stub registration (no daemon) ──────────────────────────────────

    #[test]
    fn stub_set_returns_no_daemon_error() {
        let lua = make_lua_stub();
        let err: String = lua
            .load(
                r#"
                local g = cru.grammar.new([[root ::= "yes"]])
                local _, err = cru.grammar.set_session_grammar("sess", g)
                return err
                "#,
            )
            .eval()
            .unwrap();
        assert!(err.contains("no daemon connected"), "got: {err}");
    }

    #[test]
    fn registers_in_both_cru_and_crucible_namespaces() {
        let lua = make_lua_stub();
        let ok: bool = lua
            .load(
                r#"
                return type(cru.grammar.new) == "function"
                   and type(crucible.grammar.new) == "function"
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
    }

    // ── daemon-backed: set / get / clear ────────────────────────────────

    #[tokio::test]
    async fn set_then_get_roundtrip() {
        let api = Arc::new(StubGrammarApi::with_canned_get(Grammar::named(
            "yn",
            r#"root ::= "yes" | "no""#,
        )));
        let lua = make_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonGrammarApi>);

        let (ok, content, name): (bool, String, Option<String>) = lua
            .load(
                r#"
                local g = cru.grammar.new([[root ::= "yes" | "no"]])
                local ok, err = cru.grammar.set_session_grammar("sess-1", g)
                assert(ok, "set failed: " .. tostring(err))
                local back = cru.grammar.get_session_grammar("sess-1")
                return ok, back:to_string(), back:name()
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(ok);
        assert!(content.contains("yes"));
        assert_eq!(name.as_deref(), Some("yn"));

        let recorded = api.last_set.lock().unwrap().clone().unwrap();
        assert_eq!(recorded.0, "sess-1");
        assert!(recorded.1.content.contains("yes"));
    }

    #[tokio::test]
    async fn set_accepts_raw_string() {
        let api = Arc::new(StubGrammarApi::new());
        let lua = make_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonGrammarApi>);

        let ok: bool = lua
            .load(
                r#"
                local ok = cru.grammar.set_session_grammar("sid", [[root ::= "x"]])
                return ok == true
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(ok);
        let (_sid, grammar) = api.last_set.lock().unwrap().clone().unwrap();
        assert!(grammar.content.contains("root"));
    }

    #[tokio::test]
    async fn set_rejects_invalid_string() {
        let api = Arc::new(StubGrammarApi::new());
        let lua = make_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonGrammarApi>);

        let err: String = lua
            .load(
                r#"
                local _, e = cru.grammar.set_session_grammar("sid", "no rule here")
                return tostring(e)
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(err.contains("root"), "got: {err}");
        // And the stub must not have been called.
        assert!(api.last_set.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn set_surfaces_backend_unsupported_error() {
        let api = Arc::new(StubGrammarApi::with_forced_error(
            "backend openai does not support GBNF grammars",
        ));
        let lua = make_lua_with_api(api as Arc<dyn DaemonGrammarApi>);

        let err: String = lua
            .load(
                r#"
                local g = cru.grammar.presets.yes_no()
                local _, err = cru.grammar.set_session_grammar("sid", g)
                return err
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(err.contains("does not support"), "got: {err}");
    }

    #[tokio::test]
    async fn clear_records_session_id() {
        let api = Arc::new(StubGrammarApi::new());
        let lua = make_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonGrammarApi>);

        let ok: bool = lua
            .load(r#"return cru.grammar.clear_session_grammar("sid-2") == true"#)
            .eval_async()
            .await
            .unwrap();
        assert!(ok);
        assert_eq!(
            api.last_clear.lock().unwrap().clone(),
            Some("sid-2".to_string())
        );
    }

    #[tokio::test]
    async fn get_returns_nil_when_no_grammar_attached() {
        let api = Arc::new(StubGrammarApi::new());
        let lua = make_lua_with_api(api as Arc<dyn DaemonGrammarApi>);

        let nil: bool = lua
            .load(r#"return cru.grammar.get_session_grammar("sid") == nil"#)
            .eval_async()
            .await
            .unwrap();
        assert!(nil);
    }
}
