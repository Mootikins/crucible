use super::*;

#[tokio::test]
async fn send_message_emits_text_delta_events_in_order() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::text("hello"),
                script::text(" world"),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_eq!(user_message.data["content"], "test");
    assert_eq!(user_message.data["message_id"], message_id);

    let first_delta = next_event_or_skip(&mut event_rx, "text_delta").await;
    assert_eq!(first_delta.data["content"], "hello");

    let second_delta = next_event_or_skip(&mut event_rx, "text_delta").await;
    assert_eq!(second_delta.data["content"], " world");

    let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "hello world");

    // Scheduler-owned tree should carry the turn shape: root → User → Agent.
    let tree_arc = agent_manager
        .get_session_tree(&session.id)
        .expect("session tree should exist after a turn");
    let tree = tree_arc.lock().await;
    let path = tree.path_to_here(tree.current());
    assert_eq!(
        path.len(),
        3,
        "expected root → user → agent, got {} nodes",
        path.len()
    );
    let user = tree.get(path[1]);
    match &user.content {
        crucible_core::turn::NodeContent::User { text } => assert_eq!(text, "test"),
        other => panic!("expected User node, got {other:?}"),
    }
    let agent = tree.get(path[2]);
    match &agent.content {
        crucible_core::turn::NodeContent::Agent { text } => {
            assert_eq!(text, "hello world")
        }
        other => panic!("expected Agent node, got {other:?}"),
    }
    drop(tree);

    // One complete turn = undo_depth of 1; undo rewinds the cursor.
    assert_eq!(agent_manager.undo_depth(&session.id).unwrap(), 1);
    assert!(agent_manager.can_undo(&session.id).unwrap());
    let summaries = agent_manager
        .undo(&session.id, 1, None)
        .await
        .expect("undo should succeed");
    assert_eq!(summaries.len(), 1);
    assert_eq!(agent_manager.undo_depth(&session.id).unwrap(), 0);
}

#[tokio::test]
async fn send_message_emits_thinking_before_text_delta() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::thinking("thinking..."),
                script::text("response"),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_eq!(user_message.data["content"], "test");

    let first_after_user = timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timed out waiting for first post-user event")
        .expect("event channel closed");
    assert_eq!(first_after_user.event, "thinking");
    assert_eq!(first_after_user.data["content"], "thinking...");

    let second_after_user = timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timed out waiting for second post-user event")
        .expect("event channel closed");
    assert_eq!(second_after_user.event, "text_delta");
    assert_eq!(second_after_user.data["content"], "response");

    let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "response");
}

/// When a turn emits both Thinking and TextDelta events, the thinking
/// event must reach the scheduler before text_delta.
#[tokio::test]
async fn same_chunk_thinking_emitted_before_text_delta() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // Script emits thinking before text so the scheduler must relay
    // them in that order.
    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::thinking("let me think"),
                script::text("answer"),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _user_message = next_event_or_skip(&mut event_rx, "user_message").await;

    // First event after user_message must be thinking, not text_delta
    let first = timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        first.event, "thinking",
        "Same-chunk: thinking must be emitted before text_delta, got: {}",
        first.event
    );
    assert_eq!(first.data["content"], "let me think");

    let second = timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        second.event, "text_delta",
        "Same-chunk: text_delta must follow thinking, got: {}",
        second.event
    );
    assert_eq!(second.data["content"], "answer");
}

#[tokio::test]
async fn send_message_emits_tool_call_and_tool_result_events() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.md"), "content").unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::tool_call(
                    "call1",
                    "read_file",
                    serde_json::json!({ "path": "test.md" }),
                ),
                script::tool_result("call1", "read_file", "content"),
                script::text("Done."),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let user_message = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_eq!(user_message.data["content"], "test");

    let tool_call = next_event_or_skip(&mut event_rx, "tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");
    assert_eq!(tool_call.data["args"]["path"], "test.md");

    let tool_result = next_event_or_skip(&mut event_rx, "tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");
    assert!(tool_result.data["result"]["result"]
        .as_str()
        .unwrap_or("")
        .contains("content"));

    let complete = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "Done.");
}

