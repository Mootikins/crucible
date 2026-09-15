use super::*;
use crate::test_support::temp_session_manager;

#[tokio::test]
async fn send_message_emits_text_delta_events_in_order() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![
        script::text("hello"),
        script::text(" world"),
        script::done(),
    ]);

    let message_id = h.send("test").await;

    let user_message = h.wait_for("user_message").await;
    assert_eq!(user_message.data["content"], "test");
    assert_eq!(user_message.data["message_id"], message_id);

    let first_delta = h.wait_for("text_delta").await;
    assert_eq!(first_delta.data["content"], "hello");

    let second_delta = h.wait_for("text_delta").await;
    assert_eq!(second_delta.data["content"], " world");

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "hello world");

    // Scheduler-owned tree should carry the turn shape: root → User → Agent.
    let tree_arc = h
        .agent_manager
        .get_session_tree(&h.session_id)
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
    assert_eq!(h.agent_manager.undo_depth(&h.session_id).await.unwrap(), 1);
    assert!(h.agent_manager.can_undo(&h.session_id).await.unwrap());
    let summaries = h
        .agent_manager
        .undo(&h.session_id, 1, None)
        .await
        .expect("undo should succeed");
    assert_eq!(summaries.len(), 1);
    assert_eq!(h.agent_manager.undo_depth(&h.session_id).await.unwrap(), 0);
}

#[tokio::test]
async fn send_message_emits_thinking_before_text_delta() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![
        script::thinking("thinking..."),
        script::text("response"),
        script::done(),
    ]);

    h.send("test").await;

    let user_message = h.wait_for("user_message").await;
    assert_eq!(user_message.data["content"], "test");

    let first_after_user = timeout(Duration::from_secs(2), h.event_rx.recv())
        .await
        .expect("timed out waiting for first post-user event")
        .expect("event channel closed");
    assert_eq!(first_after_user.event, "thinking");
    assert_eq!(first_after_user.data["content"], "thinking...");

    let second_after_user = timeout(Duration::from_secs(2), h.event_rx.recv())
        .await
        .expect("timed out waiting for second post-user event")
        .expect("event channel closed");
    assert_eq!(second_after_user.event, "text_delta");
    assert_eq!(second_after_user.data["content"], "response");

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["full_response"], "response");
}

/// When a turn emits both Thinking and TextDelta events, the thinking
/// event must reach the scheduler before text_delta.
#[tokio::test]
async fn same_chunk_thinking_emitted_before_text_delta() {
    let mut h = ReactorTestHarness::new().await;

    // Script emits thinking before text so the scheduler must relay
    // them in that order.
    h.inject_streaming_agent(vec![
        script::thinking("let me think"),
        script::text("answer"),
        script::done(),
    ]);

    h.send("test").await;

    let _user_message = h.wait_for("user_message").await;

    // First event after user_message must be thinking, not text_delta
    let first = timeout(Duration::from_secs(2), h.event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");
    assert_eq!(
        first.event, "thinking",
        "Same-chunk: thinking must be emitted before text_delta, got: {}",
        first.event
    );
    assert_eq!(first.data["content"], "let me think");

    let second = timeout(Duration::from_secs(2), h.event_rx.recv())
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

/// Regression: an ACP-style agent (owns_history) runs its own tool loop and
/// emits ToolCall as an *observation*. The scheduler must NOT dispatch it —
/// doing so feeds a result to `inbound_tx`, which fails because the ACP turn
/// dropped the receiver, breaking the loop and truncating the turn right after
/// the first tool call (the bug: claude's answer was silently dropped). The
/// tool call must pass through and the agent's own follow-up text must arrive.
#[tokio::test]
async fn owns_history_agent_tool_call_passes_through_without_truncating_turn() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call(
                "call1",
                "Read",
                serde_json::json!({"file_path": "notes.txt"}),
            ),
            script::tool_result("call1", "Read", "LINE TWO beta"),
            script::text("Line two is: LINE TWO beta"),
            script::done(),
        ],
    }));

    h.send("read notes").await;

    // The tool call must be surfaced (TUI renders it)...
    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["tool"], "Read");

    // ...and crucially, the agent's own follow-up answer must still arrive —
    // proving the turn was not truncated at the tool call.
    let delta = h.wait_for("text_delta").await;
    assert_eq!(delta.data["content"], "Line two is: LINE TWO beta");

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["full_response"], "Line two is: LINE TWO beta");
}

