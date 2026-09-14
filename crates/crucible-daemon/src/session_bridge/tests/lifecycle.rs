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
        card_roots: Default::default(),
        review_snapshot_root: crate::test_support::scratch_snapshot_root(),
    }));
    let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        agent_manager,
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        event_tx,
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
            cru.on_session_end(function(_session)
                local aux, err = cru.session.create({ type = "chat" })
                if err or not aux then
                    _G.hook_error = "create: " .. tostring(err)
                    return
                end
                _G.hook_session_id = aux.id
                local _, cfg_err = cru.session.configure_agent(aux.id, {
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
            vec![crate::test_support::kiln_name("kiln")],
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

/// Session end sweeps the handlers that session activated, and leaves the
/// rest alone.
///
/// **The sweep is what makes activation-registers legal, not an
/// optimisation.** A plugin turned on for a session registers a row for it,
/// and the store has no unregister — so without this every session that ever
/// enabled a plugin leaves a row behind for the life of the daemon.
///
/// It crosses the crate boundary on purpose: `clear_session` is unit-tested in
/// `crucible-lua`, and what this pins is that `fire_session_end` calls it, and
/// calls it AFTER the end hooks. A `session:end` handler scoped to this
/// session is one of the rows swept, and it has to run first.
#[tokio::test]
async fn session_end_sweeps_the_handlers_that_session_activated() {
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
        card_roots: Default::default(),
        review_snapshot_root: crate::test_support::scratch_snapshot_root(),
    }));
    let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        agent_manager,
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        event_tx,
        tmp.path().to_path_buf(),
        plugin_loader.clone(),
    ));

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("plugin loader");
    let registry = loader.plugin_handlers();
    let plugin_lua = loader.plugin_lua();
    // One handler for every session, registered at load, and one activated
    // for the session that starts — the shape a workflow plugin has.
    loader
        .eval(
            r#"
            cru.on("turn:complete", function() end)
            cru.on_session_start(function(session)
                cru.on("turn:complete", { session = session.id, key = "ralph" }, function() end)
                -- Scoped too, so the sweep running first would take it and
                -- this hook would never run.
                cru.on_session_end(function(s)
                    _G.end_hook_ran = s.id
                end, { session = session.id, key = "ralph" })
            end)
            "#,
        )
        .await
        .expect("register the hooks");

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();

    crate::session_lifecycle::fire_start_hooks(&mut loader, None, &session_manager, &session.id)
        .await
        .expect("start hooks run");
    let scoped: Vec<_> = registry
        .all()
        .into_iter()
        .filter(|r| r.scope == crucible_lua::SessionScope::Session(session.id.to_string()))
        .collect();
    assert_eq!(scoped.len(), 2, "the start hook activated two handlers");

    *plugin_loader.lock().await = Some(loader);
    ctx.session_lifecycle.fire_session_end(&session.id).await;

    let end_hook_ran: Option<String> = plugin_lua.globals().get("end_hook_ran").unwrap();
    assert_eq!(
        end_hook_ran.as_deref(),
        Some(session.id.as_str()),
        "the end hooks must run before the sweep, not be swept away first"
    );
    let left = registry.all();
    assert!(
        !left
            .iter()
            .any(|r| r.scope == crucible_lua::SessionScope::Session(session.id.to_string())),
        "the session's own handler is gone"
    );
    assert!(
        left.iter()
            .any(|r| r.scope == crucible_lua::SessionScope::Global),
        "an unscoped handler belongs to the load, not the session, and survives"
    );
}

/// Session end forgets the session's statusline expression values.
///
/// Same leak as the handler sweep above, one store over: the registry is keyed
/// by session and had no production release, so every session that ever set an
/// expression kept its map for the daemon's life.
///
/// It crosses the crate boundary for the same reason, and it pins the ORDERING
/// the other way round: the end hook here SETS a value, so a sweep placed
/// before the hooks would let the hook put the map straight back.
#[tokio::test]
async fn session_end_forgets_the_sessions_statusline_values() {
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
        card_roots: Default::default(),
        review_snapshot_root: crate::test_support::scratch_snapshot_root(),
    }));
    let exprs = agent_manager.statusline_exprs();
    let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        Arc::clone(&agent_manager),
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        event_tx,
        tmp.path().to_path_buf(),
        plugin_loader.clone(),
    ));

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("plugin loader");
    // The same registry the manager hands every client, so what the hook writes
    // is what a client would draw.
    loader
        .register_statusline_exprs(agent_manager.statusline_exprs())
        .expect("bind cru.statusline");
    loader
        .eval(
            r#"
            cru.on_session_start(function(session)
                cru.statusline.set(session.id, "phase", "running")
                cru.on_session_end(function(s)
                    -- A value written by the END hook: sweeping before the
                    -- hooks would leave exactly this behind.
                    cru.statusline.set(s.id, "phase", "shutting down")
                end, { session = session.id, key = "phase" })
            end)
            "#,
        )
        .await
        .expect("register the hooks");

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();

    crate::session_lifecycle::fire_start_hooks(&mut loader, None, &session_manager, &session.id)
        .await
        .expect("start hooks run");
    assert_eq!(
        exprs.snapshot(&session.id).get("phase").map(String::as_str),
        Some("running"),
        "the start hook's value must reach the registry the clients read"
    );

    *plugin_loader.lock().await = Some(loader);
    ctx.session_lifecycle.fire_session_end(&session.id).await;

    assert!(
        exprs.snapshot(&session.id).is_empty(),
        "the session's expression map must not outlive the session: {:?}",
        exprs.snapshot(&session.id)
    );
}
