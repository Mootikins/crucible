use super::*;
use crucible_lua::register_sessions_module_with_api;

#[tokio::test]
async fn injected_context_reaches_the_next_turn_once_and_survives_rebuild() {
    let sm = temp_session_manager();
    let workspace = TempDir::new().unwrap();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();
    let messages = Arc::new(StdMutex::new(None));
    let handle = Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
        received_prompt: Arc::new(StdMutex::new(None)),
        received_messages: messages.clone(),
        events: vec![script::text("reply"), script::done()],
    }) as BoxedAgentHandle));
    am.install_agent_for_test(session.id.to_string(), handle.clone());
    let (tx, mut rx) = crate::EventBus::channel(64);
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
    lua.globals()
        .set("session_id", session.id.to_string())
        .unwrap();

    // Park the agent after the scheduler has assembled the first input.
    // Injection must not alter this turn, even before its first text delta.
    let parked = handle.lock().await;
    let start = am.send_message(&session.id, "first".into(), &tx, true, None);
    let inject = async {
        let user = next_event_or_skip(&mut rx, "user_message").await;
        lua.load(r#"assert(cru.session.inject(session_id, "user", "Remember the kiln"))"#)
            .exec_async()
            .await
            .unwrap();
        drop(parked);
        user
    };
    let (sent, user) = timeout(Duration::from_secs(10), async {
        tokio::join!(start, inject)
    })
    .await
    .unwrap();
    sent.unwrap();
    sm.storage()
        .append_event(&session, &serde_json::to_string(&user).unwrap())
        .await
        .unwrap();

    for prompt in ["first", "second", "third"] {
        if prompt != "first" {
            am.send_message(&session.id, prompt.into(), &tx, true, None)
                .await
                .unwrap();
        }
        // Use the real wire payload's persistence policy. Writes are deliberately
        // delayed until after acceptance to exercise the two-writer ordering.
        loop {
            let event = timeout(Duration::from_secs(10), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if event.payload().is_ok_and(|payload| payload.is_persisted()) {
                sm.storage()
                    .append_event(&session, &serde_json::to_string(&event).unwrap())
                    .await
                    .unwrap();
            }
            if event.event == "message_complete" {
                break;
            }
        }
        while am.request_state.contains_key(session.id.as_str()) {
            tokio::task::yield_now().await;
        }
        let captured = messages.lock().unwrap().clone().unwrap();
        let injected: Vec<_> = captured
            .iter()
            .filter(|m| m.content == "Remember the kiln")
            .collect();
        assert_eq!(injected.len(), usize::from(prompt != "first"));
        if let Some(message) = injected.first() {
            assert_eq!(message.role, crucible_core::traits::llm::MessageRole::User);
        }
        assert_eq!(
            am.get_or_rebuild_session_tree(&session.id, &session.jsonl_path(sm.sessions_root()))
                .await
                .lock()
                .await
                .undo_depth(),
            match prompt {
                "first" => 1,
                "second" => 2,
                _ => 3,
            }
        );
    }

    // A new manager has neither the live tree nor its pending queue.
    let resumed = create_test_agent_manager(sm.clone());
    resumed.install_agent_for_test(session.id.to_string(), handle);
    resumed
        .send_message(&session.id, "resumed".into(), &tx, true, None)
        .await
        .unwrap();
    next_event_or_skip(&mut rx, "message_complete").await;
    assert_eq!(
        resumed
            .get_or_rebuild_session_tree(&session.id, &session.jsonl_path(sm.sessions_root()))
            .await
            .lock()
            .await
            .undo_depth(),
        4
    );
    let captured = messages.lock().unwrap().clone().unwrap();
    assert_eq!(
        captured
            .iter()
            .filter(|m| m.content == "Remember the kiln")
            .count(),
        1
    );
    let content: Vec<_> = captured.iter().map(|m| m.content.as_str()).collect();
    let injected = content
        .iter()
        .position(|content| *content == "Remember the kiln")
        .unwrap();
    assert_eq!(content[injected - 1], "reply");
    assert_eq!(content[injected + 1], "second");

    sm.modify_session(&session.id, |live| {
        live.agent.as_mut().unwrap().agent_type = "acp".into();
        true
    })
    .await
    .unwrap();
    let before = std::fs::read(session.jsonl_path(sm.sessions_root())).unwrap();
    for role in ["system", "user", "assistant", "tool"] {
        let result = crate::server::session::inject_context_impl(
            &sm,
            &am,
            &tx,
            &session.id,
            role,
            "refused",
            None,
        )
        .await;
        assert!(result.is_err());
    }
    assert_eq!(
        std::fs::read(session.jsonl_path(sm.sessions_root())).unwrap(),
        before
    );
}

/// Injected system context reaches the model in one `<system-message>`
/// element that names who injected it. The log keeps its kind and source,
/// so a replay (a resume, a fork) gives the same message back.
#[tokio::test]
async fn injected_system_context_is_tagged_and_keeps_its_kind_after_replay() {
    let h = ReactorTestHarness::new().await;
    let sm = &h.agent_manager.session_manager;
    crate::server::session::inject_context_impl(
        sm,
        &h.agent_manager,
        &h.event_tx,
        &h.session_id,
        "system",
        "Remember the kiln",
        Some("alpha"),
    )
    .await
    .unwrap();

    let session = sm.get_session(&h.session_id).unwrap();
    let log = std::fs::read_to_string(session.jsonl_path(sm.sessions_root())).unwrap();
    let tree = crate::observe::rebuild::rebuild_tree_from_str(&log);
    let messages = tree.flatten_current_path_to_context();
    let injected = messages.last().expect("the injected message");
    assert_eq!(injected.metadata.kind.as_deref(), Some("context"));
    assert_eq!(injected.metadata.source.as_deref(), Some("alpha"));
    assert!(
        injected
            .content
            .starts_with("<system-message kind=\"context\" source=\"alpha\">"),
        "{}",
        injected.content
    );
}
