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
use crucible_daemon::acp_handle::AcpAgentHandle;
use tempfile::TempDir;
use tokio::time::timeout;

use crate::support::parity::{coalesce, shapes, EventShape};
use crate::support::{mock_agent_path, mock_handle_params, mock_session_agent};

/// Connect a handle to the mock agent binary and drain one turn into its
/// rendering-relevant shape sequence.
async fn acp_shapes(agent_config: SessionAgent) -> Vec<EventShape> {
    let workspace = TempDir::new().expect("temp workspace");

    let mut handle = timeout(
        Duration::from_secs(30),
        AcpAgentHandle::new(mock_handle_params(&agent_config, workspace.path())),
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
///
/// `tool_call` selects the value of the mock's `CRU_MOCK_STREAM_TOOL_CALL`
/// hook: `None` leaves it unset, `Some("0")` sets it to an off value. Both
/// must produce a text-only turn — the hook reads a value, not a presence, so
/// `=0` cannot mean "on".
async fn acp_shapes_for_text_only_turn(tool_call: Option<&str>) -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "The answer is 4".into(),
    );
    if let Some(value) = tool_call {
        agent_config
            .env_overrides
            .insert("CRU_MOCK_STREAM_TOOL_CALL".to_string(), value.into());
    }

    acp_shapes(agent_config).await
}

/// A turn the agent ends with `stopReason: cancelled`.
///
/// Per ACP an agent MUST end a cancelled turn this way, so this is the shape
/// the daemon sees whenever cancellation actually lands.
async fn acp_shapes_for_cancelled_turn() -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), "Working on it".into());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STOP_REASON".to_string(), "cancelled".into());

    acp_shapes(agent_config).await
}

/// A turn in which the agent reasons, narrates, then reasons again.
///
/// `thoughts` is the mock's `CRU_MOCK_STREAM_THOUGHTS` script: `;`-separated
/// thoughts, the first emitted before the text chunk and the rest after it. The
/// hook reads a *value*, so an empty script must mean "no thoughts" rather than
/// "thoughts on".
async fn acp_shapes_for_thinking_turn(thoughts: &str) -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "The answer is 4".into(),
    );
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_THOUGHTS".to_string(), thoughts.into());

    acp_shapes(agent_config).await
}

/// A turn in which the agent also reports its context window.
///
/// `script` is the mock's `CRU_MOCK_USAGE_UPDATE` hook, `used/size`. `None`
/// leaves it unset — an agent that never reports a window, which is what
/// `cursor` and `gemini` do in the recorded fixtures.
async fn acp_shapes_for_usage_update(script: Option<&str>) -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config.env_overrides.insert(
        "CRU_MOCK_STREAM_CHUNKS".to_string(),
        "The answer is 4".into(),
    );
    if let Some(value) = script {
        agent_config
            .env_overrides
            .insert("CRU_MOCK_USAGE_UPDATE".to_string(), value.into());
    }

    acp_shapes(agent_config).await
}

/// A turn that produces nothing at all: no text, no thinking, no tool calls.
async fn acp_shapes_for_empty_turn() -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    acp_shapes(mock_session_agent(&agent_path)).await
}

/// A turn whose only tool traffic is a completed `tool_call_update` for an id
/// that no `tool_call` ever introduced.
///
/// `flavor` picks the mock's `CRU_MOCK_ORPHAN_TOOL_END` script: `"bare"` omits
/// every field the client records a call from, so the name is unknowable;
/// `"titled"` carries a title, so the call *is* recorded and the handle's
/// post-stream replay can still name it; `"repeat"` completes a properly
/// announced call twice; `"out_of_order"` completes a call before announcing
/// it.
async fn acp_shapes_for_orphaned_tool_end(flavor: &str) -> Vec<EventShape> {
    let agent_path = mock_agent_path().to_string_lossy().into_owned();
    let mut agent_config = mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_STREAM_CHUNKS".to_string(), "Done".into());
    agent_config
        .env_overrides
        .insert("CRU_MOCK_ORPHAN_TOOL_END".to_string(), flavor.into());

    acp_shapes(agent_config).await
}

