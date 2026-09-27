//! Review attribution of a Bases write that a plugin tool makes in a turn.

use super::*;

/// The plugin tool of the test. Its body writes through `cru.kiln`.
const TOOL: &str = "board_init";
const CALL_ID: &str = "plugin-tool";

/// A plugin tool that writes a base in the session's kiln is one tool call to
/// review: the hunk of the base carries the id of that call.
///
/// The turn loop brackets the call, and the Bases writer joins that bracket.
/// A second bracket of the writer on the same root would make both brackets
/// contested, and the hunk would then show as an external change.
#[tokio::test]
async fn a_bases_write_in_a_plugin_tool_is_attributed_to_that_tool_call() {
    let mut h = ReactorTestHarness::with_permissions(Some(PermissionConfig {
        default: PermissionMode::Allow,
        ..Default::default()
    }))
    .await;
    let root = h.workspace().to_path_buf();
    let sessions = h.agent_manager.session_manager().clone();
    let ctx = Arc::new(crate::rpc::RpcContext::for_test(
        Arc::new(KilnManager::new()),
        sessions.clone(),
        h.agent_manager.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            root.join("projects.json"),
        )),
        h.event_tx.clone(),
        root.clone(),
    ));
    ctx.kiln_registry
        .register_named(kiln_name("kiln"), &root)
        .expect("register kiln");

    let loader = crate::daemon_plugins::DaemonPluginLoader::new(HashMap::new()).expect("VM");
    let lua = loader.plugin_lua();
    crucible_lua::bases_api::register(&lua, Some(crate::bases::plugin_api::resolver(ctx.clone())))
        .expect("register cru.kiln");
    h.set_plugin_handlers(loader.plugin_handlers(), lua.clone());
    // The body names no session: the plugin tool acts for the session of
    // its call, which the session's dispatcher gives it.
    let func = lua
        .load(
            "return function() \
               return cru.kiln.ensure_base('kiln', {path='Board.base', yaml='views: []'}) \
             end",
        )
        .eval::<mlua::Function>()
        .expect("tool body");
    let registry = Arc::new(crate::plugin_tools::PluginRegistry::new());
    registry.register_plugin(
        "board",
        &lua,
        &[crucible_lua::DiscoveredTool {
            name: TOOL.to_string(),
            description: "writes a base".to_string(),
            params: Vec::new(),
            return_type: None,
            source_path: "board/init.lua".to_string(),
        }],
        &[],
        HashMap::from([(TOOL.to_string(), func)]),
        HashMap::new(),
    );
    h.agent_manager.set_plugin_tool_registry(registry);

    h.inject_streaming_agent(vec![
        script::tool_call(CALL_ID, TOOL, serde_json::json!({})),
        script::text("done"),
        script::done(),
    ]);
    h.send("make a board").await;
    let result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;
    assert!(
        result.data["result"]["error"].is_null(),
        "the plugin tool failed: {}",
        result.data
    );
    assert!(root.join("Board.base").exists(), "{}", result.data);

    let session = h.session_id.as_str();
    let hunks = ctx.agents.review.list_hunks(session).await.expect("hunks");
    let board: Vec<_> = hunks.iter().filter(|h| h.path == "Board.base").collect();
    assert!(!board.is_empty(), "no hunk for the base: {hunks:?}");
    for hunk in board {
        assert!(!hunk.is_external(), "the base write is external: {hunk:?}");
        assert_eq!(hunk.tool_call_ids, vec![CALL_ID.to_string()], "{hunk:?}");
    }
    let ledger = ctx.agents.review.ledger(session).expect("ledger");
    assert!(
        ledger
            .intervals()
            .iter()
            .all(|i| !i.tool_call_id.starts_with("bases_write")),
        "the Bases writer opened its own interval inside the tool call"
    );
}
