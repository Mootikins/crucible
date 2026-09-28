use super::*;
use serde_json::json;

fn event(name: &str, data: Value) -> SessionEventMessage {
    SessionEventMessage::new("s1", name, data)
}

/// The events of one fixture, in both line formats: a recording (a header,
/// then `{ts, seq, event, data}`) and stored wire lines.
fn fixture(name: &str) -> Vec<SessionEventMessage> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| {
            let name = line.get("event")?.as_str()?.to_string();
            let mut message =
                SessionEventMessage::new("s1", name, line.get("data").cloned().unwrap_or_default());
            message.seq = line.get("seq").and_then(Value::as_u64);
            Some(message)
        })
        .collect()
}

/// A turn with reasoning, narration, a tool, an update, a result and a
/// final answer — the live stream of it.
fn live_turn() -> Vec<SessionEventMessage> {
    vec![
        event(
            "user_message",
            json!({"message_id": "m1", "content": "read it"}),
        ),
        event("thinking", json!({"content": "look at "})),
        event("thinking", json!({"content": "the file"})),
        event("text_delta", json!({"content": "I will "})),
        event("text_delta", json!({"content": "read it."})),
        event(
            "segment_complete",
            json!({"message_id": "m1", "index": 0, "content": "I will read it."}),
        ),
        event(
            "tool_call",
            json!({"call_id": "c1", "tool": "read_file", "args": {}}),
        ),
        event(
            "tool_call_update",
            json!({"call_id": "c1", "args": {"path": "a.rs"}}),
        ),
        event(
            "tool_result",
            json!({"call_id": "c1", "tool": "read_file", "result": {"result": "fn a() {}"}}),
        ),
        event("text_delta", json!({"content": "It has one function."})),
        event(
            "message_complete",
            json!({"message_id": "m1", "full_response": "I will read it.It has one function.",
                   "total_tokens": 12}),
        ),
        event("turn_finished", json!({"status": "completed"})),
    ]
}

fn kinds(transcript: &Transcript) -> Vec<String> {
    transcript
        .items
        .iter()
        .map(|item| {
            let kind = match &item.body {
                ItemBody::UserTurn { .. } => "user",
                ItemBody::AssistantSegment { .. } => "segment",
                ItemBody::ToolCard { .. } => "tool",
                ItemBody::Delegation { .. } => "delegation",
                ItemBody::InjectedContext { .. } => "context",
                ItemBody::Notice { .. } => "notice",
            };
            format!("{kind}:{}", item.id)
        })
        .collect()
}

fn segment_text(transcript: &Transcript, id: &str) -> (String, String) {
    match transcript
        .items
        .iter()
        .find(|i| i.id == id)
        .map(|i| &i.body)
    {
        Some(ItemBody::AssistantSegment { text, thinking, .. }) => (text.clone(), thinking.clone()),
        other => panic!("{id} is not a segment: {other:?}"),
    }
}

#[test]
fn a_live_turn_folds_into_ordered_segments_and_a_card() {
    let transcript = TranscriptFold::of_events(&live_turn());
    assert_eq!(
        kinds(&transcript),
        [
            "user:m1",
            "segment:m1-seg-0",
            "tool:tool-c1",
            "segment:m1-seg-1"
        ]
    );
    assert_eq!(
        segment_text(&transcript, "m1-seg-0"),
        ("I will read it.".into(), "look at the file".into())
    );
    assert_eq!(
        segment_text(&transcript, "m1-seg-1").0,
        "It has one function."
    );
    let ItemBody::ToolCard {
        args,
        status,
        result,
        ..
    } = &transcript.items[2].body
    else {
        panic!("not a card");
    };
    assert_eq!(
        args,
        &json!({"path": "a.rs"}),
        "the update gives the arguments"
    );
    assert_eq!(*status, ToolStatus::Complete);
    assert_eq!(result.as_deref(), Some("fn a() {}"));
    assert!(transcript.items.iter().all(|i| !matches!(
        i.body,
        ItemBody::AssistantSegment {
            streaming: true,
            ..
        }
    )));
}

/// A stored log keeps no text deltas. Its fold must still put each part of
/// the answer where the live fold put it — before a resume moved all the
/// narration below the tools.
#[test]
fn the_stored_events_fold_into_the_live_transcript() {
    let live = live_turn();
    let stored: Vec<SessionEventMessage> = live
        .iter()
        .filter(|e| e.payload().is_ok_and(|p| p.is_persisted()))
        .cloned()
        .collect();
    assert!(stored.len() < live.len(), "the stored log drops the deltas");
    assert_eq!(
        TranscriptFold::of_events(&stored),
        TranscriptFold::of_events(&live)
    );
}

/// The ops of each event, applied to an empty transcript, give the snapshot.
/// That is what a client that holds a snapshot and follows the stream sees.
#[test]
fn the_ops_rebuild_the_snapshot() {
    for name in [
        "session_log_wire.jsonl",
        "acp_parity_internal.jsonl",
        "acp_parity_delegated.jsonl",
        "demo.jsonl",
        "reproduce.jsonl",
        "delegation-demo.jsonl",
        "undo_flow.jsonl",
        "permission_flow.jsonl",
        "malformed-acp-recording.jsonl",
    ] {
        let events = fixture(name);
        let mut fold = TranscriptFold::new();
        let mut followed = Transcript::default();
        for event in &events {
            for op in fold.apply(event) {
                assert!(followed.apply(&op), "{name}: an op did not fit: {op:?}");
            }
        }
        followed.as_of_seq = fold.snapshot().as_of_seq;
        assert_eq!(followed, fold.snapshot(), "{name}");
        assert!(
            !fold.snapshot().items.is_empty(),
            "{name}: the fold drew nothing"
        );
    }
}

