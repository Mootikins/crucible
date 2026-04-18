use crate::tui::oil::chat_app::messages::ChatAppMsg;
use crate::tui::oil::chat_runner::SessionEventStream;
use serde_json::json;

#[test]
fn replay_skips_full_response_when_granular_deltas_seen() {
    let mut stream = SessionEventStream::new();

    let events = [
        ("user_message", json!({"content": "hi"})),
        ("text_delta", json!({"content": "hello"})),
        ("text_delta", json!({"content": " world"})),
        (
            "message_complete",
            json!({
                "message_id": "m1",
                "full_response": "hello world",
            }),
        ),
    ];

    let msgs: Vec<ChatAppMsg> = events
        .iter()
        .flat_map(|(t, d)| stream.translate(t, d))
        .collect();

    let text_deltas: Vec<&str> = msgs
        .iter()
        .filter_map(|m| match m {
            ChatAppMsg::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    // Exactly the two granular deltas — not the full_response.
    assert_eq!(text_deltas, vec!["hello", " world"]);
    // StreamComplete is still emitted for the turn.
    assert!(msgs.iter().any(|m| matches!(m, ChatAppMsg::StreamComplete)));
}

#[test]
fn resume_uses_full_response_when_no_granular_deltas() {
    let mut stream = SessionEventStream::new();

    let events = [
        ("user_message", json!({"content": "hi"})),
        (
            "message_complete",
            json!({
                "message_id": "m1",
                "full_response": "hello world",
            }),
        ),
    ];

    let msgs: Vec<ChatAppMsg> = events
        .iter()
        .flat_map(|(t, d)| stream.translate(t, d))
        .collect();

    let text_deltas: Vec<&str> = msgs
        .iter()
        .filter_map(|m| match m {
            ChatAppMsg::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(text_deltas, vec!["hello world"]);
}

#[test]
fn state_resets_per_turn() {
    let mut stream = SessionEventStream::new();
    // Turn 1: granular deltas present → skip full_response.
    for ev in &[
        ("user_message", json!({"content": "a"})),
        ("text_delta", json!({"content": "t1"})),
        (
            "message_complete",
            json!({"message_id": "m1", "full_response": "t1"}),
        ),
    ] {
        let _ = stream.translate(ev.0, &ev.1);
    }
    // Turn 2: no granular deltas → must use full_response.
    let mut msgs = Vec::new();
    for ev in &[
        ("user_message", json!({"content": "b"})),
        (
            "message_complete",
            json!({"message_id": "m2", "full_response": "r2"}),
        ),
    ] {
        msgs.extend(stream.translate(ev.0, &ev.1));
    }
    let deltas: Vec<&str> = msgs
        .iter()
        .filter_map(|m| match m {
            ChatAppMsg::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["r2"]);
}

#[test]
fn subagent_spawned_maps_to_chat_msg() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "subagent_spawned",
        &json!({
            "job_id": "sa1",
            "session_link": "crucible://session/child1",
            "prompt": "analyze the code",
        }),
    );
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::SubagentSpawned { id, prompt } => {
            assert_eq!(id, "sa1");
            assert_eq!(prompt, "analyze the code");
        }
        other => panic!("Expected SubagentSpawned, got {:?}", other),
    }
}

#[test]
fn subagent_completed_maps_to_chat_msg() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "subagent_completed",
        &json!({
            "job_id": "sa2",
            "session_link": "crucible://session/child2",
            "summary": "done with analysis",
        }),
    );
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::SubagentCompleted { id, summary } => {
            assert_eq!(id, "sa2");
            assert_eq!(summary, "done with analysis");
        }
        other => panic!("Expected SubagentCompleted, got {:?}", other),
    }
}

#[test]
fn subagent_failed_maps_to_chat_msg() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "subagent_failed",
        &json!({
            "job_id": "sa3",
            "session_link": "crucible://session/child3",
            "error": "timeout",
        }),
    );
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::SubagentFailed { id, error } => {
            assert_eq!(id, "sa3");
            assert_eq!(error, "timeout");
        }
        other => panic!("Expected SubagentFailed, got {:?}", other),
    }
}

#[test]
fn message_complete_with_token_counts_emits_context_usage() {
    let mut stream = SessionEventStream::new();
    // No granular deltas: text_delta path is empty.
    let msgs = stream.translate(
        "message_complete",
        &json!({
            "message_id": "m1",
            "full_response": "hi",
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150,
        }),
    );
    // Expect TextDelta + ContextUsage + StreamComplete.
    let has_context_usage = msgs.iter().any(|m| {
        matches!(
            m,
            ChatAppMsg::ContextUsage {
                used: 150,
                total: _
            }
        )
    });
    assert!(
        has_context_usage,
        "Expected ContextUsage(used=150) in msgs: {:?}",
        msgs
    );
}

#[test]
fn message_complete_without_token_counts_does_not_emit_context_usage() {
    let mut stream = SessionEventStream::new();
    let msgs = stream.translate(
        "message_complete",
        &json!({
            "message_id": "m1",
            "full_response": "hi",
        }),
    );
    let has_context_usage = msgs
        .iter()
        .any(|m| matches!(m, ChatAppMsg::ContextUsage { .. }));
    assert!(
        !has_context_usage,
        "Did not expect ContextUsage without token counts: {:?}",
        msgs
    );
}

#[test]
fn delegation_fixture_renders_without_duplication() {
    use std::fs::read_to_string;

    let jsonl =
        read_to_string("../../assets/fixtures/delegation-demo.jsonl").expect("fixture present");
    let mut stream = SessionEventStream::new();
    let mut all_msgs = Vec::new();

    for line in jsonl.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(event_type) = v.get("event").and_then(|x| x.as_str()) else {
            continue;
        };
        let data = v.get("data").cloned().unwrap_or_default();
        all_msgs.extend(stream.translate(event_type, &data));
    }

    let text: String = all_msgs
        .iter()
        .filter_map(|m| match m {
            ChatAppMsg::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    // Invariant: no sentence from the assistant's final response should
    // appear more than once in the assembled text. Previously the fixture
    // rendered duplicated content because tool-result text bled into the
    // text_delta stream without a newline separator. Pick a phrase unique
    // to the fixture's final response and verify it appears at most once.
    let needle = "Crucible data store";
    let occurrences = text.matches(needle).count();
    assert!(
        occurrences <= 1,
        "assembled text duplicates '{needle}': occurrences={occurrences}"
    );
}
