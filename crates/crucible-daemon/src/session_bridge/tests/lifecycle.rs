//! Plugin lifecycle re-entrancy: what a plugin hook may do while the daemon is
//! still inside it.
//!
//! Its own module because the constraint is not about the create *surface* —
//! it is about who holds the plugin-loader mutex — and because the rig is a
//! live `DaemonPluginLoader` rather than the bridge fixtures next door.

use super::*;

/// A plugin that creates a session from inside `on_session_end` must not hang
/// the daemon.
///
/// `SessionLifecycle::fire_session_end` holds the plugin-loader mutex across
/// the whole Lua call, and tokio's `Mutex` is not reentrant — so any step the
/// bridge's create path takes that re-locks that handle deadlocks rather than
/// failing. The reflection plugin is exactly this shape (`on_session_end` →
/// create an aux session → configure its agent, `reflection/init.lua`), and
/// `enforce_session_start` is exactly such a step, which is why it stays at the
/// RPC layer instead of moving into `create_session_resolved`. This is the test
/// that catches someone moving it.
///
/// The loader handle is shared with the `AgentManager`, as `server::bind` wires
/// it: the manager locks it for `plugin_lua`/`plugin_registry`, so the second
/// half of the hook (`configure_agent`) is on the same hook as the first.
#[tokio::test]
async fn a_plugin_creating_a_session_from_on_session_end_does_not_deadlock() {
    use crate::daemon_plugins::DaemonPluginLoader;

    let tmp = TempDir::new().unwrap();
    let plugin_loader: Arc<tokio::sync::Mutex<Option<DaemonPluginLoader>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    let session_manager = temp_session_manager();
    let (event_tx, _keep_open) = broadcast::channel(64);
    let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: Some(bridge_llm_config()),
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: Some(plugin_loader.clone()),
    }));
    let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        agent_manager,
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        event_tx,
        Some(bridge_llm_config()),
        tmp.path().to_path_buf(),
        plugin_loader.clone(),
    ));

    let loader = DaemonPluginLoader::new(HashMap::new()).expect("plugin loader");
    loader
        .upgrade_with_sessions(Arc::new(DaemonSessionBridge::new(ctx.clone())))
        .expect("wire the bridge into the plugin VM");
    // Read back after the hook has run: a hang and a silent error both leave
    // the assertions below unmet, and only the globals say which.
    let plugin_lua = loader.plugin_lua();
    loader
        .eval(
            r#"
            crucible.on_session_end(function(_session)
                local aux, err = cru.sessions.create({ type = "chat" })
                if err or not aux then
                    _G.hook_error = "create: " .. tostring(err)
                    return
                end
                _G.hook_session_id = aux.id
                local _, cfg_err = cru.sessions.configure_agent(aux.id, {
                    agent_type = "internal",
                    provider = "ollama",
                    provider_key = "ollama",
                    model = "llama3.2",
                    system_prompt = "reflect",
                })
                if cfg_err then
                    _G.hook_error = "configure_agent: " .. tostring(cfg_err)
                end
            end)
            "#,
        )
        .await
        .expect("register the on_session_end hook");
    *plugin_loader.lock().await = Some(loader);

    let ending = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    tokio::time::timeout(
        Duration::from_secs(30),
        ctx.session_lifecycle.fire_session_end(&ending.id),
    )
    .await
    .expect("fire_session_end deadlocked on the plugin loader mutex");

    let hook_error: Option<String> = plugin_lua.globals().get("hook_error").unwrap();
    assert_eq!(hook_error, None, "the hook itself failed");
    let aux_id: String = plugin_lua
        .globals()
        .get("hook_session_id")
        .expect("the hook created a session");
    // Not just "it returned": the create ran the daemon's real path, so the
    // session is registered and its agent configured.
    let aux = session_manager
        .get_session(&aux_id)
        .expect("aux registered");
    assert!(
        aux.kilns.is_empty(),
        "a kiln-less create attaches no kiln — not the data root, which encloses \
         every transcript the daemon has written: {:?}",
        aux.kilns
    );
    assert_eq!(
        aux.agent.expect("hook configured the agent").model,
        "llama3.2"
    );
}
