//! Undo is refused on a session an external ACP agent runs.
//!
//! The daemon rewinds its own conversation tree. The external agent keeps its
//! history in its own process and no ACP method rewinds it, so a rewind moves
//! one history and not the other: the transcript loses the turn, the agent
//! still answers from it, and the user reads a reply that contradicts what is
//! on screen. Every entry point reported success for that.
//!
//! Three entry points reach one gate — `AgentManager::undo` — so all three are
//! asserted here: the RPC dispatch handler, the Lua bridge, and the manager
//! itself. The TUI `:undo` command sends `session.undo` over RPC, so the
//! handler test is its crossing too.

use super::*;
use crucible_lua::register_sessions_module_with_api;

/// A session with two complete turns on its tree, run by `agent_type`.
async fn session_with_two_turns(
    agent_type: &str,
) -> (Arc<AgentManager>, Arc<SessionManager>, String, TempDir) {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager_with_kilns(&[("kiln", tmp.path())]);
    let am = create_test_agent_manager(sm.clone());
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![kiln_name("kiln")],
            Some(tmp.path().into()),
            None,
        )
        .await
        .unwrap();
    let mut agent = test_agent();
    agent.agent_type = agent_type.to_string();
    am.configure_agent(&session.id, agent).await.unwrap();

    let tree = am
        .get_or_rebuild_session_tree(&session.id, std::path::Path::new("/nonexistent"))
        .await;
    {
        let mut t = tree.lock().await;
        let root = t.root();
        let u1 = t.add_child_and_advance(
            root,
            crucible_core::turn::NodeContent::User { text: "u1".into() },
        );
        let a1 = t.add_child_and_advance(
            u1,
            crucible_core::turn::NodeContent::Agent { text: "a1".into() },
        );
        let u2 = t.add_child_and_advance(
            a1,
            crucible_core::turn::NodeContent::User { text: "u2".into() },
        );
        t.add_child_and_advance(
            u2,
            crucible_core::turn::NodeContent::Agent { text: "a2".into() },
        );
    }
    let id = session.id.to_string();
    (am, sm, id, tmp)
}

/// The refusal names the agent and the reason. A bare "not supported" sends
/// the user looking for a setting to turn on.
#[tokio::test]
async fn undo_on_an_acp_session_is_refused_with_its_reason() {
    let (am, _sm, id, _tmp) = session_with_two_turns("acp").await;
    let (tx, _rx) = broadcast::channel(16);

    let error = am
        .undo(&id, 1, Some(&tx))
        .await
        .expect_err("an external agent's history is not Crucible's to rewind");

    let message = error.to_string();
    assert!(
        message.contains("external ACP agent") && message.contains("history"),
        "the refusal must name the agent and the reason: {message}"
    );
}

/// The tree still holds the turns, so the reading half must not answer from
/// it: a client that enables its undo control from `can_undo` would offer a
/// command that always fails.
#[tokio::test]
async fn the_undo_readers_report_nothing_to_undo_on_an_acp_session() {
    let (am, _sm, id, _tmp) = session_with_two_turns("acp").await;

    assert!(!am.can_undo(&id).await.unwrap());
    assert_eq!(am.undo_depth(&id).await.unwrap(), 0);
    assert!(am.undo_history(&id).await.unwrap().is_empty());
}

/// The same fixture with an internal agent still undoes, so the gate reads the
/// agent type and not something every session shares.
#[tokio::test]
async fn undo_on_an_internal_session_still_rewinds_a_turn() {
    let (am, _sm, id, _tmp) = session_with_two_turns("internal").await;
    let (tx, _rx) = broadcast::channel(16);

    assert!(am.can_undo(&id).await.unwrap());
    assert_eq!(am.undo_depth(&id).await.unwrap(), 2);
    let undone = am.undo(&id, 1, Some(&tx)).await.expect("an internal undo");
    assert_eq!(undone.len(), 1);
}

/// `session.undo` over RPC — the method the TUI's `:undo` and `/undo` send —
/// must carry the reason to the client, not an empty success.
#[tokio::test]
async fn the_undo_rpc_carries_the_refusal_to_the_client() {
    let (am, _sm, id, _tmp) = session_with_two_turns("acp").await;
    let (tx, _rx) = broadcast::channel(16);

    let response = crate::server::session::handle_session_undo(
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session.undo",
            "params": { "session_id": id },
            "id": 1,
        }))
        .unwrap(),
        &am,
        &tx,
    )
    .await;

    assert!(
        response.result.is_none(),
        "a refused undo must not answer with a result: {:?}",
        response.result
    );
    let error = response.error.expect("an error body");
    // A refusal is the caller's request, not a daemon fault.
    assert_eq!(error.code, crate::protocol::INVALID_PARAMS, "{error:?}");
    let message = error.message;
    assert!(
        message.contains("external ACP agent"),
        "the RPC error must name the reason: {message}"
    );
}

/// `cru.session.undo` crosses into Lua, so the refusal has to survive the
/// crossing rather than arrive as a bare `nil`.
#[tokio::test]
async fn the_lua_bridge_refuses_undo_with_the_same_reason() {
    let (am, sm, id, tmp) = session_with_two_turns("acp").await;
    let (tx, _rx) = broadcast::channel(16);
    let ctx = Arc::new(crate::rpc::RpcContext::for_test(
        am.kiln_manager.clone(),
        sm.clone(),
        am.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        tx,
        tmp.path().into(),
    ));
    let lua = mlua::Lua::new();
    register_sessions_module_with_api(
        &lua,
        Arc::new(crate::session_bridge::DaemonSessionBridge::new(ctx)),
    )
    .unwrap();
    lua.globals().set("session_id", id).unwrap();

    let reason: String = lua
        .load("local count, err = cru.session.undo(session_id); assert(count == nil, 'undo must not report a turn count'); return err")
        .eval_async()
        .await
        .unwrap();

    assert!(
        reason.contains("external ACP agent"),
        "Lua must read the reason: {reason}"
    );
}
