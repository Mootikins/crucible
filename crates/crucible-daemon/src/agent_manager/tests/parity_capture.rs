//! Provenance for `assets/fixtures/acp_parity_*.jsonl`.
//!
//! The parity fixtures are `SessionEvent` JSONL that `crucible-cli`'s story
//! tests pump through the renderer to prove a delegated turn and an internal
//! one paint the same frame. They were originally built by a throwaway capture
//! harness that was written, run, and deleted — which left nobody watching the
//! producers. `tool_call_with_metadata`, the `agent_owns_tools` pass-through
//! arm in `messaging/stream.rs`, and `call_tool_result_to_value` could all
//! change shape while the CLI tests stayed green against a shape the daemon had
//! stopped emitting. That is exactly the `CrucibleClient` failure mode
//! (divergence D1) the parity plan indicts: tests passing against a payload
//! production never produces.
//!
//! So the capture lives here instead of in a scratch file. Each test drives the
//! same `ReactorTestHarness` the original capture used — `StreamingMockAgent`
//! against the real `WorkspaceTools` dispatcher and the real permission gate for
//! the internal arm, `OwnsToolsMockAgent` with `agent_name: "claude"` for the
//! delegated arm — and asserts the broadcast events equal the committed bytes.
//! Change an emitter and these fail; regenerate with
//! `scripts/gen-acp-parity-fixtures.py` once you have decided the new shape is
//! right.
//!
//! **Deliberately not `#[ignore]`d.** The project gates tests behind `#[ignore]`
//! when they need something the machine may not have — a running daemon, Ollama,
//! an agent binary. These need none of that: they run in-process against mocks
//! and a `TempDir`. Gating them would defeat the whole point, which is that CI
//! notices the drift.

use super::*;
use crucible_core::interaction::PermResponse;
use crucible_core::types::acp::FileDiff;
use serde_json::Value;
use std::sync::{Arc as CaptureArc, Mutex as CaptureMutex};

/// Broadcast events the fixtures deliberately leave out.
///
/// `post_llm_call` is reactor telemetry: it carries a wall-clock `duration_ms`,
/// has no arm in `session_event_to_chat_msgs`, and paints no pixels. Pinning it
/// would make the fixtures time-dependent for no coverage. Nothing else is
/// filtered, so a *new* event type breaks the comparison instead of vanishing.
const OMITTED_FROM_FIXTURES: &[&str] = &["post_llm_call"];

const GREETING: &str = "fn main() {\n    println!(\"hello\");\n}\n";

/// What `read_file` answers for [`GREETING`]: `cat -n` gutters plus a counter.
/// Pinned against the real tool by
/// [`the_delegated_read_output_is_what_read_file_actually_returns`].
const READ_OUTPUT: &str =
    "     1\tfn main() {\n     2\t    println!(\"hello\");\n     3\t}\n\n[3 lines read, 3 total]";

fn edit_args() -> Value {
    serde_json::json!({
        "path": "greeting.rs",
        "old_string": "hello",
        "new_string": "hello, world",
    })
}

/// The `rawInput` shape a delegated Claude Code edit arrives with.
fn edit_args_acp() -> Value {
    serde_json::json!({
        "file_path": "greeting.rs",
        "old_string": "hello",
        "new_string": "hello, world",
    })
}

fn read_args() -> Value {
    serde_json::json!({ "path": "greeting.rs" })
}

fn read_args_acp() -> Value {
    serde_json::json!({ "file_path": "greeting.rs" })
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures")
}

/// The `(event, data)` sequence a committed fixture describes.
fn fixture_events(name: &str) -> Vec<(String, Value)> {
    let path = fixtures_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("parse {} line `{line}`: {e}", path.display()));
            // The recording header and footer carry no `event`.
            let event = value.get("event")?.as_str()?.to_string();
            Some((event, value.get("data").cloned().unwrap_or(Value::Null)))
        })
        .collect()
}

/// Project captured broadcast events onto the fixture's vocabulary.
///
/// `substitutions` replace the run-to-run identifiers (the generated message id,
/// the permission uuid) with the stable ones the fixture spells, so the
/// comparison is about shape and content rather than entropy.
fn as_fixture_events(
    captured: &[SessionEventMessage],
    substitutions: &[(String, &str)],
) -> Vec<(String, Value)> {
    captured
        .iter()
        .filter(|event| !OMITTED_FROM_FIXTURES.contains(&event.event.as_str()))
        .map(|event| {
            let mut json = event.data.to_string();
            for (from, to) in substitutions {
                json = json.replace(from.as_str(), to);
            }
            let data = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("re-parse normalized `{json}`: {e}"));
            (event.event.clone(), data)
        })
        .collect()
}