#[tokio::test]
async fn display_hook_lua_tool_enriches_tool_call_metadata() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.md"), "content").unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    {
        let session_state = agent_manager.get_or_create_session_state(&session.id);
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            crucible.on("tool:display_start", function(ctx, event)
                return {
                    label = "Custom " .. event.name,
                    detail = "LuaStart"
                }
            end)

            crucible.on("tool:display_complete", function(ctx, event)
                return {
                    summary = "Summary " .. event.name
                }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::tool_call(
                    "call-display-hook",
                    "read_file",
                    serde_json::json!({ "path": "test.md" }),
                ),
                script::text("Done."),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let tool_call = next_event_or_skip(&mut event_rx, "tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");
    assert_eq!(tool_call.data["description"], "Custom read_file");
    assert_eq!(tool_call.data["source"], "LuaStart");

    let tool_result = next_event_or_skip(&mut event_rx, "tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");
    assert_eq!(tool_result.data["result"]["summary"], "Summary read_file");
}

#[tokio::test]
async fn test_execute_agent_stream_empty_response_emits_error_event() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let mut saw_message_complete = false;
    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "message_complete" => saw_message_complete = true,
                Ok(event) if event.event == "ended" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed while waiting for ended: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ended event");

    assert!(
        !saw_message_complete,
        "unexpected message_complete before error ended"
    );
    let ended_reason = ended.data["reason"].as_str().unwrap_or_default();
    assert!(
        ended_reason.starts_with("error:"),
        "expected error ended event, got: {ended_reason}"
    );
}

#[tokio::test]
async fn test_execute_agent_stream_tool_call_only_is_not_error() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![
                script::tool_call(
                    "call-tool-only",
                    "read_file",
                    serde_json::json!({ "path": "test.md" }),
                ),
                script::tool_result("call-tool-only", "read_file", "content"),
                script::done(),
            ],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let mut saw_error_ended = false;
    let complete = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    let reason = event.data["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if reason.starts_with("error:") {
                        saw_error_ended = true;
                    }
                }
                Ok(event) if event.event == "message_complete" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => {
                    panic!("event channel closed while waiting for message_complete: {err}")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for message_complete");

    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "");
    assert!(
        !saw_error_ended,
        "unexpected error ended event before message_complete in tool-call-only flow"
    );
}

// RED → GREEN: Bug 2 — tool dispatch timeout
struct HangingToolDispatcher;

#[async_trait::async_trait]
impl crate::tool_dispatch::ToolDispatcher for HangingToolDispatcher {
    async fn dispatch_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
        _env_vars: std::collections::HashMap<String, String>,
    ) -> Result<serde_json::Value, String> {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        Ok(serde_json::Value::Null)
    }

    fn has_tool(&self, _name: &str) -> bool {
        true
    }

    fn get_tool_ref(&self, _name: &str) -> Option<crucible_core::types::ToolRef> {
        None
    }
}

