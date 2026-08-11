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

        let handlers = state.registry.runtime_handlers_for("turn:complete", None);
        assert_eq!(handlers.len(), 1);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event, Some("test-session"))
            .await;
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

        let handlers = state.registry.runtime_handlers_for("turn:complete", None);
        assert_eq!(handlers.len(), 2);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _ = state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event, Some("test-session"))
                .await;
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

        let handlers = state.registry.runtime_handlers_for("turn:complete", None);
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _result = state
                .registry
                .execute_runtime_handler(&state.lua, &handler.name, &event, Some("test-session"))
                .await;
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

        let handlers_1 = state_1.registry.runtime_handlers_for("turn:complete", None);
        let handlers_2 = state_2.registry.runtime_handlers_for("turn:complete", None);

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
                received_session_id = event.session_id
                received_message_id = event.message_id
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for("turn:complete", None);
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": "test-123",
                "message_id": "msg-456",
            }),
        };

        let _ = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event, Some("test-session"))
            .await;

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

        let handlers = state.registry.runtime_handlers_for("turn:complete", None);
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, &handlers[0].name, &event, Some("test-session"))
            .await
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
            None,
            false, // is_continuation
        )
        .await;

        assert!(injection.is_some(), "Expected injection to be returned");
        let (content, _position) = injection.unwrap();
        assert_eq!(content, "Continue working");
    }

    /// A handler registered in the PLUGIN VM (a separate registry + Lua pair,
    /// exactly what `DaemonPluginLoader` holds) must fire for `turn:complete`.
    /// Until the plugin pair was threaded through, `pre_tool_call` was the
    /// only event that ever reached plugins — a plugin registering this
    /// handler got documented silence.
    #[tokio::test]
    async fn plugin_vm_turn_complete_handler_fires_and_injects() {
        use crucible_lua::{register_crucible_on_api, LuaScriptHandlerRegistry};

        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);
        let session_state = agent_manager.get_or_create_session_state("test-session");

        // A plugin VM: its own Lua state and its own registry, like the
        // daemon's plugin loader — NOT the session VM.
        let plugin_lua = Arc::new(mlua::Lua::new());
        let plugin_registry = Arc::new(LuaScriptHandlerRegistry::new());
        register_crucible_on_api(
            &plugin_lua,
            plugin_registry.runtime_handlers(),
            plugin_registry.handler_functions(),
        )
        .unwrap();
        plugin_lua
            .load(
                r#"
            crucible.on("turn:complete", function(ctx, event)
                return { inject = { content = "from the plugin VM: " .. ctx.session_id } }
            end)
        "#,
            )
            .exec()
            .unwrap();
        let plugin_pair = (plugin_registry, plugin_lua);

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            Some(&plugin_pair),
            false,
        )
        .await;

        let (content, _) = injection.expect("plugin VM handler must be dispatched");
        assert_eq!(
            content, "from the plugin VM: test-session",
            "handler must fire from the plugin registry and see ctx.session_id"
        );
    }

    /// Cross-registry inject ordering: the plugin VM pass runs after the
    /// session VM pass, so its inject wins the last-writer race — the same
    /// rule that lets plugin transforms see session transforms' output.
    #[tokio::test]
    async fn plugin_inject_overrides_session_inject() {
        use crucible_lua::{register_crucible_on_api, LuaScriptHandlerRegistry};

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
                    return { inject = { content = "session inject" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        let plugin_lua = Arc::new(mlua::Lua::new());
        let plugin_registry = Arc::new(LuaScriptHandlerRegistry::new());
        register_crucible_on_api(
            &plugin_lua,
            plugin_registry.runtime_handlers(),
            plugin_registry.handler_functions(),
        )
        .unwrap();
        plugin_lua
            .load(
                r#"
            crucible.on("turn:complete", function(ctx, event)
                return { inject = { content = "plugin inject" } }
            end)
        "#,
            )
            .exec()
            .unwrap();
        let plugin_pair = (plugin_registry, plugin_lua);

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            &session_state,
            Some(&plugin_pair),
            false,
        )
        .await;

        assert_eq!(injection.unwrap().0, "plugin inject");
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
            None,
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
            None,
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
                    received_continuation = event.is_continuation
                    if event.is_continuation then
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
            None,
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
            None,
            false,
        )
        .await;

        assert!(injection.is_none(), "No injection when handler returns nil");
    }
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

/// Partial cancel (user hits Esc) must drop any in-flight permission
/// `oneshot::Sender`s for the session, otherwise queued prompts behind
/// the `PermissionSerializer` lock stay blocked for the full 300s
/// timeout. Regression test for the cancel-arm fix.
#[tokio::test]
async fn cancel_drops_pending_permission_senders() {
    use crate::agent_manager::PendingPermission;
    use crucible_core::interaction::PermRequest;

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "cancel-pending-perm";

    // Insert a pending permission with a oneshot we can poll.
    let (response_tx, mut response_rx) = oneshot::channel();
    let perm_request = PermRequest::tool("bash", serde_json::json!({"command": "ls"}));
    agent_manager.slot(session_id).insert_permission(
        "perm-1".to_string(),
        PendingPermission {
            request: perm_request,
            response_tx,
        },
    );

    // Receiver should still be open right now.
    assert!(
        matches!(
            response_rx.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ),
        "receiver should be empty before cancel"
    );

    let cancelled = agent_manager.cancel(session_id).await;
    assert!(
        cancelled,
        "cancel should report success when it had pending state"
    );

    // After cancel, the sender was dropped → receiver returns Closed.
    assert!(
        matches!(
            response_rx.try_recv(),
            Err(oneshot::error::TryRecvError::Closed)
        ),
        "receiver must report Closed after cancel drops the sender"
    );
    assert!(
        agent_manager.slot(session_id).list_permissions().is_empty(),
        "the session's pending permissions should be gone after cancel"
    );
}

