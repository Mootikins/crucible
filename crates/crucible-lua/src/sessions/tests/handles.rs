use super::MockDaemonApi;
use crate::sessions::{DaemonSessionApi, Session};
use crate::test_support::TestLuaBuilder;
use mlua::{Lua, Value};
use std::sync::Arc;

/// A handle's method runs the shared operation body with the handle's own
/// id — the whole point of a handle — and answers in the same
/// `(value, nil) | (nil, err)` shape the free function uses.
#[tokio::test]
async fn a_handle_method_sends_with_the_handles_own_id() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (response_id, err): (String, Value) = lua
        .load(
            r#"
            local s, cerr = cru.session.create({ type = "chat" })
            assert(cerr == nil, "unexpected error: " .. tostring(cerr))
            local rid, err = s:send_message("hello")
            return rid, err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(err, Value::Nil), "unexpected error: {err:?}");
    assert_eq!(response_id, "msg-response-001");
    // The id the mock saw is the id the handle was built from, not one the
    // caller had to repeat.
    let sends = mock.send_calls();
    assert_eq!(sends.len(), 1);
    assert!(sends[0].starts_with("chat-"));
}

/// One lifecycle verb and one review verb through the handle, proving the
/// macro-registered methods exist across the surface, not just the first.
#[tokio::test]
async fn handle_methods_cover_lifecycle_and_review_verbs() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let ok: bool = lua
        .load(
            r#"
            local s = cru.session.get("exists-123")
            local ok1 = s:end_session()
            local hunks = s:review_list_hunks()
            return ok1 and #hunks == 0
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(ok);
    assert_eq!(mock.end_calls(), vec!["exists-123".to_string()]);
    assert_eq!(mock.review_list_calls(), vec!["exists-123".to_string()]);
}

/// A handle with no API behind it — the bare-executor case — reports that it
/// is not connected rather than silently doing nothing.
#[tokio::test]
async fn an_unconnected_handle_method_reports_it() {
    let lua = TestLuaBuilder::new().build();

    let bare = Session::new("s1".to_string());
    lua.globals()
        .set("bare_session", lua.create_userdata(bare).unwrap())
        .unwrap();

    let err: String = lua
        .load(
            r#"
            local _, err = bare_session:send_message("hello")
            return err
            "#,
        )
        .eval()
        .unwrap();

    assert!(
        err.contains("not connected"),
        "expected a not-connected error, got: {err}"
    );
}

/// A handle from `get` has no live RPC behind it, so `model` must read the
/// daemon's record. Without this the reflection plugin, which reads
/// `cru.session.get(id).model` on the session that ended, raised
/// "Session not connected" and wrote no note.
#[tokio::test]
async fn model_on_a_get_handle_reads_the_record() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let model: String = lua
        .load(
            r#"
            local s = cru.session.get("exists-123")
            return s.model
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(model, "claude-haiku-4-5-20251001");
}

/// A handle from `get` answers `workspace` and `isolation` from the record.
/// The handle reads both from its own fields, so before the fix they read as
/// nil, and a plugin could not start a session like the one it read.
#[tokio::test]
async fn workspace_and_isolation_on_a_get_handle_read_the_record() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (workspace, image, bare_workspace): (Option<String>, Option<String>, bool) = lua
        .load(
            r#"
            local s = cru.session.get("isolated-123")
            local bare = cru.session.get("exists-123")
            return s.workspace, s.isolation and s.isolation.image,
                bare.workspace == nil and bare.isolation == nil
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(workspace.as_deref(), Some("/work/project"));
    assert_eq!(image.as_deref(), Some("alpine"));
    assert!(
        bare_workspace,
        "a record with neither field reads both as nil"
    );
}

/// A handle from `get` names the plugin that created the session, and a
/// session that no plugin created reads `plugin` as nil, not as an unknown
/// property. The reflection pass reads it on every session.
#[tokio::test]
async fn plugin_on_a_get_handle_names_the_creating_plugin_or_nil() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (plugin, bare_is_nil): (Option<String>, bool) = lua
        .load(
            r#"
            local s = cru.session.get("isolated-123")
            local bare = cru.session.get("exists-123")
            return s.plugin, bare.plugin == nil
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(plugin.as_deref(), Some("discord"));
    assert!(bare_is_nil);
}

/// A record with no model reads as nil, the same answer a bound handle
/// gives when the daemon has no model for the session.
#[tokio::test]
async fn model_on_a_get_handle_is_nil_when_the_record_has_none() {
    let lua = TestLuaBuilder::new().build();

    let bare = Session::new("s1".to_string())
        .with_record(serde_json::json!({ "id": "s1", "model": null }));
    lua.globals()
        .set("bare_session", lua.create_userdata(bare).unwrap())
        .unwrap();

    let is_nil: bool = lua.load("return bare_session.model == nil").eval().unwrap();
    assert!(is_nil);
}