/// C4: an ACP agent's reasoning must reach the turn stream.
///
/// Claude Code, Gemini and every other conforming ACP agent stream reasoning as
/// `session/update` `agent_thought_chunk` frames. The client matched
/// `AgentMessageChunk` and let thought chunks fall into its terminal
/// "ignoring session update" arm, so `StreamingChunk::Thinking` had **no
/// production producer** and the whole
/// `Thinking` → `TurnEvent::Thinking` → `thinking` SessionEvent →
/// `ThinkingComponent` chain was dead on delegated sessions: the internal agent
/// showed thinking blocks and a delegated one showed none.
///
/// Order is part of the contract, not decoration: a delegated agent runs its
/// own tool loop, so it reasons, narrates and reasons again inside one turn.
/// Hoisting every thought to the front — or dropping the ones that follow text
/// — would rewrite what the user reads.
///
/// The rest of the chain is already pinned: `TurnEvent::Thinking` → `thinking`
/// SessionEvent in `agent_manager/tests/messaging.rs`, and `thinking` → a
/// rendered block in `user_story_tests/acp_parity_tests.rs`
/// (`a_delegated_agent_second_thought_reaches_the_screen`). This test is the
/// link that was missing.
#[tokio::test]
async fn acp_thought_chunks_reach_the_turn_stream() {
    let shapes = acp_shapes_for_thinking_turn("let me add them;that checks out").await;

    assert!(
        shapes.iter().any(|s| matches!(s, EventShape::Thinking(_))),
        "an ACP agent's thought chunks never became Thinking events, so a \
         delegated session renders no thinking blocks at all; got {shapes:#?}"
    );
    assert_eq!(
        shapes,
        vec![
            EventShape::Thinking("let me add them".into()),
            EventShape::Text("The answer is 4".into()),
            EventShape::Thinking("that checks out".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "delegated reasoning must interleave with the narration in wire order; \
         got {shapes:#?}"
    );
}

/// The mock's thinking hook reads a *value*, not the variable's presence.
///
/// A presence check would make `CRU_MOCK_STREAM_THOUGHTS=` script a thought
/// with empty text, which the harness drops as invisible — so the negative case
/// would silently stop testing anything.
#[tokio::test]
async fn mock_thinking_hook_set_to_empty_scripts_no_thoughts() {
    let shapes = acp_shapes_for_thinking_turn("").await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "CRU_MOCK_STREAM_THOUGHTS= must mean no thoughts; got {shapes:#?}"
    );
}

/// B1: an ACP turn that called a tool must close the batch.
///
/// `ToolBatchEnd` is the event the scheduler's per-batch bookkeeping hangs off
/// (`agent_manager/messaging/stream.rs:839`): batch reset and the conjunctive
/// `terminate` check both key on it. The internal agent emits it after every
/// tool batch (`provider/genai_handle.rs`).
///
/// Emitting it here changes no daemon behaviour *today* — every variable that
/// handler touches is already at its default on an `owns_history` turn, so the
/// scheduler has no live consumer of the boundary on the ACP path. This pins
/// the contract, not an observable effect: `AcpAgentHandle` must honour the
/// same `TurnEvent` grammar as `GenaiAgentHandle` so the first consumer that
/// does key on the boundary is not silently wrong on delegated sessions.
#[tokio::test]
async fn acp_emits_tool_batch_end_after_tool_calls() {
    let shapes = acp_shapes_for_scripted_tool_call().await;

    assert!(
        shapes.contains(&EventShape::ToolBatchEnd),
        "ACP turn emitted no ToolBatchEnd, so a delegated turn's tool batch is \
         never closed and the ACP event grammar diverges from the internal \
         agent's; got {shapes:#?}"
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
    let shapes = acp_shapes_for_text_only_turn(None).await;

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

/// The mock's tool-call hook reads a *value*, not the variable's presence.
///
/// A presence check (`env::var(..).is_ok()`) makes
/// `CRU_MOCK_STREAM_TOOL_CALL=0` turn the tool call *on*, which would quietly
/// invert any future test that scripts the negative case through this hook.
#[tokio::test]
async fn mock_tool_call_hook_set_to_zero_scripts_no_tool_call() {
    let shapes = acp_shapes_for_text_only_turn(Some("0")).await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "CRU_MOCK_STREAM_TOOL_CALL=0 must mean off; got {shapes:#?}"
    );
}

/// B3: a cancelled turn must not render as a normal completion.
///
/// The stop reason is the only signal that separates "the agent finished" from
/// "the agent was stopped", and ACP puts it on the wire: an agent that has seen
/// `session/cancel` MUST end the turn with `stopReason: cancelled`. The handle
/// used to discard the whole `PromptResponse` and hardcode `EndTurn`, so a
/// cancelled delegated turn was indistinguishable from a finished one.
#[tokio::test]
async fn acp_cancelled_turn_reports_cancelled_stop_reason() {
    let shapes = acp_shapes_for_cancelled_turn().await;

    assert_eq!(
        shapes.last(),
        Some(&EventShape::Done(StopReason::Cancelled)),
        "an ACP turn the agent ended as cancelled reported a different stop \
         reason, so a stopped turn renders as a completed one; got {shapes:#?}"
    );
}

/// B3: a turn that produced nothing says so.
///
/// `StopReason::Empty` means "the user saw nothing" — no text, no thinking, no
/// tool calls. Before this task *neither* agent reported it for a well-formed
/// contentless turn: `genai_handle.rs` emitted `Empty` at exactly one site, an
/// unexpected stream close, and reported `EndTurn` for a turn that simply said
/// nothing. Both now agree.
///
/// Nothing reads the value yet. `terminal_stop_reason`
/// (`agent_manager/messaging/stream.rs`) is only tested for `is_none()`, and
/// the daemon-proxy path re-fabricates `EndTurn`
/// (`rpc_client/agent/convert.rs`). This pins the contract so the first reader
/// is not silently wrong, exactly as Task 3 did for `ToolBatchEnd`.
#[tokio::test]
async fn acp_empty_turn_reports_empty_stop_reason() {
    let shapes = acp_shapes_for_empty_turn().await;

    assert_eq!(
        shapes,
        vec![EventShape::Done(StopReason::Empty)],
        "a turn with no text, no thinking and no tool calls did not report \
         Empty; got {shapes:#?}"
    );
}

/// B4: a result for a call that was never announced must not invent a name.
///
/// A bare `tool_call_update{status: completed}` — no title, no rawInput, no
/// diff — reaches the handle as a `ToolEnd` with an id the stream never saw a
/// `ToolStart` for. The handle used to name it `"unknown_tool"`, which is a
/// tool that does not exist: it reaches the session transcript and the web view
/// as a real tool result, while the TUI drops it silently (`update_tool`
/// matches nothing and warns, `containers.rs:335-364`) because no card was ever
/// created for that id.
///
/// Dropping it is not free, and the loss is not only cosmetic: the row also
/// never reaches `session.jsonl` (`should_persist` persists `tool_result`),
/// `recording.jsonl`, Lua `tool_result` handlers and redactors, or
/// `cru.sessions.send_and_collect`. It is still the right trade — the row names
/// a `call_id` nothing else in the turn mentions — and the daemon log keeps the
/// payload so the drop stays diagnosable.
///
/// It must also not leave the `ToolBatchEnd` guard lying: a turn that announced
/// no call must not claim a batch.
#[tokio::test]
async fn acp_orphaned_tool_end_does_not_invent_a_tool_name() {
    let shapes = acp_shapes_for_orphaned_tool_end("bare").await;

    assert!(
        !shapes.iter().any(|s| matches!(
            s,
            EventShape::ToolResult { name, .. } if name == "unknown_tool"
        )),
        "a tool_call_update with no matching tool_call produced a result for a \
         fabricated tool; got {shapes:#?}"
    );
    assert_eq!(
        shapes,
        vec![
            EventShape::Text("Done".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "an unnameable result has no card to land in, so the turn must carry \
         neither the result nor a batch end; got {shapes:#?}"
    );
}

/// B4, the recoverable half: the name arrives late, so use it.
///
/// When the orphaned `tool_call_update` carries a title, the client records the
/// call and the handle's post-stream replay announces it — after the result.
/// Emitting the result live would have to guess a name; holding it until the
/// replay names it keeps the real one *and* puts the `ToolCall` before its
/// `ToolResult`, which is the order the renderer needs to build the card first.
#[tokio::test]
async fn acp_orphaned_tool_end_with_a_late_title_reports_the_real_name() {
    let shapes = acp_shapes_for_orphaned_tool_end("titled").await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("Done".into()),
            EventShape::ToolCall {
                call: "call#0".into(),
                name: "Late Named Tool".into(),
                diff_paths: vec![],
            },
            EventShape::ToolResult {
                call: "call#0".into(),
                name: "Late Named Tool".into(),
                is_error: false,
            },
            EventShape::ToolBatchEnd,
            EventShape::Done(StopReason::EndTurn),
        ],
        "a late-titled orphan must be announced before its result and named \
         from the announcement, with the same humanized spelling the live \
         path uses; got {shapes:#?}"
    );
}

/// A second completed update for a call already announced is still its result.
///
/// ACP `tool_call_update` frames are partial-field merges, so an agent may send
/// `completed` more than once for one call — the later frame typically carries
/// the fuller `rawOutput`. Both renderers key a result on the **call id**
/// (`containers.rs::update_tool`, the web's `updateToolMessage`), so the second
/// result lands on the same card and replaces the first. Consuming the recorded
/// name on the first completion turns the second into an unnameable orphan and
/// drops it: the card keeps the partial payload forever, and the row is missing
/// from `session.jsonl`, `recording.jsonl`, Lua `tool_result` handlers and
/// `send_and_collect`.
#[tokio::test]
async fn acp_repeated_completion_for_an_announced_call_keeps_its_name() {
    let shapes = acp_shapes_for_orphaned_tool_end("repeat").await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("Done".into()),
            EventShape::ToolCall {
                call: "call#0".into(),
                name: "Repeated Tool".into(),
                diff_paths: vec![],
            },
            EventShape::ToolResult {
                call: "call#0".into(),
                name: "Repeated Tool".into(),
                is_error: false,
            },
            EventShape::ToolResult {
                call: "call#0".into(),
                name: "Repeated Tool".into(),
                is_error: false,
            },
            EventShape::ToolBatchEnd,
            EventShape::Done(StopReason::EndTurn),
        ],
        "a repeated completed update for an announced call was dropped, so the \
         card keeps the earlier partial payload; got {shapes:#?}"
    );
}

