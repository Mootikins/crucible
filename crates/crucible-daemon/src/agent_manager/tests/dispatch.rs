use super::*;

mod event_dispatch {
    use super::*;
    use crucible_lua::ScriptHandlerResult;

    #[tokio::test]
    async fn handler_executes_when_event_fires() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

        state
            .lua
            .load(
                r#"
            crucible.on("turn:complete", function(ctx, event)
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete");
        assert_eq!(handlers.len(), 1);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn multiple_handlers_run_in_priority_order() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

        state
            .lua
            .load(
                r#"
            execution_order = {}
            crucible.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "first")
                return nil
            end)
            crucible.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "second")
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete");
        assert_eq!(handlers.len(), 2);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _ = state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event);
        }

        let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
        assert_eq!(order, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn handler_errors_dont_break_chain() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

        state
            .lua
            .load(
                r#"
            execution_order = {}
            crucible.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "first")
                error("intentional error")
            end)
            crucible.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "second")
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete");
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _result = state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event);
        }

        let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
        assert_eq!(order, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn handlers_are_session_scoped() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state_1 = agent_manager.get_or_create_session_state("session-1");
        let session_state_2 = agent_manager.get_or_create_session_state("session-2");

        {
            let state = session_state_1.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        {
            let state = session_state_2.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return nil
                end)
                crucible.on("turn:complete", function(ctx, event)
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        let state_1 = session_state_1.lock().await;
        let state_2 = session_state_2.lock().await;

        let handlers_1 = state_1.registry.runtime_handlers_for("turn:complete");
        let handlers_2 = state_2.registry.runtime_handlers_for("turn:complete");

        assert_eq!(handlers_1.len(), 1, "Session 1 should have 1 handler");
        assert_eq!(handlers_2.len(), 2, "Session 2 should have 2 handlers");
    }

    #[tokio::test]
    async fn handler_receives_event_payload() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

        state
            .lua
            .load(
                r#"
            received_session_id = nil
            received_message_id = nil
            crucible.on("turn:complete", function(ctx, event)
                received_session_id = event.payload.session_id
                received_message_id = event.payload.message_id
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete");
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": "test-123",
                "message_id": "msg-456",
            }),
        };

        let _ = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event);

        let session_id: String = state.lua.load("return received_session_id").eval().unwrap();
        let message_id: String = state.lua.load("return received_message_id").eval().unwrap();
        assert_eq!(session_id, "test-123");
        assert_eq!(message_id, "msg-456");
    }

    #[tokio::test]
    async fn handler_can_return_cancel() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");
        let state = session_state.lock().await;

        state
            .lua
            .load(
                r#"
            crucible.on("turn:complete", function(ctx, event)
                return { cancel = true, reason = "test cancel" }
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete");
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event)
            .unwrap();

        match result {
            ScriptHandlerResult::Cancel { reason } => {
                assert_eq!(reason, "test cancel");
            }
            _ => panic!("Expected Cancel result"),
        }
    }

    #[tokio::test]
    async fn handler_returns_inject_collected_by_dispatch() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register handler that returns inject
        {
            let state = session_state.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return { inject = { content = "Continue working" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        // Dispatch handlers and check for injection
        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            false, // is_continuation
        )
        .await;

        assert!(injection.is_some(), "Expected injection to be returned");
        let (content, _position) = injection.unwrap();
        assert_eq!(content, "Continue working");
    }

    #[tokio::test]
    async fn second_inject_replaces_first() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register two handlers that both return inject
        {
            let state = session_state.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return { inject = { content = "First injection" } }
                end)
                crucible.on("turn:complete", function(ctx, event)
                    return { inject = { content = "Second injection" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        // Dispatch handlers - last one should win
        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            false,
        )
        .await;

        assert!(injection.is_some(), "Expected injection to be returned");
        let (content, _position) = injection.unwrap();
        assert_eq!(content, "Second injection", "Last inject should win");
    }

    #[tokio::test]
    async fn inject_includes_position() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");

        {
            let state = session_state.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return { inject = { content = "Suffix content", position = "user_suffix" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            false,
        )
        .await;

        assert!(injection.is_some());
        let (content, position) = injection.unwrap();
        assert_eq!(content, "Suffix content");
        assert_eq!(position, "user_suffix");
    }

    #[tokio::test]
    async fn continuation_flag_passed_to_handlers() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");

        // Register handler that checks is_continuation and skips if true
        {
            let state = session_state.lock().await;
            state
                .lua
                .load(
                    r#"
                received_continuation = nil
                crucible.on("turn:complete", function(ctx, event)
                    received_continuation = event.payload.is_continuation
                    if event.payload.is_continuation then
                        return nil  -- Skip injection on continuation
                    end
                    return { inject = { content = "Should not inject" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        // Dispatch with is_continuation = true
        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            true, // is_continuation
        )
        .await;

        // Handler should have returned nil, so no injection
        assert!(
            injection.is_none(),
            "Handler should skip injection on continuation"
        );

        // Verify the flag was received
        let state = session_state.lock().await;
        let received: bool = state
            .lua
            .load("return received_continuation")
            .eval()
            .unwrap();
        assert!(
            received,
            "Handler should have received is_continuation=true"
        );
    }

    #[tokio::test]
    async fn no_inject_when_handler_returns_nil() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_state = agent_manager.get_or_create_session_state("test-session");

        {
            let state = session_state.lock().await;
            state
                .lua
                .load(
                    r#"
                crucible.on("turn:complete", function(ctx, event)
                    return nil
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            false,
        )
        .await;

        assert!(injection.is_none(), "No injection when handler returns nil");
    }
}

#[tokio::test]
async fn cleanup_session_removes_lua_state() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "test-session";

    let _ = agent_manager.get_or_create_session_state(session_id);
    assert!(
        agent_manager.session_states.contains_key(session_id),
        "Lua state should exist after creation"
    );

    agent_manager.cleanup_session(session_id);

    assert!(
        !agent_manager.session_states.contains_key(session_id),
        "Lua state should be removed after cleanup"
    );
}

#[tokio::test]
async fn cleanup_session_removes_agent_cache() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "test-session";

    agent_manager.agent_cache.insert(
        session_id.to_string(),
        Arc::new(Mutex::new(Box::new(MockAgent))),
    );
    assert!(
        agent_manager.agent_cache.contains_key(session_id),
        "Agent cache should exist after insertion"
    );

    agent_manager.cleanup_session(session_id);

    assert!(
        !agent_manager.agent_cache.contains_key(session_id),
        "Agent cache should be removed after cleanup"
    );
}

#[tokio::test]
async fn cleanup_session_cancels_pending_requests() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "test-session";
    let (cancel_tx, mut cancel_rx) = oneshot::channel();

    agent_manager.request_state.insert(
        session_id.to_string(),
        RequestState {
            cancel_tx: Some(cancel_tx),
            task_handle: None,
            started_at: Instant::now(),
        },
    );

    assert!(
        agent_manager.request_state.contains_key(session_id),
        "Request state should exist after insertion"
    );

    agent_manager.cleanup_session(session_id);

    assert!(
        !agent_manager.request_state.contains_key(session_id),
        "Request state should be removed after cleanup"
    );

    let result = cancel_rx.try_recv();
    assert!(
        result.is_ok(),
        "Cancel signal should have been sent during cleanup"
    );
}
