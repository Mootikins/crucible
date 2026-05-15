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
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

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

    let received_prompt =
        h.inject_capturing_agent(vec![script::text("should-not-run"), script::done()]);

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
    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

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

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

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

    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

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
        script::tool_call(
            "call-pre-tool-cancel",
            "write",
            serde_json::json!({ "path": "foo.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
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

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

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
        script::tool_call(
            "call-runtime-pre-tool-cancel",
            "write",
            serde_json::json!({ "path": "foo.txt", "content": "x" }),
        ),
        script::text("done"),
        script::done(),
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

    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

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

    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

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

    let received_prompt = h.inject_capturing_agent(ReactorTestHarness::default_ok_events());

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

#[tokio::test]
async fn runtime_pre_tool_handled_with_terminate_ends_turn() {
    // Pi-style conjunctive early-stop: if a pre_tool_call handler returns
    // { handled=true, result=..., terminate=true }, and that's the only tool
    // in the batch, the agent loop ends after the batch.
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            crucible.on("pre_tool_call", function(ctx, event)
                return { handled = true, result = "final answer", terminate = true }
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    // Single tool_call only; the scripted stream emits ToolBatchEnd and
    // waits for ToolResult feedback. With terminate=true the loop should
    // end after the batch instead of returning to the model.
    h.inject_streaming_agent(vec![script::tool_call(
        "call-terminate",
        "submit_answer",
        serde_json::json!({ "answer": "x" }),
    )]);

    h.send("test").await;

    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(tool_result.data["tool"], "submit_answer");

    let ended = h.wait_for("ended").await;
    assert!(
        ended.data["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("terminate"),
        "ended reason should mention terminate, got: {:?}",
        ended.data
    );
}

#[tokio::test]
async fn runtime_pre_tool_terminate_mixed_batch_does_not_end() {
    // Conjunctive: if any result in the batch lacks terminate=true, the
    // loop continues normally — one tool can't unilaterally cut another's
    // work short.
    let mut h = ReactorTestHarness::new().await;

    let session_state = h.agent_manager.get_or_create_session_state(&h.session_id);
    {
        let state = session_state.lock().await;
        state
            .lua
            .load(
                r#"
            crucible.on("pre_tool_call", function(ctx, event)
                local tool = event.payload.tool
                if tool == "submit_final" then
                    return { handled = true, result = "done", terminate = true }
                elseif tool == "keep_going" then
                    return { handled = true, result = "more work", terminate = false }
                end
            end)
        "#,
            )
            .exec()
            .unwrap();
    }

    // Two tools in one batch: only one signals terminate.
    h.inject_streaming_agent(vec![
        script::tool_call("call-1", "submit_final", serde_json::json!({})),
        script::tool_call("call-2", "keep_going", serde_json::json!({})),
    ]);

    h.send("test").await;

    // Mixed batch should NOT terminate — flow completes normally
    // (scripted stream yields Done after ToolResults round-trip).
    h.wait_for("message_complete").await;
}