fn assert_matches_fixture(
    captured: &[SessionEventMessage],
    substitutions: &[(String, &str)],
    fixture: &str,
) {
    let actual = as_fixture_events(captured, substitutions);
    let expected = fixture_events(fixture);
    assert_eq!(
        actual.iter().map(|(e, _)| e.as_str()).collect::<Vec<_>>(),
        expected.iter().map(|(e, _)| e.as_str()).collect::<Vec<_>>(),
        "the daemon no longer emits the event sequence `{fixture}` records"
    );
    for ((event, actual), (_, expected)) in actual.iter().zip(expected.iter()) {
        assert_eq!(
            actual, expected,
            "`{fixture}` pins a `{event}` payload the daemon no longer emits. \
             If the new shape is right, regenerate with \
             `scripts/gen-acp-parity-fixtures.py` — and re-read the frames the \
             CLI story tests render from it before accepting."
        );
    }
}

/// Drain everything the turn broadcast, in order.
///
/// `Lagged` panics rather than ending the loop. A silent truncation here
/// surfaces as "the daemon no longer emits the event sequence" — a fixture
/// mismatch pointing at production code that is fine — and the fix is to raise
/// the channel capacity, not to re-record the fixture. Capacity is 256 against
/// ~9 events, so this is a tripwire, not a live risk.
fn drain(rx: &mut broadcast::Receiver<SessionEventMessage>) -> Vec<SessionEventMessage> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => out.push(event),
            Err(broadcast::error::TryRecvError::Empty)
            | Err(broadcast::error::TryRecvError::Closed) => return out,
            Err(broadcast::error::TryRecvError::Lagged(n)) => panic!(
                "the session broadcast dropped {n} event(s) before this capture read them, \
                 so the sequence below is truncated and any fixture mismatch it reports is \
                 an artefact. Raise the channel capacity."
            ),
        }
    }
}

async fn configure_delegated(h: &ReactorTestHarness) {
    h.reconfigure(SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("claude".to_string()),
        ..test_agent()
    })
    .await;
}

#[tokio::test]
async fn internal_edit_fixture_matches_a_live_capture() {
    let h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("greeting.rs"), GREETING).unwrap();
    // The same synthesizer the permission gate and `GenaiAgentHandle` both call.
    // The mock stands in for the handle, so it supplies what the handle would;
    // everything after that is the daemon's own work.
    let diffs: Vec<FileDiff> =
        crate::tools::diff_synth::synthesize_diffs("edit_file", &edit_args(), h.workspace());
    assert_eq!(diffs.len(), 1, "the edit fixture needs a synthesized diff");

    h.inject_streaming_agent(vec![
        script::text("I'll fix the greeting."),
        TurnEvent::ToolCall {
            id: "call-edit-1".to_string(),
            name: "edit_file".to_string(),
            args: edit_args(),
            diffs,
        },
        script::text(" Done."),
    ]);

    let mut collected = h.event_tx.subscribe();
    let mut watcher = h.event_tx.subscribe();
    let granted: CaptureArc<CaptureMutex<Option<String>>> =
        CaptureArc::new(CaptureMutex::new(None));

    // `send_message` returns as soon as the turn is scheduled, so the gate has
    // to be answered concurrently — otherwise `edit_file` waits out its 300 s
    // permission timeout and the capture records a denial.
    let send = h.agent_manager.send_message(
        &h.session_id,
        "fix the greeting".to_string(),
        &h.event_tx,
        true,
        None,
    );
    let approve = async {
        let request = next_event_or_skip(&mut watcher, "interaction_requested").await;
        let id = request.data["request_id"]
            .as_str()
            .expect("permission request carries a request_id")
            .to_string();
        *granted.lock().unwrap() = Some(id.clone());
        h.agent_manager
            .respond_to_permission(&h.session_id, &id, PermResponse::allow())
            .expect("respond to the gate the internal edit opened");
    };
    let (message_id, ()) = tokio::join!(send, approve);
    let message_id = message_id.unwrap();
    let _ = next_event_or_skip(&mut watcher, "message_complete").await;

    let permission_id = granted.lock().unwrap().clone().unwrap();
    assert_matches_fixture(
        &drain(&mut collected),
        &[
            (message_id, "msg-parity-0001"),
            (permission_id, "perm-parity-0001"),
        ],
        "acp_parity_internal.jsonl",
    );
}

