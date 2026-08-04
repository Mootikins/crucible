//! Per-agent `TurnEvent` contract for an ACP-delegated turn.
//!
//! These tests drive a real `AcpAgentHandle` (which spawns the
//! `mock-acp-agent` binary and speaks ACP over its stdio) and project the
//! outbound `TurnEvent` stream with the shared parity harness
//! (`acp_support/parity.rs`).
//!
//! They assert the **ACP agent's own** event contract, not equality with the
//! internal agent — the two diverge at this layer by design (see the module
//! docs on `support::parity`). Cross-agent equality lives one layer down, at
//! `SessionEventMessage`.

use std::time::Duration;

use crucible_core::session::SessionAgent;
use crucible_core::turn::{Agent, StopReason, TurnContext};
use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};
use tempfile::TempDir;
use tokio::time::timeout;

use crate::support::parity::{coalesce, shapes, EventShape};
use crate::support::{mock_agent_path, mock_session_agent};

/// Connect a handle to the mock agent binary and drain one turn into its
/// rendering-relevant shape sequence.
async fn acp_shapes(agent_config: SessionAgent) -> Vec<EventShape> {
    let workspace = TempDir::new().expect("temp workspace");

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(AcpAgentHandleParams {
            agent_config: &agent_config,
            workspace: workspace.path(),
            kiln_path: None,
            knowledge_repo: None,
            embedding_provider: None,
            background_spawner: None,
            delegation_spawner: None,
            parent_session_id: None,
            delegation_config: None,
            acp_config: None,
            permission_handler: None,
            sandbox_exec: None,
        }),
    )
    .await
    .expect("ACP handshake timed out")
    .expect("ACP handshake failed");

    let collected = timeout(Duration::from_secs(30), async {
        let stream = handle
            .turn(TurnContext::new("what is 2+2?"))
            .await
            .expect("Agent::turn failed");
        shapes(stream).await
    })
    .await
    .expect("ACP turn timed out");

    coalesce(collected)
}

/// A turn in which the agent narrates, runs one tool, and answers.
async fn acp_shapes_for_scripted_tool_call() -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), "Calculating…".into());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_TOOL_CALL".to_string(), "1".into());

    acp_shapes(agent_config).await
}

/// The same turn with no tool call at all.
async fn acp_shapes_for_text_only_turn() -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "The answer is 4".into(),
    );

    acp_shapes(agent_config).await
}

/// B1: an ACP turn that called a tool must close the batch.
///
/// `ToolBatchEnd` is the event the scheduler's per-batch bookkeeping hangs
/// off (`agent_manager/messaging/stream.rs`): batch reset and the conjunctive
/// `terminate` check both key on it. The internal agent emits it after every
/// tool batch (`provider/genai_handle.rs`); an ACP turn that never emits it
/// leaves every consumer of that boundary permanently un-notified.
#[tokio::test]
async fn acp_emits_tool_batch_end_after_tool_calls() {
    let shapes = acp_shapes_for_scripted_tool_call().await;

    assert!(
        shapes.contains(&EventShape::ToolBatchEnd),
        "ACP turn emitted no ToolBatchEnd, so every consumer of the tool-batch \
         boundary is dead on delegated sessions; got {shapes:#?}"
    );
}

/// The batch closes *after* the calls it contains and *before* the turn ends.
///
/// Ordering is the whole content of the event: a `ToolBatchEnd` that arrives
/// before the last `ToolCall` would split one logical batch in two, and the
/// second half would never be closed.
#[tokio::test]
async fn acp_tool_batch_end_comes_after_every_tool_call_and_before_done() {
    let shapes = acp_shapes_for_scripted_tool_call().await;

    let batch_end = shapes
        .iter()
        .position(|s| *s == EventShape::ToolBatchEnd)
        .unwrap_or_else(|| panic!("no ToolBatchEnd in {shapes:#?}"));
    let last_call = shapes
        .iter()
        .rposition(|s| matches!(s, EventShape::ToolCall { .. }))
        .unwrap_or_else(|| panic!("no ToolCall in {shapes:#?}"));
    let done = shapes
        .iter()
        .position(|s| matches!(s, EventShape::Done(_)))
        .unwrap_or_else(|| panic!("no Done in {shapes:#?}"));

    assert!(
        last_call < batch_end && batch_end < done,
        "expected ToolCall … ToolBatchEnd … Done, got {shapes:#?}"
    );
    assert_eq!(shapes.last(), Some(&EventShape::Done(StopReason::EndTurn)));
}

/// A turn with no tool calls must not announce an empty batch.
///
/// The scheduler ignores empty batches, but an unconditional `ToolBatchEnd`
/// would claim a batch that never existed and would mean the event no longer
/// tells a consumer that tools ran.
#[tokio::test]
async fn acp_text_only_turn_emits_no_tool_batch_end() {
    let shapes = acp_shapes_for_text_only_turn().await;

    assert!(
        !shapes.contains(&EventShape::ToolBatchEnd),
        "a turn with no tool calls announced a tool batch; got {shapes:#?}"
    );
    assert_eq!(
        shapes,
        vec![
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ]
    );
}
