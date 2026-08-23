//! The plugin review surface: `cru.sessions.review_comment` reads its spec
//! through the bridge's `parse_comment_spec`, so the serde error a plugin
//! sees for a bad spec is pinned here, from Lua, not from the parser alone.

use super::*;
use crucible_lua::{register_sessions_module_with_api, DaemonSessionApi};

/// A Lua VM whose `cru.sessions` talks to a bridge over a fresh session
/// manager. The spec parse runs before any session lookup, so no session
/// exists.
fn lua_over_bridge(tmp: &std::path::Path) -> mlua::Lua {
    let session_manager = temp_session_manager();
    let agent_manager = build_test_agent_manager(session_manager.clone());
    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(session_manager, agent_manager, event_tx, tmp);
    let bridge: Arc<dyn DaemonSessionApi> = Arc::new(DaemonSessionBridge::new(ctx));
    let lua = mlua::Lua::new();
    register_sessions_module_with_api(&lua, bridge).expect("daemon-backed sessions module");
    lua
}

async fn review_comment_error(lua: &mlua::Lua, spec: &str) -> String {
    let (value, err): (mlua::Value, Option<String>) = lua
        .load(format!(
            r#"return cru.sessions.review_comment("no-such-session", {spec})"#
        ))
        .eval_async()
        .await
        .expect("the call returns (nil, err) rather than raising");
    assert!(matches!(value, mlua::Value::Nil), "got a value: {value:?}");
    err.expect("a bad spec reports an error")
}

/// A spec with a missing required field names that field, so a plugin author
/// can read which key to add. The text is serde's, pinned so a rename of the
/// wire field or a move of the parse shows up here.
#[tokio::test]
async fn a_missing_required_field_is_named_in_the_error() {
    let tmp = TempDir::new().unwrap();
    let lua = lua_over_bridge(tmp.path());

    let cases = [
        (r#"{ body = "b", line_start = 1 }"#, "missing field `path`"),
        (
            r#"{ path = "src/a.rs", line_start = 1 }"#,
            "missing field `body`",
        ),
        (
            r#"{ path = "src/a.rs", body = "b" }"#,
            "missing field `line_start`",
        ),
    ];
    for (spec, expected) in cases {
        let err = review_comment_error(&lua, spec).await;
        assert_eq!(err, expected, "spec {spec}");
    }
}

/// A spec that is not a table fails before serde, with the bridge's own text.
#[tokio::test]
async fn a_non_table_spec_is_refused_by_name() {
    let tmp = TempDir::new().unwrap();
    let lua = lua_over_bridge(tmp.path());

    let err = review_comment_error(&lua, r#""not a table""#).await;
    assert_eq!(err, "comment spec must be a table");
}
