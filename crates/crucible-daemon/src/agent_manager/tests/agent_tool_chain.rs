//! A tool call of an agent that runs its own tools (ACP) goes through the
//! same call and result handlers as a Crucible tool. It skips only what the
//! agent owns: the dispatch and the spill.

use super::*;
use crucible_core::turn::NodeContent;
use crucible_core::types::CanonicalToolCall;

fn failed(id: &str, name: &str) -> TurnEvent {
    TurnEvent::ToolResult {
        id: id.to_string(),
        name: name.to_string(),
        result: serde_json::Value::String(String::new()),
        error: Some("no such file".to_string()),
    }
}

/// The node kinds of the conversation tree, in the order of their ids.
async fn tree_shape(h: &ReactorTestHarness) -> (Vec<&'static str>, usize, usize) {
    let tree = h
        .agent_manager
        .get_or_rebuild_session_tree(&h.session_id, std::path::Path::new("/nonexistent.jsonl"))
        .await;
    let tree = tree.lock().await;
    let kinds = tree
        .iter()
        .map(|(_, node)| match node.content {
            NodeContent::Root => "root",
            NodeContent::User { .. } => "user",
            NodeContent::ToolCall { .. } => "tool_call",
            NodeContent::ToolResult { .. } => "tool_result",
            NodeContent::Agent { .. } => "agent",
            _ => "other",
        })
        .collect();
    let removed = tree
        .turn_summaries()
        .iter()
        .map(|s| s.messages_removed)
        .sum();
    (kinds, tree.undo_depth(), removed)
}

/// Decision 6: an ACP call gets a `ToolResult` node, like a Crucible tool.
/// Undo reads the tree, so both turns must also undo the same way.
#[tokio::test]
async fn an_agent_tool_call_leaves_the_same_tree_as_a_crucible_tool_call() {
    let mut internal = ReactorTestHarness::new().await;
    std::fs::write(internal.workspace().join("a.txt"), "one").unwrap();
    internal.inject_streaming_agent(vec![
        script::tool_call("call1", "read_file", serde_json::json!({"path": "a.txt"})),
        script::text("done"),
        script::done(),
    ]);
    internal.send("read a.txt").await;
    internal.wait_for("turn_finished").await;

    let mut agent = ReactorTestHarness::new().await;
    agent.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call("call1", "read_file", serde_json::json!({"path": "a.txt"})),
            script::tool_result("call1", "read_file", "one"),
            script::text("done"),
            script::done(),
        ],
    }));
    agent.send("read a.txt").await;
    agent.wait_for("turn_finished").await;

    let expected = vec!["root", "user", "tool_call", "tool_result", "agent"];
    let (internal_kinds, internal_depth, internal_removed) = tree_shape(&internal).await;
    let (agent_kinds, agent_depth, agent_removed) = tree_shape(&agent).await;
    assert_eq!(internal_kinds, expected);
    assert_eq!(agent_kinds, expected, "the ACP call has no result node");
    assert_eq!(agent_depth, internal_depth);
    assert_eq!(agent_removed, internal_removed);
}

/// The render runs for an ACP call and for its result. It changes what the
/// user sees, and Crucible draws the card of every call.
#[tokio::test]
async fn the_render_runs_for_an_agent_tool_call_and_its_result() {
    let mut h = ReactorTestHarness::new().await;
    let _vm = h.load_daemon_lua(
        r#"
        cru.on("tool:render", { pattern = "tool" }, function(ctx, call)
            return {
                line = "Custom " .. call.tool,
                summary = call.result and ("Summary " .. call.result),
            }
        end)
    "#,
    );
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::tool_call("call1", "Read", serde_json::json!({"file_path": "a.txt"})),
            script::tool_result("call1", "Read", "one"),
            script::done(),
        ],
    }));

    h.send("read a.txt").await;

    let tool_call = h.wait_for("tool_call").await;
    assert_eq!(tool_call.data["display"]["render"]["line"], "Custom Read");
    let tool_result = h.wait_for("tool_result").await;
    assert_eq!(
        tool_result.data["result"]["render"]["summary"],
        "Summary one"
    );
}

/// The loop guard cannot stop an ACP call before it runs, so it ends the
/// turn. Three failures in a row of one call block the tool, as for a
/// Crucible tool, and the next call of the tool cancels the turn.
#[tokio::test]
async fn the_loop_guard_cancels_an_agent_turn_that_repeats_a_failed_call() {
    let mut h = ReactorTestHarness::new().await;
    let args = serde_json::json!({"file_path": "missing.txt"});
    let mut events = Vec::new();
    for id in ["c1", "c2", "c3"] {
        events.push(script::tool_call(id, "Read", args.clone()));
        events.push(failed(id, "Read"));
    }
    events.push(script::tool_call("c4", "Read", args));
    events.push(script::text("the turn went on"));
    events.push(script::done());
    h.inject_agent(Box::new(OwnsToolsMockAgent { events }));

    h.send("read missing.txt").await;

    let finished = h.wait_for("turn_finished").await;
    assert_eq!(
        finished.data["status"], "handler_cancelled",
        "got: {:?}",
        finished.data
    );
    assert!(
        finished.data["error"]
            .as_str()
            .is_some_and(|e| e.contains("'Read' is blocked")),
        "got: {:?}",
        finished.data
    );
}

/// An agent that writes a file between its `ToolCall` and its `ToolResult`,
/// as an ACP agent does.
struct WritingAgent {
    path: std::path::PathBuf,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for WritingAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities {
            owns_history: true,
            tool_calls: true,
            ..Default::default()
        }
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<futures::stream::BoxStream<'a, TurnEvent>, crucible_core::turn::AgentError> {
        drop(ctx);
        let path = self.path.clone();
        let args = serde_json::json!({"file_path": "a.txt"});
        let call = CanonicalToolCall {
            kind: "file_edit".to_string(),
            ..CanonicalToolCall::crucible_tool("Edit", &args)
        };
        Ok(Box::pin(async_stream::stream! {
            yield TurnEvent::ToolCall {
                id: "call1".to_string(),
                name: "Edit".to_string(),
                args,
                call: Some(Box::new(call)),
            };
            // The daemon polls the next event after it handled the call.
            std::fs::write(&path, "two\n").unwrap();
            yield script::tool_result("call1", "Edit", "edited");
            yield script::done();
        }))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(WritingAgent);

#[async_trait::async_trait]
impl AgentHandle for WritingAgent {
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

/// Audit 1.11: the review bracket opens at the ACP `ToolCall` and closes at
/// its `ToolResult`, so the ledger names the call that wrote the file, and
/// the edit is not `external`.
#[tokio::test]
async fn an_agent_edit_is_attributed_to_its_call() {
    let mut h = ReactorTestHarness::new().await;
    let path = h.workspace().join("a.txt");
    std::fs::write(&path, "one\n").unwrap();
    h.inject_agent(Box::new(WritingAgent { path }));

    h.send("edit a.txt").await;
    h.wait_for("turn_finished").await;

    let ledger = h
        .agent_manager
        .review
        .ledger(&h.session_id)
        .expect("the turn opens the ledger");
    let calls: Vec<(&str, bool)> = ledger
        .intervals()
        .iter()
        .map(|i| (i.tool_call_id.as_str(), i.contested))
        .collect();
    assert_eq!(calls, vec![("call1", false)]);
}
