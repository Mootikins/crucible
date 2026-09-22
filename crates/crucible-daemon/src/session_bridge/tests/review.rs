//! The plugin review surface: `cru.diff.*` and the proposal decisions run
//! the `diff.*` and `proposal.*` handlers through the bridge. The refusal
//! that a plugin sees for bad params is pinned here, from Lua, and a
//! decision on a child proposal writes the note.

use super::*;
use crucible_core::file_write::ExpectedBase;
use crucible_core::proposal::{ProposalAuthor, ProposalState};
use crucible_core::session::{PhysicalRoot, SessionId};
use crucible_lua::{register_sessions_module_with_api, DaemonSessionApi, ProposalDecision};

/// A bridge over a fresh session manager, and its context.
fn bridge_over(tmp: &std::path::Path) -> (Arc<RpcContext>, Arc<dyn DaemonSessionApi>) {
    let session_manager = temp_session_manager();
    let agent_manager = build_test_agent_manager(session_manager.clone());
    let (event_tx, _) = broadcast::channel(16);
    let ctx = bridge_ctx(session_manager, agent_manager, event_tx, tmp);
    let bridge: Arc<dyn DaemonSessionApi> = Arc::new(DaemonSessionBridge::new(ctx.clone()));
    (ctx, bridge)
}

/// A Lua VM whose `cru.diff` talks to a bridge. The params parse runs
/// before any session lookup, so no session exists.
fn lua_over_bridge(tmp: &std::path::Path) -> mlua::Lua {
    let (_, bridge) = bridge_over(tmp);
    let lua = mlua::Lua::new();
    register_sessions_module_with_api(&lua, bridge).expect("daemon-backed sessions module");
    lua
}

async fn diff_comment_error(lua: &mlua::Lua, params: &str) -> String {
    let (value, err): (mlua::Value, Option<String>) = lua
        .load(format!("return cru.diff.comment({params})"))
        .eval_async()
        .await
        .expect("the call returns (nil, err) rather than raising");
    assert!(matches!(value, mlua::Value::Nil), "got a value: {value:?}");
    err.expect("bad params report an error")
}

/// Params with a missing required field name that field, so a plugin author
/// can read which key to add. The text is the handler's, pinned so a rename
/// of the wire field shows up here. `side` and `author` have defaults on
/// this path, so they are never missing.
#[tokio::test]
async fn a_missing_required_field_is_named_in_the_error() {
    let tmp = TempDir::new().unwrap();
    let lua = lua_over_bridge(tmp.path());
    let source = r#"source = { kind = "session_record", session = "no-such-session" }"#;

    let cases = [
        (
            format!(r#"{{ {source}, body = "b", line_start = 1 }}"#),
            "invalid params: missing field `path`",
        ),
        (
            format!(r#"{{ {source}, path = "src/a.rs", line_start = 1 }}"#),
            "invalid params: missing field `body`",
        ),
        (
            format!(r#"{{ {source}, path = "src/a.rs", body = "b" }}"#),
            "invalid params: missing field `line_start`",
        ),
    ];
    for (params, expected) in cases {
        let err = diff_comment_error(&lua, &params).await;
        assert_eq!(err, expected, "params {params}");
    }
}

/// Params that are not a table fail before the handler, with the bridge's
/// own text.
#[tokio::test]
async fn non_table_params_are_refused_by_name() {
    let tmp = TempDir::new().unwrap();
    let lua = lua_over_bridge(tmp.path());

    let err = diff_comment_error(&lua, r#""not a table""#).await;
    assert_eq!(err, "diff.comment params must be a table");
}

/// A delegated child proposes two notes in one turn. The bridge lists the
/// proposal under the child only, accepts one path, and rejects the other
/// with a reason. The accepted note is on disk, and the rejected one is not.
#[tokio::test]
async fn the_bridge_lists_and_decides_a_child_proposal() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    let kiln = kiln.canonicalize().unwrap();
    let (ctx, bridge) = bridge_over(tmp.path());
    ctx.kiln.get_or_open(&kiln).await.expect("open the kiln");

    let child = SessionId::parse("chat-child-1").unwrap();
    let store = ctx.agents.proposals();
    for path in ["a.md", "b.md"] {
        store
            .record_write(
                ProposalAuthor::Session { id: child.clone() },
                &child,
                PhysicalRoot::from_top_level(&kiln),
                path,
                ExpectedBase::Absent,
                format!("{path} text\n"),
            )
            .unwrap();
    }

    let listed = bridge
        .list_proposals(Some(child.to_string()), false)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    let id = listed[0]["id"].as_str().unwrap().to_string();
    assert!(bridge
        .list_proposals(Some("chat-other".into()), false)
        .await
        .unwrap()
        .is_empty());

    let accepted = bridge
        .decide_proposal(
            ProposalDecision::Accept,
            serde_json::json!({ "id": id, "paths": ["a.md"] }),
        )
        .await
        .unwrap();
    assert_eq!(accepted["state"]["kind"], "accepted", "{accepted}");
    assert_eq!(
        std::fs::read_to_string(kiln.join("a.md")).unwrap(),
        "a.md text\n"
    );

    let rejected = bridge
        .decide_proposal(
            ProposalDecision::Reject,
            serde_json::json!({ "id": id, "reason": "not this one" }),
        )
        .await
        .unwrap();
    let state: ProposalState = serde_json::from_value(rejected["state"].clone()).unwrap();
    assert_eq!(
        state,
        ProposalState::Rejected {
            reason: Some("not this one".into())
        }
    );
    assert!(!kiln.join("b.md").exists());
    assert!(bridge
        .list_proposals(Some(child.to_string()), false)
        .await
        .unwrap()
        .is_empty());
}