#[tokio::test]
async fn delegated_edit_fixture_matches_a_live_capture() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("greeting.rs"), GREETING).unwrap();
    let diffs: Vec<FileDiff> =
        crate::tools::diff_synth::synthesize_diffs("edit_file", &edit_args(), h.workspace());

    configure_delegated(&h).await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::text("I'll fix the greeting."),
            TurnEvent::ToolCall {
                id: "call-edit-1".to_string(),
                name: "Edit File".to_string(),
                args: edit_args_acp(),
                // Claude Code announces the call with empty content and
                // attaches the diff in a follow-up `tool_call_update`.
                diffs: Vec::new(),
            },
            TurnEvent::ToolCallDiffUpdate {
                id: "call-edit-1".to_string(),
                diffs,
            },
            script::tool_result("call-edit-1", "Edit File", "Replaced 1 occurrence(s)"),
            script::text(" Done."),
            script::done(),
        ],
    }));

    let mut collected = h.event_tx.subscribe();
    let message_id = h.send("fix the greeting").await;
    let _ = h.wait_for("message_complete").await;

    assert_matches_fixture(
        &drain(&mut collected),
        &[(message_id, "msg-parity-0001")],
        "acp_parity_delegated.jsonl",
    );
}

#[tokio::test]
async fn internal_read_fixture_matches_a_live_capture() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("greeting.rs"), GREETING).unwrap();

    h.inject_streaming_agent(vec![
        script::text("Let me look."),
        TurnEvent::ToolCall {
            id: "call-read-1".to_string(),
            name: "read_file".to_string(),
            args: read_args(),
            diffs: Vec::new(),
        },
        script::text(" That's the greeting."),
    ]);

    let mut collected = h.event_tx.subscribe();
    let message_id = h.send("read the greeting").await;
    let _ = h.wait_for("message_complete").await;

    assert_matches_fixture(
        &drain(&mut collected),
        &[(message_id, "msg-parity-0002")],
        "acp_parity_read_internal.jsonl",
    );
}

#[tokio::test]
async fn delegated_read_fixture_matches_a_live_capture() {
    let mut h = ReactorTestHarness::new().await;
    std::fs::write(h.workspace().join("greeting.rs"), GREETING).unwrap();

    configure_delegated(&h).await;
    h.inject_agent(Box::new(OwnsToolsMockAgent {
        events: vec![
            script::text("Let me look."),
            TurnEvent::ToolCall {
                id: "call-read-1".to_string(),
                name: "Read File".to_string(),
                args: read_args_acp(),
                diffs: Vec::new(),
            },
            script::tool_result("call-read-1", "Read File", READ_OUTPUT),
            script::text(" That's the greeting."),
            script::done(),
        ],
    }));

    let mut collected = h.event_tx.subscribe();
    let message_id = h.send("read the greeting").await;
    let _ = h.wait_for("message_complete").await;

    assert_matches_fixture(
        &drain(&mut collected),
        &[(message_id, "msg-parity-0002")],
        "acp_parity_read_delegated.jsonl",
    );
}

/// The delegated read fixture quotes the *internal* tool's own answer, so the
/// pair compares presentation rather than two tools' formatting. Pin that it
/// really is what `read_file` returns — otherwise the read pair could drift
/// into comparing a delegated agent against a fiction.
#[tokio::test]
async fn the_delegated_read_output_is_what_read_file_actually_returns() {
    let (tmp, session_manager, session) = setup_session_manager().await;
    std::fs::write(tmp.path().join("greeting.rs"), GREETING).unwrap();
    let agent_manager = create_test_agent_manager(session_manager);
    let dispatcher = agent_manager
        .get_or_create_session_dispatcher(&session)
        .await;

    let value = dispatcher
        .dispatch_tool("read_file", read_args(), Default::default())
        .await
        .expect("read_file dispatches");

    assert_eq!(
        value.get("result").and_then(Value::as_str),
        Some(READ_OUTPUT),
        "the delegated read fixture quotes an output `read_file` no longer produces"
    );
}