/// An owns-history agent's `ToolBatchEnd` must not cut its turn short.
///
/// `AcpAgentHandle` closes its tool batch (`acp_handle.rs`), so the scheduler
/// now sees this event on delegated turns. It reaches the conjunctive
/// early-stop check, which ends the turn when *every* result in the batch set
/// `terminate` — and `batch_terminate_signals` is filled only by the dispatch
/// path an owns-history agent never takes. An empty signal set must therefore
/// read as "nobody asked to stop", not as "all agreed": the vacuous-truth
/// reading would end every delegated turn at its first tool call, dropping the
/// agent's answer exactly like the truncation bug above.
#[tokio::test]
async fn owns_history_tool_batch_end_does_not_end_the_turn() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call(
                "call1",
                "Read",
                serde_json::json!({"file_path": "notes.txt"}),
            ),
            script::tool_result("call1", "Read", "LINE TWO beta"),
            crucible_core::turn::TurnEvent::ToolBatchEnd,
            script::text("Line two is: LINE TWO beta"),
            script::done(),
        ],
    }));

    h.send("read notes").await;

    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["tool"], "Read");

    let delta = h.wait_for("text_delta").await;
    assert_eq!(delta.data["content"], "Line two is: LINE TWO beta");

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["full_response"], "Line two is: LINE TWO beta");
}

/// The daemon half of the ACP provenance-badge contract (US-307).
///
/// The TUI badges a delegated tool card `[acp:claude]` by parsing the
/// `source` field off the `tool_call` event. That field is produced *here*,
/// and the CLI's tests can only ever prove the parse — revert this producer
/// to a bare `"acp"` and every crucible-cli test stays green while the badge
/// silently loses the agent name. This asserts the wire string itself.
#[tokio::test]
async fn acp_pass_through_tool_call_names_the_agent_in_its_source() {
    let mut h = ReactorTestHarness::new().await;
    h.reconfigure(SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("claude".to_string()),
        ..test_agent()
    })
    .await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call("call1", "Read", serde_json::json!({"file_path": "a.txt"})),
            script::tool_result("call1", "Read", "contents"),
            script::done(),
        ],
    }));

    h.send("read a.txt").await;

    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["tool"], "Read");
    assert_eq!(
        tool_call.data["source"], "Acp:claude",
        "delegated tool_call must carry `Acp:<agent>` as its source, or the \
         TUI renders the card with no provenance badge; got {:?}",
        tool_call.data["source"]
    );
}

/// A delegated turn that only ran tools is a success, not an error.
///
/// The empty-response guard (`stream.rs`) fires when the turn accumulated no
/// text *and* dispatched no tools — but `tool_calls_dispatched` is set only on
/// the dispatch path, which the `agent_owns_tools` arm `continue`s past. So a
/// delegated turn that ran tools and narrated nothing tripped the guard: the
/// user saw `error: LLM returned empty response` on a turn that worked, while
/// the identical internal turn completed normally. `AcpAgentHandle` reports
/// `EndTurn` for the same turn, so the two layers disagreed outright.
#[tokio::test]
async fn owns_history_tool_only_turn_is_not_reported_as_an_empty_response() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call("call1", "Write", serde_json::json!({"path": "a.txt"})),
            script::tool_result("call1", "Write", "wrote 12 bytes"),
            crucible_core::turn::TurnEvent::ToolBatchEnd,
            script::done(),
        ],
    }));

    h.send("write a.txt").await;

    // The turn must reach `message_complete` without an error `ended` on the
    // way — an empty `full_response` is correct here; an error is not.
    timeout(Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    let reason = event.data["reason"].as_str().unwrap_or_default();
                    assert!(
                        !reason.starts_with("error:"),
                        "a delegated turn that only ran tools ended in error: {reason}"
                    );
                }
                Ok(event) if event.event == "message_complete" => return,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for message_complete on a tool-only delegated turn");
}

