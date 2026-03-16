use super::*;

#[tokio::test]
async fn reactor_pre_llm_modifies_prompt() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-modify-prompt".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::ModifyPrompt("MODIFIED: hello".to_string()),
    })
    .await;
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("MODIFIED: hello"));
}

#[tokio::test]
async fn reactor_pre_llm_cancel_aborts() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-cancel-pre-llm".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Cancel,
    })
    .await;

    let received_prompt = h.inject_capturing_agent(vec![ChatChunk {
        delta: "should-not-run".to_string(),
        done: true,
        tool_calls: None,
        tool_results: None,
        reasoning: None,
        usage: None,
        subagent_events: None,
        precognition_notes_count: None,
        precognition_notes: None,
    }]);

    h.send("hello").await;
    let ended = h.wait_for("ended").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(ended.data["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("cancelled by handler"));
    let prompt = received_prompt.lock().unwrap();
    assert!(prompt.is_none());
}

#[tokio::test]
async fn reactor_pre_llm_empty_passthrough() {
    let mut h = ReactorTestHarness::new().await;
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn reactor_pre_llm_error_fails_open() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-fatal-pre-llm".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::FatalError("boom".to_string()),
    })
    .await;

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn reactor_post_llm_fires_after_stream() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-post-llm".to_string(),
        event_pattern: "post_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Passthrough,
    })
    .await;

    h.inject_streaming_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn reactor_pre_tool_cancel_denies() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-pre-tool-cancel".to_string(),
        event_pattern: "pre_tool_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Cancel,
    })
    .await;

    h.inject_streaming_agent(vec![
        ChatChunk {
            delta: String::new(),
            done: false,
            tool_calls: Some(vec![ChatToolCall {
                name: "write".to_string(),
                arguments: Some(serde_json::json!({ "path": "foo.txt", "content": "x" })),
                id: Some("call-pre-tool-cancel".to_string()),
            }]),
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
        ChatChunk {
            delta: "done".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
    ]);

    h.send("run tool").await;

    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(tool_result.data["tool"], "write");
    assert!(tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Tool call denied by handler"));
}

#[tokio::test]
async fn runtime_dispatch_pre_llm_call_transforms_prompt() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            crucible.on("pre_llm_call", function(ctx, event)
                return { prompt = event.payload.prompt .. " [modified]" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello [modified]"));
}

#[tokio::test]
async fn runtime_dispatch_pre_tool_call_cancels_execution() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            crucible.on("pre_tool_call", function(ctx, event)
                return { cancel = true, reason = "blocked" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    h.inject_streaming_agent(vec![
        ChatChunk {
            delta: String::new(),
            done: false,
            tool_calls: Some(vec![ChatToolCall {
                name: "write".to_string(),
                arguments: Some(serde_json::json!({ "path": "foo.txt", "content": "x" })),
                id: Some("call-runtime-pre-tool-cancel".to_string()),
            }]),
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
        ChatChunk {
            delta: "done".to_string(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        },
    ]);

    h.send("run tool").await;

    let tool_result = h.wait_for("tool_result").await;
    h.wait_for("message_complete").await;

    assert_eq!(tool_result.data["tool"], "write");
    assert!(tool_result.data["result"]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("blocked"));
}

#[tokio::test]
async fn runtime_dispatch_post_llm_call_fires_handler() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            post_llm_runtime_fired = false
            crucible.on("post_llm_call", function(ctx, event)
                post_llm_runtime_fired = true
                return { cancel = true, reason = "ignored" }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    h.inject_streaming_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;

    next_event_or_skip(&mut h.event_rx, "post_llm_call").await;

    let fired = timeout(Duration::from_secs(2), async {
        loop {
            let state = session_state.lock().await;
            let fired: bool = state
                .lua
                .load("return post_llm_runtime_fired")
                .eval()
                .unwrap();
            drop(state);
            if fired {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for post_llm_call runtime handler");

    assert!(fired);
}

#[tokio::test]
async fn reactor_persists_across_messages() {
    let mut h = ReactorTestHarness::new().await;

    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    h.register_handler(MockHandler {
        name: "test-persists".to_string(),
        event_pattern: "pre_llm_call".to_string(),
        call_count: call_count.clone(),
        behavior: MockHandlerBehavior::Passthrough,
    })
    .await;

    h.inject_streaming_agent(ReactorTestHarness::default_ok_chunks());

    h.send("one").await;
    h.wait_for("message_complete").await;

    h.send("two").await;
    h.wait_for("message_complete").await;

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn reactor_cleanup_drops_state() {
    let h = ReactorTestHarness::new().await;

    let _ = h.agent_manager.get_or_create_session_state(&h.session_id);
    assert!(h.agent_manager.session_states.contains_key(&h.session_id));

    h.agent_manager.cleanup_session(&h.session_id);

    assert!(!h.agent_manager.session_states.contains_key(&h.session_id));
}

#[tokio::test]
async fn reactor_lua_handler_discovery_empty_dir() {
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        assert!(state.reactor.is_empty());
    }

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_chunks());

    h.send("hello").await;
    h.wait_for("message_complete").await;

    let prompt = received_prompt.lock().unwrap();
    assert_eq!(prompt.as_deref(), Some("hello"));
}

#[test]
fn event_patterns_match_event_type() {
    let _repo = MockKnowledgeRepository { results: vec![] };
    let _embedding = MockEmbeddingProvider { should_fail: false };

    let pre_llm = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: String::new(),
        model: String::new(),
    });
    assert_eq!(pre_llm.event_type(), "pre_llm_call");

    let post_llm = SessionEvent::internal(InternalSessionEvent::PostLlmCall {
        response_summary: String::new(),
        model: String::new(),
        duration_ms: 0,
        token_count: None,
    });
    assert_eq!(post_llm.event_type(), "post_llm_call");

    let pre_tool = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: String::new(),
        args: serde_json::Value::Null,
    });
    assert_eq!(pre_tool.event_type(), "pre_tool_call");
}
