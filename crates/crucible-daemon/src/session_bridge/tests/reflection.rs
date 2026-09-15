//! The shipped plugin, not a replacement hook: end → provider → note → review.

use super::*;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::observe::LogEvent;
use crate::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_core::session::SessionState;
use crucible_lua::{manifest::PluginSource, DaemonSessionApi};
use serde_json::json;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn shipped_reflection_writes_a_reviewable_note_on_session_end() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = MockServer::start().await;
    let note = "# Remember\n\nUse the daemon as the only storage owner.\n";
    Mock::given(method("POST"))
        .respond_with(move |request: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let has_result = body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
            let message = if has_result {
                json!({"role": "assistant", "content": "Saved one lesson for review."})
            } else {
                json!({"role": "assistant", "content": "", "tool_calls": [{"id": "lesson", "function": {
                    "name": "create_note", "arguments": {"path": "Remember.md", "content": note}
                }}]})
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/x-ndjson")
                .set_body_string(format!("{}\n{}\n", json!({"model": "reflection-fixture", "message": message, "done": false}), json!({"done": true, "done_reason": "stop"})))
        })
        .mount(&provider)
        .await;

    let tmp = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let sessions = temp_session_manager_with_kilns(&[("knowledge", kiln.path())]);
    let shared_loader = Arc::new(tokio::sync::Mutex::new(None));
    let (events, mut observed) = broadcast::channel(128);
    let kilns = Arc::new(KilnManager::with_event_tx(
        events.clone(),
        Some(crucible_core::config::EmbeddingProviderConfig::mock(Some(
            384,
        ))),
        crucible_core::config::default_max_precognition_chars(),
    ));
    let mut llm = bridge_llm_config();
    llm.providers.get_mut("ollama").unwrap().endpoint = Some(provider.uri());
    let mut loader = DaemonPluginLoader::new(HashMap::new()).unwrap();
    let lua = loader.plugin_lua();
    let previous = crucible_lua::set_source(&lua, crucible_lua::LuaSource::Builtin);
    lua.load(crucible_lua::BUILTIN_INIT_LUA).exec().unwrap();
    crucible_lua::set_source(&lua, previous);
    let agents = Arc::new(
        AgentManager::new(AgentManagerParams {
            kiln_manager: kilns.clone(),
            session_manager: sessions.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(events.clone())),
            mcp_gateway: None,
            llm_config: Some(llm),
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: Some(shared_loader.clone()),
            card_roots: Default::default(),
            review_snapshot_root: tmp.path().join("snapshots"),
        })
        .with_modes(Some(loader.mode_registry())),
    );
    let ctx = Arc::new(RpcContext::for_test_with_plugin_loader(
        kilns,
        sessions.clone(),
        agents.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            tmp.path().join("projects.json"),
        )),
        events,
        tmp.path().into(),
        shared_loader.clone(),
    ));
    let bridge = Arc::new(DaemonSessionBridge::new(ctx.clone()));
    loader.upgrade_with_sessions(bridge.clone()).unwrap();
    loader
        .upgrade_with_tools(Arc::new(
            crate::tools_bridge::DaemonToolsBridge::new(
                Arc::new(crate::tools::workspace::WorkspaceTools::new(tmp.path())),
                None,
            )
            .with_active_tools(agents.active_tools(), sessions.clone()),
        ))
        .unwrap();
    agents.set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());
    agents.set_daemon_permissions(loader.permission_registry());
    agents.set_plugin_tool_registry(loader.plugin_registry());
    agents.set_isolation(loader.isolation());
    loader
        .add_plugin_paths(&[(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/plugins"),
            PluginSource::Runtime,
        )])
        .unwrap();
    loader.activate_plugin("reflection").await.unwrap();
    loader.eval(r#"require("reflection").setup({ model = "reflection-fixture", min_turns = 1, timeout = 5 })"#).await.unwrap();
    *shared_loader.lock().await = Some(loader);

    let source = sessions
        .create_session(
            SessionType::Chat,
            vec![kiln_name("knowledge")],
            Some(tmp.path().into()),
            None,
        )
        .await
        .unwrap();
    for event in [
        LogEvent::user("What did we learn?"),
        LogEvent::assistant("Keep storage in the daemon."),
    ] {
        sessions
            .storage()
            .append_event(&source, &event.to_jsonl().unwrap())
            .await
            .unwrap();
    }
    tokio::time::timeout(
        Duration::from_secs(10),
        ctx.session_lifecycle.fire_session_end(&source.id),
    )
    .await
    .expect("reflection end hook completes without re-entering the loader lock");

    let passes: Vec<_> = sessions
        .list_sessions()
        .into_iter()
        .filter(|s| s.session_type == SessionType::Plugin)
        .collect();
    assert_eq!(
        passes.len(),
        1,
        "one source end produces one reflection pass"
    );
    let pass = sessions.get_session(&passes[0].id).unwrap();
    assert_eq!(
        pass.state,
        SessionState::Ended,
        "the aux session is ended on completion"
    );
    assert_eq!(pass.workspace, None, "a reviewer owns no project workspace");
    assert_eq!(pass.kilns, source.kilns);
    let config = pass.agent.unwrap();
    assert_eq!(config.model, "reflection-fixture");
    assert_eq!(config.mode.as_deref(), Some("auto"));
    assert!(config.mcp_servers.is_empty());
    let requests: Vec<serde_json::Value> = provider
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.method.as_str() == "POST")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    let mut seen = Vec::new();
    while let Ok(event) = observed.try_recv() {
        seen.push(event);
    }
    assert!(
        kiln.path().join("Remember.md").exists(),
        "reviewer did not write its note; requests: {requests:?}; events: {seen:?}"
    );
    assert_eq!(
        std::fs::read_to_string(kiln.path().join("Remember.md")).unwrap(),
        note
    );
    assert_eq!(
        agents.active_tools().get(&pass.id),
        None,
        "ending drops the transient tool set"
    );
    assert!(
        !agents.has_cached_agent(&pass.id),
        "ending releases the provider handle"
    );

    assert_eq!(requests.len(), 2, "one tool call and one continuation");
    let request = &requests[0];
    assert_eq!(request["model"], "reflection-fixture");
    assert!(request["messages"]
        .to_string()
        .contains("You are a reflection reviewer"));
    assert!(request["messages"]
        .to_string()
        .contains("Keep storage in the daemon."));
    let tools: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(tools.contains(&"create_note"), "{tools:?}");
    assert!(
        !tools
            .iter()
            .any(|t| ["bash", "read_file", "write_file"].contains(t)),
        "{tools:?}"
    );

    // Ending releases the live ledger; the normal review surface restores its journal.
    let hunks = bridge.review_list_hunks(pass.id.to_string()).await.unwrap();
    assert_eq!(hunks.len(), 1, "the pass's note is one unreviewed proposal");
    assert_eq!(hunks[0]["state"], "unreviewed");
    bridge
        .review_set_state(
            pass.id.to_string(),
            hunks[0]["id"].as_str().unwrap().into(),
            "rejected".into(),
        )
        .await
        .unwrap();
    assert!(
        !kiln.path().join("Remember.md").exists(),
        "rejecting a created note restores its absence"
    );
    assert!(
        bridge
            .review_list_hunks(source.id.to_string())
            .await
            .unwrap()
            .is_empty(),
        "the source owns no edits from the reflection pass"
    );
    ctx.session_lifecycle.fire_session_end(&source.id).await;
    assert_eq!(
        sessions.list_sessions().len(),
        2,
        "repeated teardown does not spawn another pass"
    );
}