/// A result that arrives before its call is named by the call that follows.
///
/// The bare completed update carries nothing the client records a call from, so
/// it reaches the handle as a `ToolEnd` for an id the stream has not announced
/// *yet* — the naming `tool_call` is still in flight. Deferring the result to
/// the end of the turn is what makes it nameable: by then the `ToolStart` has
/// landed and the live name table answers.
#[tokio::test]
async fn acp_result_arriving_before_its_call_is_named_by_the_later_call() {
    let shapes = acp_shapes_for_orphaned_tool_end("out_of_order").await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("Done".into()),
            EventShape::ToolCall {
                call: "call#0".into(),
                name: "Late Call".into(),
                diff_paths: vec![],
            },
            EventShape::ToolResult {
                call: "call#0".into(),
                name: "Late Call".into(),
                is_error: false,
            },
            EventShape::ToolBatchEnd,
            EventShape::Done(StopReason::EndTurn),
        ],
        "a result whose `tool_call` arrived later in the same turn was dropped \
         even though the turn announced the call; got {shapes:#?}"
    );
}

/// A3: an ACP agent's context window must reach the turn stream.
///
/// The statusline's context indicator needs two numbers, and on a delegated
/// session the daemon can produce neither on its own: `context_limit_resolved`
/// (`server/session/mod.rs`) needs an endpoint and a non-empty model to query,
/// and an ACP session delegates, so it has neither. The window is therefore
/// only knowable from the agent — which puts it on the wire. Both recorded
/// fixtures carry it mid-turn:
///
/// ```text
/// claude:   {"sessionUpdate":"usage_update","used":22700,"size":1000000,…}
/// opencode: {"sessionUpdate":"usage_update","used":28224,"size":200000,…}
/// ```
///
/// Crucible threw it away, and not in the "unmatched arm" way: `usage_update`
/// is gated behind the upstream `unstable_session_usage` feature, which this
/// build does not enable, so the variant is absent from the `SessionUpdate`
/// enum and the whole `SessionNotification` fails to deserialize. The frame
/// died at `tracing::warn!("Failed to parse SessionNotification: …")` in
/// `acp/client/streaming.rs` — before any `match` on the update ran. So the
/// user saw `— ctx` (or at best a bare `22k tok`) where the internal agent
/// shows a percentage.
#[tokio::test]
async fn acp_usage_update_reaches_the_turn_stream() {
    let shapes = acp_shapes_for_usage_update(Some("22700/1000000")).await;

    assert!(
        shapes
            .iter()
            .any(|s| matches!(s, EventShape::ContextWindow { .. })),
        "an ACP agent's usage_update never became a ContextWindow event, so a \
         delegated session has no context limit and the statusline renders its \
         no-data path; got {shapes:#?}"
    );
    assert_eq!(
        shapes,
        vec![
            EventShape::ContextWindow {
                used: 22700,
                limit: 1_000_000,
            },
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "the window must carry both operands, in the wire position the agent \
         put it in; got {shapes:#?}"
    );
}

/// An agent that reports no window must produce no window event.
///
/// `cursor` and `gemini` send no `usage_update` at all in the recorded
/// fixtures. A `ContextWindow { used: 0, limit: 0 }` for them would be worse
/// than silence: `context_label` (`components/status_items.rs`) branches on
/// `limit > 0`, so a zero window is indistinguishable from no window in the
/// frame but *is* a distinct claim on the wire, and any future consumer that
/// divides by it is silently wrong.
#[tokio::test]
async fn acp_turn_without_a_usage_update_reports_no_context_window() {
    let shapes = acp_shapes_for_usage_update(None).await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "an agent that never reported its window still produced one; got \
         {shapes:#?}"
    );
}

/// The mock's usage hook reads a *value*, not the variable's presence.
///
/// A presence check would make an unparseable script report a zeroed window,
/// which is exactly the claim the test above pins as forbidden — so the
/// negative case would silently stop testing anything.
#[tokio::test]
async fn mock_usage_hook_set_to_a_malformed_script_reports_no_window() {
    let shapes = acp_shapes_for_usage_update(Some("not-a-window")).await;

    assert_eq!(
        shapes,
        vec![
            EventShape::Text("The answer is 4".into()),
            EventShape::Done(StopReason::EndTurn),
        ],
        "CRU_MOCK_USAGE_UPDATE=not-a-window must mean no window; got {shapes:#?}"
    );
}