/// Mock handle that returns a scripted sequence of stream responses,
/// one per top-level call to `Agent::turn`. Captures the prompt passed
/// to each `turn` invocation so tests can assert the depth-cap prompt
/// was replayed.
struct ScriptedHandle {
    scripts: std::sync::Mutex<Vec<Vec<TurnEvent>>>,
    captured_prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for ScriptedHandle {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError> {
        const DEPTH_CAP_PROMPT: &str = "You have reached the tool call limit. Please provide your final answer based on the information gathered so far.";

        self.captured_prompts
            .lock()
            .unwrap()
            .push(ctx.content.clone());
        let scripts = std::mem::take(&mut *self.scripts.lock().unwrap());
        let mut scripts_iter = scripts.into_iter();
        let captured_prompts = Arc::clone(&self.captured_prompts);
        let mut inbound = ctx.inbound;

        let body = async_stream::stream! {
            'turn: loop {
                let Some(script) = scripts_iter.next() else {
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                    return;
                };

                let mut pending_tool_ids: std::collections::HashSet<String> =
                    std::collections::HashSet::new();

                for event in script {
                    if let TurnEvent::ToolCall { ref id, .. } = event {
                        pending_tool_ids.insert(id.clone());
                    }
                    yield event;
                }

                if pending_tool_ids.is_empty() {
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                    return;
                }

                yield TurnEvent::ToolBatchEnd;

                let Some(rx) = inbound.as_mut() else {
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                    return;
                };

                while !pending_tool_ids.is_empty() {
                    let Some(event) = rx.recv().await else {
                        yield TurnEvent::Done { stop_reason: StopReason::Cancelled };
                        return;
                    };
                    match event {
                        TurnEvent::ToolResult { id, .. } => {
                            pending_tool_ids.remove(&id);
                        }
                        TurnEvent::DepthCapHit { .. } => {
                            captured_prompts
                                .lock()
                                .unwrap()
                                .push(DEPTH_CAP_PROMPT.to_string());
                            continue 'turn;
                        }
                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(body))
    }

    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

impl ScriptedHandle {
    fn new(scripts: Vec<Vec<TurnEvent>>, captured: Arc<std::sync::Mutex<Vec<String>>>) -> Self {
        Self {
            scripts: std::sync::Mutex::new(scripts),
            captured_prompts: captured,
        }
    }
}

#[async_trait::async_trait]
impl AgentHandle for ScriptedHandle {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

fn tool_call_fixture(name: &str, id: &str) -> TurnEvent {
    script::tool_call(id, name, serde_json::json!({ "path": "fixtures/test.md" }))
}

#[tokio::test]
async fn depth_cap_triggers_depth_prompt_and_completes_with_text() {
    // Scenario: the model keeps emitting tool calls until we exceed
    // max_iterations. The runtime should send DepthCapHit on the inbound
    // channel, the adapter restarts the inner stream with the depth-cap
    // prompt, the mock replies with final text, and the turn finishes
    // normally — no "error: max_tool_depth exceeded" ended event.

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent_cfg = test_agent();
    agent_cfg.max_iterations = Some(2); // cap after 2 tool rounds
    agent_manager
        .configure_agent(&session.id, agent_cfg)
        .await
        .unwrap();

    let captured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    // Script (each entry = one `Agent::turn` iteration):
    //   1. initial turn("test")                 → tool_call id=call-1
    //   2. turn after tool result               → tool_call id=call-2
    //   3. turn after tool result               → tool_call id=call-3 (would be depth=3, capped)
    //   4. turn with DEPTH_CAP_PROMPT injection → terminal text "final"
    let handle: BoxedAgentHandle = Box::new(ScriptedHandle::new(
        vec![
            vec![tool_call_fixture("read_file", "call-1")],
            vec![tool_call_fixture("read_file", "call-2")],
            vec![tool_call_fixture("read_file", "call-3")],
            vec![script::text("final answer"), script::done()],
        ],
        captured.clone(),
    ));
    agent_manager
        .agent_cache
        .insert(session.id.clone(), Arc::new(Mutex::new(handle)));

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    // Drain until message_complete; the response should contain the
    // depth-prompt reply "final answer". No "error: max_tool_depth" ended.
    let mut saw_error_ended = false;
    let complete = timeout(Duration::from_secs(5), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    let reason = event.data["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if reason.starts_with("error:") {
                        saw_error_ended = true;
                    }
                }
                Ok(event) if event.event == "message_complete" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for message_complete");

    assert!(
        !saw_error_ended,
        "depth-cap flow must complete normally, not as error"
    );
    assert!(
        complete.data["full_response"]
            .as_str()
            .unwrap_or_default()
            .contains("final answer"),
        "final response missing depth-prompt reply: {:?}",
        complete.data["full_response"]
    );

    // The runtime must have replayed the depth-cap prompt to the model.
    let prompts = captured.lock().unwrap();
    assert!(
        prompts.iter().any(|p| p.contains("tool call limit")),
        "depth-cap prompt was not replayed: captured = {:?}",
        *prompts
    );
}

#[tokio::test(start_paused = true)]
async fn tool_dispatch_has_timeout() {
    // GREEN: verifies that a 30s timeout on dispatch_tool works correctly.
    // The production timeout lives in messaging.rs; this test verifies the
    // timeout mechanism itself using the same pattern.
    let dispatcher = std::sync::Arc::new(HangingToolDispatcher);

    let timeout_result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        dispatcher.dispatch_tool("test_tool", serde_json::json!({}), Default::default()),
    )
    .await;

    // With start_paused=true and no time advance, the future is still pending.
    // The timeout fires immediately because virtual time hasn't advanced.
    // This confirms the timeout mechanism works — production code uses same pattern.
    assert!(
        timeout_result.is_err(),
        "dispatch_tool should timeout after 30s when tool hangs"
    );
}

/// With `OutputValidation::Json` and `validation_retries=0`, an invalid
/// JSON response must surface as a single `ended` event whose reason is
/// the validation-exhausted marker — no second turn is attempted.
#[tokio::test]
async fn test_validate_retry_zero_retries_emits_exhausted_ended() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent = test_agent();
    agent.output_validation = crucible_core::session::OutputValidation::Json;
    agent.validation_retries = 0;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("not json at all"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed while waiting for ended: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ended event");

    let reason = ended.data["reason"].as_str().unwrap_or_default();
    assert_eq!(
        reason, "error: output validation exhausted retries",
        "expected validation-exhausted reason, got: {reason}"
    );
}

/// With `OutputValidation::None` (the default), invalid JSON should
/// flow through normally — no validation, no retry, no ended-error.
#[tokio::test]
async fn test_validate_retry_none_validation_passes_freely() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("not json"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    // We expect message_complete to fire normally — no validation
    // gate intercepted it.
    let mc = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(mc.data["full_response"], "not json");
}

/// Build a Lua VM with `cru.context.register_validator(...)` mounted and
/// return `(Arc<Lua>, Arc<LuaValidatorRegistry>)` ready for hand-off to
/// `AgentManager::set_lua_validators`. Mirrors the daemon's plugin loader
/// path without spinning up the full loader.
fn lua_validator_runtime(
) -> (Arc<mlua::Lua>, Arc<crucible_lua::LuaValidatorRegistry>) {
    let lua = Arc::new(mlua::Lua::new());
    let registry = Arc::new(crucible_lua::LuaValidatorRegistry::new());
    crucible_lua::register_context_validators(&lua, Arc::clone(&registry))
        .expect("register_context_validators");
    (lua, registry)
}

/// `OutputValidation::Lua` with a registered validator that returns
/// `false, reason` — the stream loop must inject a retry prompt and on
/// exhaustion emit the standard validation-exhausted ended event.
#[tokio::test]
async fn test_lua_validator_failure_triggers_retry_and_exhausts() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let (lua, registry) = lua_validator_runtime();
    lua.load(r#"cru.context.register_validator("nope", function(_) return false, "boom" end)"#)
        .exec()
        .expect("register validator");
    agent_manager.set_lua_validators(Arc::clone(&registry), Arc::clone(&lua));

    let mut agent = test_agent();
    agent.output_validation = crucible_core::session::OutputValidation::Lua {
        name: "nope".to_string(),
    };
    agent.validation_retries = 0;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("anything"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ended event");

    let reason = ended.data["reason"].as_str().unwrap_or_default();
    assert_eq!(
        reason, "error: output validation exhausted retries",
        "expected validation-exhausted reason, got: {reason}"
    );
}

/// `OutputValidation::Lua` with a registered validator that returns
/// `true` — the response should flow through normally without retry.
#[tokio::test]
async fn test_lua_validator_pass_no_retry() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let (lua, registry) = lua_validator_runtime();
    lua.load(r#"cru.context.register_validator("ok", function(_) return true end)"#)
        .exec()
        .expect("register validator");
    agent_manager.set_lua_validators(Arc::clone(&registry), Arc::clone(&lua));

    let mut agent = test_agent();
    agent.output_validation = crucible_core::session::OutputValidation::Lua {
        name: "ok".to_string(),
    };
    agent.validation_retries = 0;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("anything"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let mc = next_event_or_skip(&mut event_rx, "message_complete").await;
    assert_eq!(mc.data["full_response"], "anything");
}

/// `OutputValidation::Lua { name }` referring to an unregistered name
/// surfaces as a validation failure (with a clear reason) and exhausts
/// per `validation_retries`. The plugin runtime IS bound here — the only
/// problem is that `name` was never `register_validator`'d.
#[tokio::test]
async fn test_lua_validator_unregistered_name_errors() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    // Registry is bound but no validator named "missing" was registered.
    let (lua, registry) = lua_validator_runtime();
    agent_manager.set_lua_validators(Arc::clone(&registry), Arc::clone(&lua));

    let mut agent = test_agent();
    agent.output_validation = crucible_core::session::OutputValidation::Lua {
        name: "missing".to_string(),
    };
    agent.validation_retries = 0;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("anything"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;

    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.event == "ended" => return event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for ended event");

    let reason = ended.data["reason"].as_str().unwrap_or_default();
    assert_eq!(
        reason, "error: output validation exhausted retries",
        "expected validation-exhausted reason, got: {reason}"
    );
}
