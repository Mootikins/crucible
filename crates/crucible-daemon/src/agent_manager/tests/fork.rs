use super::*;
use crucible_lua::register_sessions_module_with_api;

#[tokio::test]
async fn lua_and_rpc_forks_inherit_scope_config_and_run_with_the_selected_history() {
    let workspace = TempDir::new().unwrap();
    let sm = temp_session_manager_with_kilns(&[("notes", workspace.path())]);
    let am = Arc::new(create_test_agent_manager(sm.clone()));
    let mut parent = sm
        .create_session(
            SessionType::Chat,
            vec![kiln_name("notes")],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    parent.isolation = Some(serde_json::json!(false));
    parent
        .variables
        .insert("project_rule".into(), serde_json::json!("preserved"));
    sm.update_session(&parent).await.unwrap();
    let mut config = test_agent();
    config.system_prompt = "Inherited instructions".into();
    config.context_budget = Some(8192);
    am.configure_agent(&parent.id, config.clone())
        .await
        .unwrap();
    for event in [
        serde_json::to_string(&SessionEventMessage::user_message(
            &parent.id,
            "parent-turn",
            "first",
        ))
        .unwrap(),
        crate::observe::LogEvent::assistant("answer")
            .to_jsonl()
            .unwrap(),
        serde_json::to_string(&crate::observe::events::InjectedContext {
            after_turn: Some("parent-turn".into()),
            message: crate::observe::LogEvent::user("remember"),
        })
        .unwrap(),
        crate::observe::LogEvent::user("excluded")
            .to_jsonl()
            .unwrap(),
    ] {
        sm.storage().append_event(&parent, &event).await.unwrap();
    }
    let (tx, _) = broadcast::channel(128);
    let ctx = Arc::new(crate::rpc::RpcContext::for_test(
        am.kiln_manager.clone(),
        sm.clone(),
        am.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            workspace.path().join("projects.json"),
        )),
        tx.clone(),
        workspace.path().into(),
    ));
    let lua = mlua::Lua::new();
    register_sessions_module_with_api(
        &lua,
        Arc::new(crate::session_bridge::DaemonSessionBridge::new(ctx)),
    )
    .unwrap();
    lua.globals().set("parent", parent.id.to_string()).unwrap();
    let lua_child: String = lua.load("local child, err = cru.session.fork(parent, { up_to = 3 }); assert(child, err); return child.id").eval_async().await.unwrap();
    let response = crate::server::session::handle_session_fork(serde_json::from_value(serde_json::json!({"jsonrpc": "2.0", "method": "session.fork", "params": {"session_id": parent.id, "up_to": 3}, "id": 1})).unwrap(), &sm, &am).await;
    let rpc_child = response.result.unwrap()["id"].as_str().unwrap().to_string();
    for child_id in [lua_child, rpc_child] {
        let child = sm.get_session(&child_id).unwrap();
        assert_eq!(child.workspace, parent.workspace);
        assert_eq!(child.kilns, parent.kilns);
        assert_eq!(child.isolation, parent.isolation);
        assert_eq!(child.variables, parent.variables);
        assert!(child.parent_session_id.is_none());
        am.configure_agent(&child_id, config.clone()).await.unwrap();
        assert_eq!(
            sm.get_session(&child_id).unwrap().variables,
            parent.variables
        );
        assert_eq!(
            serde_json::to_value(&child.agent).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
        let messages = Arc::new(StdMutex::new(None));
        am.install_agent_for_test(
            child_id.clone(),
            Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
                received_prompt: Arc::new(StdMutex::new(None)),
                received_messages: messages.clone(),
                events: vec![script::text("fork reply"), script::done()],
            }))),
        );
        let (_, completed) = am
            .send_message_notified(&child_id, "continue".into(), &tx, false, None)
            .await
            .unwrap();
        timeout(Duration::from_secs(5), completed)
            .await
            .unwrap()
            .unwrap();
        let captured = messages.lock().unwrap().clone().unwrap();
        let texts: Vec<_> = captured.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(texts, ["first", "answer", "remember", "continue"]);
        assert_eq!(
            am.get_or_rebuild_session_tree(&child_id, &child.jsonl_path(sm.sessions_root()))
                .await
                .lock()
                .await
                .undo_depth(),
            2
        );
    }
}

#[tokio::test]
async fn forks_read_cold_parents_and_refuse_unreadable_history_without_creating_sessions() {
    let sm = temp_session_manager();
    let parent = sm
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .unwrap();
    sm.storage()
        .append_event(
            &parent,
            &crate::observe::LogEvent::user("remember")
                .to_jsonl()
                .unwrap(),
        )
        .await
        .unwrap();
    sm.end_session(&parent.id).await.unwrap();
    let restarted = Arc::new(SessionManager::with_storage(sm.storage().clone()));
    let am = create_test_agent_manager(restarted.clone());
    for (limit, expected) in [(Some(0), 0), (None, 1)] {
        let (child, count) = am
            .fork_session(
                restarted.read_session(&parent.id).await.unwrap().unwrap(),
                limit,
            )
            .await
            .unwrap();
        assert_eq!(count, expected);
        assert!(child.agent.is_none());
        assert!(child.workspace.is_none());
        assert_eq!(
            crate::observe::load_events(restarted.session_dir(&child.id))
                .await
                .unwrap()
                .len(),
            expected as usize
        );
    }
    assert!(
        restarted.get_session(&parent.id).is_none(),
        "forking does not revive the parent"
    );
    assert_eq!(
        restarted
            .read_session(&parent.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        crucible_core::session::SessionState::Ended
    );

    let unreadable = sm
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .unwrap();
    std::fs::create_dir(unreadable.jsonl_path(sm.sessions_root())).unwrap();
    let before = sm.storage().list().await.unwrap().len();
    assert!(am.fork_session(unreadable, None).await.is_err());
    assert_eq!(sm.storage().list().await.unwrap().len(), before);
}