/// The two verbs a plugin needs to prepare a session it created: the mode it
/// runs its turn in, and the title a human reads in the sessions list.
///
/// Both go through the handle, because the id is the handle's. A
/// `NewIndex` setter (`s.mode = "auto"`) cannot serve here: a handle from
/// `create` binds no `SessionConfigRpc`, so the assignment answers
/// "Session not connected".
#[tokio::test]
async fn a_handle_sets_the_mode_and_the_title_of_its_own_session() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let ok: bool = lua
        .load(
            r#"
            local s, cerr = cru.session.create({ type = "plugin" })
            assert(cerr == nil, "unexpected error: " .. tostring(cerr))
            local m, merr = s:set_mode("auto")
            assert(merr == nil, "set_mode: " .. tostring(merr))
            local t, terr = s:set_title("Reflection: yesterday")
            assert(terr == nil, "set_title: " .. tostring(terr))
            return m and t
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(ok, "both verbs answer true");
    let modes = mock.mode_calls();
    assert_eq!(modes.len(), 1);
    assert!(modes[0].0.starts_with("plugin-"), "{:?}", modes[0]);
    assert_eq!(modes[0].1, "auto");
    let titles = mock.title_calls();
    assert_eq!(titles.len(), 1);
    assert_eq!(titles[0].0, modes[0].0, "one session, one id");
    assert_eq!(titles[0].1, "Reflection: yesterday");
}

/// The free function and the handle method call one body, so
/// `cru.session.set_title(id, t)` reaches the same daemon call with the id
/// the caller named.
#[tokio::test]
async fn the_free_functions_set_the_mode_and_the_title_by_id() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let ok: bool = lua
        .load(
            r#"
            local m = cru.session.set_mode("exists-123", "plan")
            local t = cru.session.set_title("exists-123", "T")
            return m and t
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(ok);
    assert_eq!(
        mock.mode_calls(),
        vec![("exists-123".to_string(), "plan".to_string())]
    );
    assert_eq!(
        mock.title_calls(),
        vec![("exists-123".to_string(), "T".to_string())]
    );
}

/// A daemon refusal is the `(nil, err)` pair every other session function
/// answers with, not a raise.
#[tokio::test]
async fn an_unknown_mode_answers_the_error_pair() {
    let mock = Arc::new(MockDaemonApi::new());
    mock.refuse_mode("unknown mode 'zoom'. Valid: normal, plan, auto");
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (value, err): (Value, String) = lua
        .load(
            r#"
            local ok, err = cru.session.set_mode("exists-123", "zoom")
            return ok, err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(value, Value::Nil), "{value:?}");
    assert!(err.contains("unknown mode 'zoom'"), "{err}");
}

/// A session handle carrying the record `session_json` builds.
fn handle_with_record() -> Session {
    Session::new("chat-test".to_string()).with_record(serde_json::json!({
        "id": "chat-test",
        "session_type": "chat",
        "kilns": ["Crucible Help"],
        "state": "Active",
        "title": "A session",
        "model": "claude-sonnet-5",
        "started_at": "2026-09-16T00:00:00Z",
        "event_count": 3,
    }))
}

/// Reading a name the record does not carry must come back as an
/// `mlua::Error`, not as a panic and not as a dead process.
///
/// This is the shape that took the daemon down. `session-board` read
/// `s.agent_model`, the record spells it `model`, the `Index` metamethod
/// answered `Err(mlua::Error::runtime(..))` exactly as it should — and the
/// release build died, because Luau raises that `Err` by throwing out of
/// this very callback and the profile said `panic = "abort"`. The `Err`
/// was never the bug. The profile was, and `lib.rs` now refuses to
/// compile under it.
///
/// `catch_unwind` is the point of the test, not decoration: it is what
/// separates "returned an error" from "unwound out of the callback", and
/// the two read identically to `is_err()`.
#[test]
fn an_unknown_property_is_an_error_and_not_a_panic() {
    let outcome = std::panic::catch_unwind(|| {
        let lua = Lua::new();
        let ud = lua.create_userdata(handle_with_record()).unwrap();
        lua.globals().set("s", ud).unwrap();
        lua.load("return s.agent_model").eval::<mlua::Value>()
    });

    let result = outcome.expect("reading an unknown property must not panic");
    let err = result.expect_err("an unknown property must not read as nil");
    assert!(
        err.to_string().contains("unknown property: agent_model"),
        "the error must name the property that was not found, got: {err}"
    );
}

/// The names the record does carry still read, so the gate above is a
/// gate and not a wall.
#[test]
fn every_name_the_record_carries_reads_back() {
    let lua = Lua::new();
    let ud = lua.create_userdata(handle_with_record()).unwrap();
    lua.globals().set("s", ud).unwrap();

    for (expr, expected) in [
        ("s.id", "chat-test"),
        ("s.session_type", "chat"),
        ("s.state", "Active"),
        ("s.title", "A session"),
        ("s.model", "claude-sonnet-5"),
    ] {
        let got: String = lua
            .load(format!("return {expr}"))
            .eval()
            .unwrap_or_else(|e| panic!("{expr} must read: {e}"));
        assert_eq!(got, expected, "{expr}");
    }
}