/// A provider that streams reasoning can send the whole block again at the
/// end. The repeat is dropped; a short repeat of one thought stays.
#[test]
fn a_reasoning_replay_is_dropped_and_a_repeated_thought_stays() {
    let replayed = TranscriptFold::of_events(&[
        event("user_message", json!({"message_id": "m1", "content": "q"})),
        event("thinking", json!({"content": "a"})),
        event("thinking", json!({"content": "b"})),
        event("thinking", json!({"content": "ab"})),
    ]);
    assert_eq!(segment_text(&replayed, "m1-seg-0").1, "ab");

    let repeated = TranscriptFold::of_events(&[
        event("user_message", json!({"message_id": "m1", "content": "q"})),
        event("thinking", json!({"content": "a"})),
        event("thinking", json!({"content": "a"})),
    ]);
    assert_eq!(segment_text(&repeated, "m1-seg-0").1, "aa");
}

#[test]
fn a_tool_that_never_answered_is_incomplete_and_a_failed_turn_says_so() {
    let transcript = TranscriptFold::of_events(&[
        event("user_message", json!({"message_id": "m1", "content": "q"})),
        event(
            "tool_call",
            json!({"call_id": "c1", "tool": "bash", "args": {}}),
        ),
        event(
            "turn_finished",
            json!({"status": "failed", "error": "provider down"}),
        ),
    ]);
    assert!(matches!(
        transcript.items[1].body,
        ItemBody::ToolCard {
            status: ToolStatus::Incomplete,
            ..
        }
    ));
    assert_eq!(
        transcript.items[2].body,
        ItemBody::Notice {
            notice: Notice::TurnFailed {
                status: TurnStatus::Failed,
                error: Some("provider down".into())
            }
        }
    );
}

/// Injected context goes before the first turn after its anchor, also when
/// the stored line comes later in the log.
#[test]
fn injected_context_goes_before_the_turn_after_its_anchor() {
    let transcript = TranscriptFold::of_events(&[
        event(
            "user_message",
            json!({"message_id": "m1", "content": "one"}),
        ),
        event(
            "user_message",
            json!({"message_id": "m2", "content": "two"}),
        ),
        event(
            "context_injected",
            json!({"role": "system", "content": "ctx", "after_turn": "m1"}),
        ),
    ]);
    assert_eq!(
        kinds(&transcript),
        ["user:m1", "context:context-2", "user:m2"]
    );
}

#[test]
fn precognition_joins_its_turn_and_a_stop_reason_gets_a_notice() {
    let transcript = TranscriptFold::of_events(&[
        event("user_message", json!({"message_id": "m1", "content": "q"})),
        event(
            "precognition_complete",
            json!({"notes_count": 2, "query_summary": "q"}),
        ),
        event(
            "message_complete",
            json!({"message_id": "m1", "full_response": "cut", "stop_reason": "max_tokens"}),
        ),
    ]);
    assert!(matches!(
        &transcript.items[0].body,
        ItemBody::UserTurn { precognition: Some(p), .. } if p.notes_count == 2
    ));
    assert!(matches!(
        &transcript.items[2].body,
        ItemBody::Notice {
            notice: Notice::StopReason {
                reason: StopReason::MaxTokens,
                ..
            }
        }
    ));
}

#[test]
fn an_append_that_does_not_fit_asks_for_a_new_snapshot() {
    let mut transcript = TranscriptFold::of_events(&live_turn()[..4]);
    let gap = TranscriptOp::Append {
        id: "m1-seg-0".into(),
        field: TextField::Text,
        at: 999,
        text: "x".into(),
    };
    assert!(!transcript.apply(&gap));
    let unknown = TranscriptOp::Append {
        id: "nothing".into(),
        field: TextField::Text,
        at: 0,
        text: "x".into(),
    };
    assert!(!transcript.apply(&unknown));
}

#[test]
fn the_closed_prefix_comes_off_the_whole_answer() {
    assert_eq!(strip_closed_prefix("ab cd", &["ab ".into()]), "cd");
    // A lost space at the seam still matches.
    assert_eq!(strip_closed_prefix("abcd", &["ab ".into()]), "cd");
    // Any other mismatch keeps the whole text.
    assert_eq!(strip_closed_prefix("xyz", &["ab".into()]), "xyz");
}

/// The fold of each recorded session, kept beside the fixture. A change of
/// the fold shows here as a diff, and a person reads it before accepting it.
/// `CRUCIBLE_WRITE_GOLDEN=1` writes the files instead of comparing them.
#[test]
fn the_fold_of_each_recording_matches_its_golden_file() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/golden/transcript");
    let write = std::env::var_os("CRUCIBLE_WRITE_GOLDEN").is_some();
    for name in [
        "session_log_wire.jsonl",
        "acp_parity_internal.jsonl",
        "acp_parity_delegated.jsonl",
        "reproduce.jsonl",
        "delegation-demo.jsonl",
    ] {
        let transcript = TranscriptFold::of_events(&fixture(name));
        let got = serde_json::to_string_pretty(&transcript).unwrap();
        let path = dir.join(name.replace(".jsonl", ".json"));
        if write {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, format!("{got}\n")).unwrap();
            continue;
        }
        let golden = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            got.trim(),
            golden.trim(),
            "{} differs. The fold gives:\n{got}",
            path.display()
        );
    }
}