/// The post-cleanup invariant, in one place instead of five. Populate every
/// per-session store this manager owns, end the session, and assert nothing is
/// left — including the stores that are not `AgentManager` fields, which is how
/// `SESSION_SEQ_COUNTERS` came to leak an entry per session unnoticed.
///
/// A unique session id, not a shared `"test-session"`: the seq counters are a
/// process-global `static` shared with every other test in this binary.
#[tokio::test]
async fn cleanup_session_leaves_no_per_session_residue() {
    use crate::agent_manager::PendingPermission;
    use crucible_core::interaction::PermRequest;
    use crucible_core::session::{Comment, CommentAuthor, LineRange, PhysicalRoot, TreeSha};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);
    let session_id = "residue-session";

    // One populated entry in every per-session store, by the same route
    // production takes where there is one.
    //
    // `get_or_create_session_state` covers two: it builds the Lua VM and
    // records the session's captured defaults in `session_overrides`.
    let _ = agent_manager.get_or_create_session_state(session_id);
    let _ = agent_manager
        .get_or_rebuild_session_tree(session_id, std::path::Path::new("/nonexistent.jsonl"))
        .await;
    agent_manager.install_agent_for_test(
        session_id.to_string(),
        Arc::new(Mutex::new(Box::new(MockAgent))),
    );
    agent_manager
        .slot(session_id)
        .seed_build_for_test(None, Some(&agent_manager.tool_dispatcher));
    agent_manager.slot(session_id).set_pending_mode("plan");
    agent_manager
        .slot(session_id)
        .record_usage(&crucible_core::traits::llm::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: Some(4),
            cache_creation_tokens: None,
        });
    agent_manager.snapshots.insert(
        session_id.to_string(),
        0,
        crate::workspace_snapshot::WorkspaceSnapshot::default(),
    );
    let (cancel_tx, _cancel_rx) = oneshot::channel();
    agent_manager.request_state.insert(
        session_id.to_string(),
        RequestState {
            cancel_tx: Some(cancel_tx),
            task_handle: None,
            started_at: Instant::now(),
        },
    );
    let (response_tx, _response_rx) = oneshot::channel();
    agent_manager.slot(session_id).insert_permission(
        "perm-1".to_string(),
        PendingPermission {
            request: PermRequest::tool("bash", serde_json::json!({"command": "ls"})),
            response_tx,
        },
    );
    // A comment is the cheapest review-ledger entry: no git repo needed, and
    // teardown for a session with no registered parent is synchronous.
    agent_manager
        .review
        .add_comment(
            session_id,
            Comment::new(
                PhysicalRoot::from_top_level("/repo"),
                "a.txt",
                TreeSha::new("0".repeat(40)),
                LineRange::new(1, 2),
                "why this?",
                CommentAuthor::Human,
            ),
        )
        .await;
    // And one emitted event, so the session owns a sequence counter.
    let (event_tx, _event_rx) = broadcast::channel(4);
    crate::event_emitter::emit_event(
        &event_tx,
        SessionEventMessage::new(session_id, "test_event", serde_json::json!({})),
    );

    assert!(
        !agent_manager.session_residue(session_id).is_empty(),
        "the fixture must actually populate something, or this proves nothing"
    );

    agent_manager.cleanup_session(session_id);

    assert_eq!(
        agent_manager.session_residue(session_id),
        Vec::<&str>::new(),
        "cleanup_session must free every per-session store"
    );
}

/// The seq-counter map is a process-global `static` with no `Drop` reaching it,
/// so "one entry per session, forever" was its shipped behaviour. Assert the
/// bound directly rather than only through the residue check: N create/cleanup
/// cycles must leave N-0 entries, not N.
#[tokio::test]
async fn ending_sessions_does_not_grow_the_seq_counter_map() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);
    let (event_tx, _event_rx) = broadcast::channel(16);

    for i in 0..8 {
        let session_id = format!("seq-cycle-{i}");
        crate::event_emitter::emit_event(
            &event_tx,
            SessionEventMessage::new(&session_id, "test_event", serde_json::json!({})),
        );
        assert!(
            crate::event_emitter::has_seq_counter(&session_id),
            "emitting must mint a counter, or this test proves nothing"
        );
        agent_manager.cleanup_session(&session_id);
        assert!(
            !crate::event_emitter::has_seq_counter(&session_id),
            "session {session_id}'s counter must be freed at cleanup"
        );
    }
}
