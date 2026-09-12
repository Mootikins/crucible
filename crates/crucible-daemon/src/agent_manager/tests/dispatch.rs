use super::*;
use crate::test_support::temp_session_manager;

mod event_dispatch {
    use super::*;
    use crate::agent_manager::messaging::stream::TurnFacts;
    use crucible_lua::ScriptHandlerResult;

    /// A turn that ran no tool, ended naturally, and is not a re-prompt.
    fn a_first_turn() -> TurnFacts {
        TurnFacts {
            stop_reason: Some(crucible_core::turn::StopReason::EndTurn),
            continuation_depth: 0,
            saw_tool_activity: false,
        }
    }

    /// The same turn, reached by one re-prompt.
    fn a_re_prompted_turn() -> TurnFacts {
        TurnFacts {
            continuation_depth: 1,
            ..a_first_turn()
        }
    }

    /// The shipped default, so a test reads the payload a session really gets.
    fn tail_chars() -> usize {
        crucible_core::config::components::chat::DEFAULT_RESPONSE_TAIL_CHARS
    }

    #[tokio::test]
    async fn handler_executes_when_event_fires() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
            cru.on("turn:complete", function(ctx, event)
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for(
            "turn:complete",
            None,
            crucible_lua::Firing::Sessionless,
        );
        assert_eq!(handlers.len(), 1);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, handlers[0].id, &event, Some("test-session"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn multiple_handlers_run_in_priority_order() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
            execution_order = {}
            cru.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "first")
                return nil
            end)
            cru.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "second")
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for(
            "turn:complete",
            None,
            crucible_lua::Firing::Sessionless,
        );
        assert_eq!(handlers.len(), 2);

        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _ = state
                .registry
                .execute_runtime_handler(&state.lua, handler.id, &event, Some("test-session"))
                .await;
        }

        let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
        assert_eq!(order, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn handler_errors_dont_break_chain() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
            execution_order = {}
            cru.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "first")
                error("intentional error")
            end)
            cru.on("turn:complete", function(ctx, event)
                table.insert(execution_order, "second")
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for(
            "turn:complete",
            None,
            crucible_lua::Firing::Sessionless,
        );
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        for handler in &handlers {
            let _result = state
                .registry
                .execute_runtime_handler(&state.lua, handler.id, &event, Some("test-session"))
                .await;
        }

        let order: Vec<String> = state.lua.load("return execution_order").eval().unwrap();
        assert_eq!(order, vec!["first", "second"]);
    }

    #[tokio::test]
    async fn handler_receives_event_payload() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
            received_session_id = nil
            received_message_id = nil
            cru.on("turn:complete", function(ctx, event)
                received_session_id = event.session_id
                received_message_id = event.message_id
                return nil
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for(
            "turn:complete",
            None,
            crucible_lua::Firing::Sessionless,
        );
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": "test-123",
                "message_id": "msg-456",
            }),
        };

        let _ = state
            .registry
            .execute_runtime_handler(&state.lua, handlers[0].id, &event, Some("test-session"))
            .await;

        let session_id: String = state.lua.load("return received_session_id").eval().unwrap();
        let message_id: String = state.lua.load("return received_message_id").eval().unwrap();
        assert_eq!(session_id, "test-123");
        assert_eq!(message_id, "msg-456");
    }

    #[tokio::test]
    async fn handler_can_return_cancel() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
            cru.on("turn:complete", function(ctx, event)
                return { cancel = true, reason = "test cancel" }
            end)
        "#,
            )
            .exec()
            .unwrap();

        let handlers = state.registry.runtime_handlers_for(
            "turn:complete",
            None,
            crucible_lua::Firing::Sessionless,
        );
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({}),
        };

        let result = state
            .registry
            .execute_runtime_handler(&state.lua, handlers[0].id, &event, Some("test-session"))
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
        let state = handler_vm();

        // Register handler that returns inject
        {
            state
                .lua
                .load(
                    r#"
                cru.on("turn:complete", function(ctx, event)
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
            Some(&state.handlers()),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("Continue working"));
    }

    /// A handler registered in the PLUGIN VM (a separate registry + Lua pair,
    /// exactly what `DaemonPluginLoader` holds) must fire for `turn:complete`.
    /// Until the plugin pair was threaded through, `pre_tool_call` was the
    /// only event that ever reached plugins — a plugin registering this
    /// handler got documented silence.
    #[tokio::test]
    async fn plugin_vm_turn_complete_handler_fires_and_injects() {
        use crucible_lua::{register_cru_on_api, LuaScriptHandlerRegistry};

        // A plugin VM: its own Lua state and its own registry, like the
        // daemon's plugin loader.
        let plugin_lua = Arc::new(mlua::Lua::new());
        let plugin_registry = Arc::new(LuaScriptHandlerRegistry::new());
        register_cru_on_api(&plugin_lua, (*plugin_registry).clone()).unwrap();
        plugin_lua
            .load(
                r#"
            cru.on("turn:complete", function(ctx, event)
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
            Some(&plugin_pair),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        let content = injection.expect("plugin VM handler must be dispatched");
        assert_eq!(
            content, "from the plugin VM: test-session",
            "handler must fire from the plugin registry and see ctx.session_id"
        );
    }

    /// Cross-registry inject ordering: the plugin VM pass runs after the
    /// only pass, so its inject wins the last-writer race — the same
    /// rule that lets plugin transforms see session transforms' output.
    #[tokio::test]
    async fn plugin_inject_overrides_session_inject() {
        use crucible_lua::{register_cru_on_api, LuaScriptHandlerRegistry};

        let state = handler_vm();
        {
            state
                .lua
                .load(
                    r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = "session inject" } }
                end)
            "#,
                )
                .exec()
                .unwrap();
        }

        let plugin_lua = Arc::new(mlua::Lua::new());
        let plugin_registry = Arc::new(LuaScriptHandlerRegistry::new());
        register_cru_on_api(&plugin_lua, (*plugin_registry).clone()).unwrap();
        plugin_lua
            .load(
                r#"
            cru.on("turn:complete", function(ctx, event)
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
            Some(&plugin_pair),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("plugin inject"));
    }

    #[tokio::test]
    async fn second_inject_replaces_first() {
        let state = handler_vm();

        // Register two handlers that both return inject
        {
            state
                .lua
                .load(
                    r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = "First injection" } }
                end)
                cru.on("turn:complete", function(ctx, event)
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
            Some(&state.handlers()),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert_eq!(
            injection.as_deref(),
            Some("Second injection"),
            "Last inject should win"
        );
    }

    /// A handler that still writes the deleted `position` key still injects,
    /// and the key changes nothing.
    ///
    /// The test this replaces asserted that `position` survived the parse. It
    /// never asserted that the value did anything, and the scheduler dropped
    /// it — so both values behaved identically while the test passed.
    #[tokio::test]
    async fn an_inject_with_an_unknown_key_still_injects() {
        let state = handler_vm();

        {
            state
                .lua
                .load(
                    r#"
                cru.on("turn:complete", function(ctx, event)
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
            Some(&state.handlers()),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("Suffix content"));
    }

    #[tokio::test]
    async fn continuation_flag_passed_to_handlers() {
        let state = handler_vm();

        // Register handler that checks is_continuation and skips if true
        {
            state
                .lua
                .load(
                    r#"
                received_continuation = nil
                cru.on("turn:complete", function(ctx, event)
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
            Some(&state.handlers()),
            a_re_prompted_turn(),
            tail_chars(),
        )
        .await;

        // Handler should have returned nil, so no injection
        assert!(
            injection.is_none(),
            "Handler should skip injection on continuation"
        );

        // Verify the flag was received
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

    /// A handler reads the END of the reply, and is told when text was cut.
    ///
    /// The payload used to carry `response_length` alone — a number the
    /// scheduler computed from a string it was holding and then discarded. A
    /// plugin that wanted the text had to read `session.jsonl`.
    #[tokio::test]
    async fn a_handler_reads_the_reply_tail() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = event.response_tail
                        .. "|truncated=" .. tostring(event.response_truncated) } }
                end)
            "#,
            )
            .exec()
            .unwrap();

        let response = format!("{}the end", "x".repeat(50));
        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            &response,
            Some(&state.handlers()),
            a_first_turn(),
            10,
        )
        .await;

        assert_eq!(injection.as_deref(), Some("xxxthe end|truncated=true"));
    }

    /// A reply shorter than the limit arrives whole, and says so.
    #[tokio::test]
    async fn a_short_reply_is_not_truncated() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = event.response_tail
                        .. "|truncated=" .. tostring(event.response_truncated) } }
                end)
            "#,
            )
            .exec()
            .unwrap();

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "short",
            Some(&state.handlers()),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("short|truncated=false"));
    }

    /// A handler counts its own re-prompts.
    ///
    /// `is_continuation` is a bare bool, so a handler could tell the first
    /// turn from the rest and nothing more. The host counts and the plugin
    /// decides: there is no cap here, and a plugin that wants one sets it.
    #[tokio::test]
    async fn a_handler_counts_its_own_re_prompts() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = "depth=" .. tostring(event.continuation_depth) } }
                end)
            "#,
            )
            .exec()
            .unwrap();

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            Some(&state.handlers()),
            TurnFacts {
                continuation_depth: 7,
                ..a_first_turn()
            },
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("depth=7"));
    }

    /// A handler sees that the turn ran a tool.
    ///
    /// A turn that ends right after a tool result is a model still working,
    /// which no amount of reply text can tell a plugin.
    #[tokio::test]
    async fn a_handler_sees_tool_activity() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = "tools=" .. tostring(event.saw_tool_activity) } }
                end)
            "#,
            )
            .exec()
            .unwrap();

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            Some(&state.handlers()),
            TurnFacts {
                saw_tool_activity: true,
                ..a_first_turn()
            },
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("tools=true"));
    }

    /// A handler reads why the turn ended, in the wire spelling.
    #[tokio::test]
    async fn a_handler_reads_the_stop_reason() {
        let state = handler_vm();

        state
            .lua
            .load(
                r#"
                cru.on("turn:complete", function(ctx, event)
                    return { inject = { content = "stop=" .. tostring(event.stop_reason) } }
                end)
            "#,
            )
            .exec()
            .unwrap();

        let injection = AgentManager::dispatch_turn_complete_handlers(
            "test-session",
            "msg-123",
            "Some response",
            Some(&state.handlers()),
            TurnFacts {
                stop_reason: Some(crucible_core::turn::StopReason::MaxTokens),
                ..a_first_turn()
            },
            tail_chars(),
        )
        .await;

        assert_eq!(injection.as_deref(), Some("stop=max_tokens"));
    }

    #[tokio::test]
    async fn no_inject_when_handler_returns_nil() {
        let state = handler_vm();

        {
            state
                .lua
                .load(
                    r#"
                cru.on("turn:complete", function(ctx, event)
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
            Some(&state.handlers()),
            a_first_turn(),
            tail_chars(),
        )
        .await;

        assert!(injection.is_none(), "No injection when handler returns nil");
    }
}

#[tokio::test]
async fn cleanup_session_cancels_pending_requests() {
    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "test-session";
    let (cancel_tx, mut cancel_rx) = oneshot::channel();

    agent_manager.request_state.insert(
        session_id.to_string(),
        RequestState {
            cancel_tx: Some(cancel_tx),
            task_handle: None,
            _work: None,
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
    use crucible_core::interaction::PermRequest;

    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager(session_manager);

    let session_id = "cancel-pending-perm";

    // Register a pending permission with a oneshot we can poll.
    let perm_request = PermRequest::tool("bash", serde_json::json!({"command": "ls"}));
    let (_permission_id, mut response_rx) = agent_manager
        .slot(session_id)
        .register_permission(perm_request);

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
    use crucible_core::interaction::PermRequest;
    use crucible_core::session::{Comment, CommentAuthor, LineRange, PhysicalRoot, TreeSha};

    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager(session_manager);
    let session_id = "residue-session";

    // One populated entry in every per-session store, by the same route
    // production takes where there is one.
    //
    // `get_or_create_session_state` covers two: it builds the Lua VM and
    // records the session's captured defaults in `session_overrides`.
    let _ = agent_manager
        .get_or_rebuild_session_tree(session_id, std::path::Path::new("/nonexistent.jsonl"))
        .await;
    agent_manager.install_agent_for_test(
        session_id.to_string(),
        Arc::new(Mutex::new(Box::new(MockAgent))),
    );
    let dispatcher: Arc<dyn crate::tool_dispatch::ToolDispatcher> =
        Arc::new(crate::tool_dispatch::DaemonToolDispatcher::new(vec![]));
    agent_manager
        .slot(session_id)
        .seed_build_for_test(None, Some(&dispatcher));
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
            _work: None,
        },
    );
    let (_permission_id, _response_rx) =
        agent_manager
            .slot(session_id)
            .register_permission(PermRequest::tool(
                "bash",
                serde_json::json!({"command": "ls"}),
            ));
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
    // A plugin's narrowed tool set, by the route `cru.tools.set_active` takes.
    agent_manager
        .active_tools()
        .set(session_id, vec!["read_file".to_string()]);
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
    let session_manager = temp_session_manager();
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