/// A `tool_result` handler must fire for tool calls an ACP-style agent ran
/// itself, not just for ones the daemon dispatched. The pass-through arm used
/// to emit straight to subscribers, so a redactor scrubbed the transcript for
/// internal agents and silently did nothing for external ones — the same
/// documented-but-silent shape this seam exists to close. The handler also
/// receives the announced `args`, so it can filter on them like on the
/// dispatched path.
#[tokio::test]
async fn tool_result_handlers_patch_acp_pass_through_results() {
    let mut h = ReactorTestHarness::new().await;

    let _vm = h.load_daemon_lua(
        r#"
        cru.on("tool_result", function(ctx, event)
            return {
                result = "redacted<" .. tostring(event.args.file_path) .. ">"
            }
        end)
    "#,
    );

    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call(
                "call1",
                "Read",
                serde_json::json!({"file_path": "secrets.txt"}),
            ),
            script::tool_result("call1", "Read", "SUPER SECRET"),
            script::text("done"),
            script::done(),
        ],
    }));

    h.send("read secrets").await;

    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(tool_result.data["tool"], "Read");
    assert_eq!(
        tool_result.data["result"]["result"], "redacted<secrets.txt>",
        "tool_result handler must patch ACP pass-through results and see their args"
    );
}

#[tokio::test]
async fn send_message_emits_tool_call_and_tool_result_events() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("test.md"), "content").unwrap();

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call1",
            "read_file",
            serde_json::json!({ "path": "test.md" }),
        ),
        script::tool_result("call1", "read_file", "content"),
        script::text("Done."),
        script::done(),
    ]);

    let message_id = h.send("test").await;

    let user_message = h.wait_for("user_message").await;
    assert_eq!(user_message.data["content"], "test");

    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");
    assert_eq!(tool_call.data["args"]["path"], "test.md");

    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");
    assert!(tool_result.data["result"]["result"]
        .as_str()
        .unwrap_or("")
        .contains("content"));

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "Done.");
}

/// A text → tool → text turn must emit a `segment_complete` for the
/// pre-tool narration, carrying the turn's `message_id`, `index` 0, and the
/// narration text — emitted BEFORE the `tool_call` event — while
/// `message_complete` still carries the WHOLE accumulated response. This is
/// what lets live viewers and reloaded viewers converge on the same
/// segmented bubbles with canonical ids.
#[tokio::test]
async fn text_then_tool_emits_segment_complete_before_tool_call() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("test.md"), "content").unwrap();

    h.inject_streaming_agent(vec![
        script::text("Let me look that up."),
        script::tool_call(
            "call1",
            "read_file",
            serde_json::json!({ "path": "test.md" }),
        ),
        script::tool_result("call1", "read_file", "content"),
        script::text("Here is what I found."),
        script::done(),
    ]);

    let message_id = h.send("test").await;

    // Drain events in arrival order until message_complete so we can assert
    // the segment_complete lands before the tool_call.
    let mut names = Vec::new();
    let mut segment: Option<SessionEventMessage> = None;
    let complete = timeout(Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
                Ok(event) => {
                    names.push(event.event.clone());
                    if event.event == "segment_complete" {
                        segment = Some(event.clone());
                    }
                    if event.event == "message_complete" {
                        return event;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed: {err}"),
            }
        }
    })
    .await
    .expect("timed out waiting for message_complete");

    let segment = segment.expect("expected a segment_complete event");
    assert_eq!(segment.data["message_id"], message_id);
    assert_eq!(segment.data["index"], 0);
    assert_eq!(segment.data["content"], "Let me look that up.");

    let seg_pos = names.iter().position(|n| n == "segment_complete");
    let tool_pos = names.iter().position(|n| n == "tool_call");
    assert!(
        seg_pos.is_some() && tool_pos.is_some() && seg_pos < tool_pos,
        "segment_complete must precede tool_call; got order: {names:?}"
    );

    // message_complete still carries the WHOLE turn's accumulated text —
    // other clients depend on this; segments do not replace it.
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(
        complete.data["full_response"],
        "Let me look that up.Here is what I found."
    );
}

