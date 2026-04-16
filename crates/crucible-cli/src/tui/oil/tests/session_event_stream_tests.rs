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
fn delegation_fixture_renders_without_duplication() {
    use std::fs::read_to_string;

    let jsonl = read_to_string("../../assets/fixtures/delegation-demo.jsonl")
        .expect("fixture present");
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

    // The response text should appear exactly once.
    let occurrences = text.matches("Agent profiles").count();
    assert_eq!(occurrences, 1, "assembled text duplicates 'Agent profiles'");
}