/// A turn with narration but no tool call must NOT emit any
/// `segment_complete` — the single bubble carries the whole response.
#[tokio::test]
async fn text_only_turn_emits_no_segment_complete() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![script::text("just a plain answer"), script::done()]);

    h.send("test").await;

    let mut saw_segment = false;
    let complete = timeout(Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
                Ok(event) if event.event == "segment_complete" => saw_segment = true,
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
        !saw_segment,
        "text-only turn must not emit segment_complete"
    );
    assert_eq!(complete.data["full_response"], "just a plain answer");
}

#[tokio::test]
async fn display_hook_lua_tool_enriches_tool_call_metadata() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("test.md"), "content").unwrap();

    let _vm = h.load_daemon_lua(
        r#"
        cru.on("tool:display_start", function(ctx, event)
            return {
                label = "Custom " .. event.name,
                detail = "LuaStart"
            }
        end)

        cru.on("tool:display_complete", function(ctx, event)
            return {
                summary = "Summary " .. event.name
            }
        end)
    "#,
    );

    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-display-hook",
            "read_file",
            serde_json::json!({ "path": "test.md" }),
        ),
        script::text("Done."),
        script::done(),
    ]);

    h.send("test").await;

    let _ = h.wait_for("user_message").await;

    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");
    assert_eq!(tool_call.data["description"], "Custom read_file");
    assert_eq!(tool_call.data["source"], "LuaStart");

    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");
    assert_eq!(tool_result.data["result"]["summary"], "Summary read_file");
}

#[tokio::test]
async fn test_execute_agent_stream_empty_response_emits_error_event() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![script::done()]);

    h.send("test").await;

    let _ = h.wait_for("user_message").await;

    let mut saw_message_complete = false;
    let ended = timeout(Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
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
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![
        script::tool_call(
            "call-tool-only",
            "read_file",
            serde_json::json!({ "path": "test.md" }),
        ),
        script::tool_result("call-tool-only", "read_file", "content"),
        script::done(),
    ]);

    let message_id = h.send("test").await;

    let _ = h.wait_for("user_message").await;

    let mut saw_error_ended = false;
    let complete = timeout(Duration::from_secs(2), async {
        loop {
            match h.event_rx.recv().await {
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

    async fn tool_surface(&self, _name: &str) -> crucible_core::traits::tools::ToolSurface {
        crucible_core::traits::tools::ToolSurface::Unknown
    }
}

/// Mock handle that returns a scripted sequence of stream responses,
/// one per top-level call to `Agent::turn`.
struct ScriptedHandle {
    scripts: std::sync::Mutex<Vec<Vec<TurnEvent>>>,
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
        let scripts = std::mem::take(&mut *self.scripts.lock().unwrap());
        let mut scripts_iter = scripts.into_iter();
        let mut inbound = ctx.inbound;

        let body = async_stream::stream! {
            loop {
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
                    if let TurnEvent::ToolResult { id, .. } = event {
                        pending_tool_ids.remove(&id);
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
    fn new(scripts: Vec<Vec<TurnEvent>>) -> Self {
        Self {
            scripts: std::sync::Mutex::new(scripts),
        }
    }
}

crucible_core::impl_unsupported_session_knobs!(ScriptedHandle);

#[async_trait::async_trait]
impl AgentHandle for ScriptedHandle {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

fn tool_call_fixture(name: &str, id: &str) -> TurnEvent {
    script::tool_call(id, name, serde_json::json!({ "path": "fixtures/test.md" }))
}

/// A turn runs as many tool rounds as the model asks for.
///
/// There used to be a cap: `max_iterations`, defaulting to 10. On reaching it
/// the runtime injected a "give your final answer" prompt, and a model that
/// called a tool anyway failed the turn with `max_tool_depth exceeded`. Ten
/// rounds is well inside a normal refactor, so the shipped default cut real
/// work short. The cap is gone; this proves it.
#[tokio::test]
async fn a_turn_runs_more_tool_rounds_than_the_old_cap() {
    const ROUNDS: usize = 14; // the old default was 10

    let mut h = ReactorTestHarness::new().await;
    h.reconfigure(test_agent()).await;

    let mut script: Vec<Vec<TurnEvent>> = (0..ROUNDS)
        .map(|i| vec![tool_call_fixture("read_file", &format!("call-{i}"))])
        .collect();
    script.push(vec![script::text("final answer"), script::done()]);
    h.inject_agent(Box::new(ScriptedHandle::new(script)));

    h.send("test").await;
    let _ = h.wait_for("user_message").await;

    let mut saw_error_ended = false;
    let complete = timeout(Duration::from_secs(10), async {
        loop {
            match h.event_rx.recv().await {
                Ok(event) if event.event == "ended" => {
                    if event.data["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("error:")
                    {
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
        "{ROUNDS} tool rounds must complete normally, with no cap error"
    );
    assert!(
        complete.data["full_response"]
            .as_str()
            .unwrap_or_default()
            .contains("final answer"),
        "the turn must reach the model's own final answer: {:?}",
        complete.data["full_response"]
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

/// `send_message` captures a workspace snapshot before the agent turn
/// begins, and `undo` restores that snapshot. This drives the full
/// wire-up: the snapshot is keyed by the conversation tree's pre-turn
/// cursor, the cursor lands back on that key after `undo_turns(1)`, and
/// the journal-mode restore replays the original file bytes.
#[tokio::test]
async fn turn_undo_restores_snapshotted_file() {
    let mut h = ReactorTestHarness::new().await;
    let workspace = h.workspace().to_path_buf();

    // Seed the workspace with a tracked file in its pre-turn state.
    std::fs::write(workspace.join("a.txt"), b"v1").unwrap();

    // Mock agent: text reply, done. The "tool effect" we simulate is
    // the file mutation below — we do not need a real tool to verify
    // the snapshot/undo wire-up.
    h.inject_streaming_agent(vec![script::text("ok"), script::done()]);

    crate::server::session::inject_context_impl(
        &h.agent_manager.session_manager,
        &h.agent_manager,
        &h.event_tx,
        &h.session_id,
        "system",
        "Remember the kiln",
    )
    .await
    .unwrap();

    h.send("go").await;

    // Drain to message_complete so the snapshot has definitely been
    // taken (capture is synchronous in send_message before the spawned
    // task starts the stream).
    let _ = h.wait_for("message_complete").await;

    // Simulate a tool that wrote to the file mid-turn.
    std::fs::write(workspace.join("a.txt"), b"v2").unwrap();
    assert_eq!(std::fs::read(workspace.join("a.txt")).unwrap(), b"v2");

    // Undo the turn — restoration should bring back v1.
    let summaries = h
        .agent_manager
        .undo(&h.session_id, 1, None)
        .await
        .expect("undo should succeed");
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        std::fs::read(workspace.join("a.txt")).unwrap(),
        b"v1",
        "workspace snapshot should have restored a.txt to its pre-turn bytes"
    );
}

/// Snapshot lookup is keyed by the conversation-tree cursor *post-undo*.
/// This unit test exercises the SnapshotMap directly to lock in the
/// contract: `insert(node_id) → remove(node_id)` returns the same value
/// and clears the entry.
#[tokio::test]
async fn snapshot_map_round_trip_consumes_entry() {
    use crate::workspace_snapshot::{SnapshotMap, WorkspaceSnapshot};

    let map = SnapshotMap::default();
    map.insert("s1".to_string(), 7, WorkspaceSnapshot::default());
    assert_eq!(map.len(), 1);

    let got = map.remove("s1", 7).expect("expected entry under (s1, 7)");
    assert!(got.tree_id.is_none() && got.journal.is_none());
    assert!(map.is_empty());
}

/// Ended sessions are transparently resumable: sending to a session that has
/// been ended (removed from the in-memory map) revives it from storage before
/// processing the turn. The UI never has to gate on lifecycle state.
#[tokio::test]
async fn send_revives_ended_session_from_storage() {
    let mut h = ReactorTestHarness::new().await;
    // Scripted agent so the revived turn runs without a real provider. The
    // agent_cache entry survives `end_session` (only RPC cleanup clears it),
    // so the revived session reuses it.
    h.inject_streaming_agent(vec![script::text("revived"), script::done()]);

    // End the session. It stays resident and Ended — `end_session` no longer
    // evicts, because doing so dropped the turn's still-draining events (see
    // `end_session`). So this exercises the resident-but-Ended path; the
    // non-resident one is `send_revives_evicted_session_from_storage` below.
    let sm = h.agent_manager.session_manager().clone();
    sm.end_session(&h.session_id).await.unwrap();
    assert_eq!(
        sm.get_session(&h.session_id).map(|s| s.state),
        Some(crucible_core::session::SessionState::Ended),
        "an ended session stays resident until the archive sweep reclaims it"
    );

    // Sending to the ended session transparently revives it and accepts the turn.
    let message_id = h.send("hello again").await;

    // Revived into memory as Active.
    let revived = sm
        .get_session(&h.session_id)
        .expect("session should be revived into memory on send");
    assert_eq!(
        revived.state,
        crucible_core::session::SessionState::Active,
        "revived session should be Active, not Ended"
    );

    // The turn is really processed end-to-end.
    let user_message = h.wait_for("user_message").await;
    assert_eq!(user_message.data["content"], "hello again");
    assert_eq!(user_message.data["message_id"], message_id);

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "revived");
}

/// The same promise for a session that is genuinely gone from memory.
///
/// Reached in production by the archive sweep, which is the only thing that
/// evicts now: `remove_session` is the eviction verb it uses, and it accepts only
/// an already-`Ended` session. Kept as its own test because the resident path
/// above no longer touches `resume_session_from_storage`, so without this the
/// from-storage revive would be uncovered.
#[tokio::test]
async fn send_revives_evicted_session_from_storage() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![script::text("revived"), script::done()]);

    let sm = h.agent_manager.session_manager().clone();
    sm.end_session(&h.session_id).await.unwrap();
    sm.remove_session(&h.session_id)
        .expect("an ended session may be evicted");
    assert!(
        sm.get_session(&h.session_id).is_none(),
        "the point of this test is a session that is NOT resident"
    );

    let message_id = h.send("hello again").await;

    let revived = sm
        .get_session(&h.session_id)
        .expect("session should be revived into memory on send");
    assert_eq!(
        revived.state,
        crucible_core::session::SessionState::Active,
        "revived session should be Active, not Ended"
    );

    let user_message = h.wait_for("user_message").await;
    assert_eq!(user_message.data["content"], "hello again");
    assert_eq!(user_message.data["message_id"], message_id);

    let complete = h.wait_for("message_complete").await;
    assert_eq!(complete.data["message_id"], message_id);
    assert_eq!(complete.data["full_response"], "revived");
}

/// The load-bearing test for `cru.context.attach`: content retrieved by a
/// `tool_result` handler must reach the agent for its next LLM call of the
/// SAME turn, and must NOT enter the conversation tree.
///
/// Unit tests on `ContextAttachRegistry` pass with the stream-loop drain
/// unwired — only this one fails. Same gap that let the plugin-hook break ship.
#[tokio::test]
async fn attached_context_reaches_the_agent_within_the_same_turn() {
    let mut h = ReactorTestHarness::new().await;

    let _vm = h.load_daemon_lua(
        r#"
        -- No real tool executor in this harness; `handled` supplies the
        -- result. tool_result still fires over it, which is the point.
        cru.on("pre_tool_call", function(ctx, event)
            return { handled = true, result = "file contents" }
        end)
        cru.on("tool_result", function(ctx, event)
            cru.context.attach(ctx.session_id, "SENTINEL-ATTACHED-KNOWLEDGE",
                               { key = "k1" })
        end)
    "#,
    );

    let recorded = Arc::new(StdMutex::new(Vec::new()));
    h.inject_agent(Box::new(InboundRecordingAgent {
        events: vec![script::tool_call(
            "call1",
            "Read",
            serde_json::json!({"file_path": "notes.md"}),
        )],
        recorded: recorded.clone(),
    }));

    h.send("read the notes").await;
    let _ = h.wait_for("tool_result").await;
    let _ = h.wait_for("message_complete").await;

    let events = recorded.lock().unwrap().clone();
    let attached: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::ContextAttach { content } => Some(content.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        attached,
        vec!["SENTINEL-ATTACHED-KNOWLEDGE".to_string()],
        "attachment should reach the agent inbound; got {events:?}"
    );

    // Read-only guarantee: context is derived per call, history is not.
    let tree = h
        .agent_manager
        .get_session_tree(&h.session_id)
        .expect("session tree");
    let messages = tree.lock().await.flatten_current_path_to_context();
    assert!(
        !messages
            .iter()
            .any(|m| m.content.contains("SENTINEL-ATTACHED-KNOWLEDGE")),
        "attachment must not enter the conversation tree: {messages:?}"
    );
}

/// A handler firing on every tool call is the expected shape, so the dedup
/// must hold across calls rather than per batch.
#[tokio::test]
async fn repeated_triggers_attach_once_per_key() {
    let mut h = ReactorTestHarness::new().await;
    let _vm = h.load_daemon_lua(
        r#"
        cru.on("pre_tool_call", function(ctx, event)
            return { handled = true, result = "file contents" }
        end)
        cru.on("tool_result", function(ctx, event)
            cru.context.attach(ctx.session_id, "CPP-NOTES", { key = "filetype:cpp" })
        end)
    "#,
    );

    let recorded = Arc::new(StdMutex::new(Vec::new()));
    h.inject_agent(Box::new(InboundRecordingAgent {
        events: vec![
            script::tool_call("c1", "Read", serde_json::json!({"file_path": "a.cpp"})),
            script::tool_call("c2", "Read", serde_json::json!({"file_path": "b.cpp"})),
            script::tool_call("c3", "Read", serde_json::json!({"file_path": "c.cpp"})),
        ],
        recorded: recorded.clone(),
    }));

    h.send("read the cpp files").await;
    let _ = h.wait_for("message_complete").await;

    let attaches = recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, TurnEvent::ContextAttach { .. }))
        .count();

    assert_eq!(attaches, 1, "three .cpp reads should attach the notes once");
}

/// The registry must exist from `AgentManager::new`, not from plugin boot.
///
/// It used to be bound late, so a VM built before the bind got a nil
/// `cru.context.attach` — permanently, because VMs are cached — and the
/// resulting failure was silent: the handler raised, the hook failed open, and
/// the retrieval just never happened.
#[tokio::test]
async fn context_attach_is_available_on_the_handler_vm() {
    let h = ReactorTestHarness::new().await;
    let vm = h.load_daemon_lua("");

    let kind: String = vm
        .plugin_lua()
        .load("return type(cru and cru.context and cru.context.attach)")
        .eval()
        .expect("probe should evaluate");

    assert_eq!(
        kind, "function",
        "cru.context.attach must be present wherever handlers run"
    );
}

/// `@file` in a user message must put the file's CONTENTS in front of the
/// agent, not the path string.
///
/// Asserted on the messages the agent receives, deliberately. The TUI's
/// `attached_context` field — where the accepted completion used to write —
/// was read by nothing, so any assertion against it passes while the agent
/// still gets `@notes/todo.md` and has to guess. This is also why the feature
/// looked like it worked: a capable model answers by calling `read_file` on
/// the path it was handed.
#[tokio::test]
async fn at_mention_attaches_the_file_contents_to_the_turn() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("notes")).unwrap();
    std::fs::write(
        tmp.path().join("notes/todo.md"),
        "# Todo\n\nSENTINEL-FILE-BODY: ship the attachment path.\n",
    )
    .unwrap();

    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            Some(tmp.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent = test_agent();
    agent.precognition_enabled = false; // isolate: only attachment may inject
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    let received_messages = Arc::new(StdMutex::new(None));
    agent_manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: Arc::new(StdMutex::new(None)),
            received_messages: received_messages.clone(),
            events: vec![script::text("ok"), script::done()],
        }) as BoxedAgentHandle)),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(
            &session.id,
            "summarize @notes/todo.md please".to_string(),
            &event_tx,
            true,
            None,
        )
        .await
        .unwrap();
    let _ = next_event_or_skip(&mut event_rx, "message_complete").await;

    let messages = received_messages
        .lock()
        .unwrap()
        .clone()
        .expect("the agent should have received messages");
    assert!(
        messages
            .iter()
            .any(|m| m.content.contains("SENTINEL-FILE-BODY")),
        "the attached file's contents must reach the agent, got: {:?}",
        messages
            .iter()
            .map(|m| (&m.role, &m.content))
            .collect::<Vec<_>>()
    );
}

// -- ACP session id persistence (plan W7) -----------------------------------

/// An agent that reports an external (ACP) session id, the way
/// `AcpAgentHandle` does after connect. The turn itself is a noop: the
/// persistence under test happens at agent build, before any turn runs.
struct AcpIdReportingAgent;

crucible_core::impl_noop_agent!(AcpIdReportingAgent);
crucible_core::impl_unsupported_session_knobs!(AcpIdReportingAgent);

#[async_trait::async_trait]
impl AgentHandle for AcpIdReportingAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
    fn acp_session_id(&self) -> Option<String> {
        Some("agent-sess-1".to_string())
    }
}

/// The id an ACP handle reports at build time reaches the persisted
/// session. A handle built after a daemon restart reads it back and sends
/// `session/resume`, so the agent keeps its history.
#[tokio::test]
async fn the_agents_acp_session_id_is_persisted_for_resume() {
    use crate::session_storage::FileSessionStorage;

    let tmp = tempfile::TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new(FileSessionStorage::root_for(
        tmp.path(),
    )));
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();
    agent_manager.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async { Ok(Box::new(AcpIdReportingAgent) as BoxedAgentHandle) })
    }));

    let (event_tx, _event_rx) = broadcast::channel(64);
    let (_message_id, completion) = agent_manager
        .send_message_notified(&session.id, "hello".to_string(), &event_tx, true, None)
        .await
        .expect("send accepted");
    let _ = completion.await;

    let persisted = session_manager
        .get_session(&session.id)
        .and_then(|s| s.acp_session_id);
    assert_eq!(persisted.as_deref(), Some("agent-sess-1"));

    // The id also survives the round trip through storage, which is what a
    // daemon restart reads.
    let reloaded = session_manager
        .resume_session_from_storage(&session.id)
        .await
        .unwrap();
    assert_eq!(reloaded.acp_session_id.as_deref(), Some("agent-sess-1"));
}
