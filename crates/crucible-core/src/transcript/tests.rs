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
            // A recording names the time `ts`; a stored line names it
            // `timestamp`.
            message.timestamp = line
                .get("ts")
                .or_else(|| line.get("timestamp"))
                .and_then(|ts| serde_json::from_value(ts.clone()).ok());
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
    // Each event has its own time, so the test also proves that an item
    // takes the same time from the live stream and from the log.
    let start: chrono::DateTime<chrono::Utc> = "2026-09-01T10:00:00Z".parse().unwrap();
    let live: Vec<SessionEventMessage> = live_turn()
        .into_iter()
        .enumerate()
        .map(|(i, mut e)| {
            e.timestamp = Some(start + chrono::Duration::seconds(i as i64));
            e
        })
        .collect();
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
        ItemBody::UserTurn { precognition: Some(p), .. }
            if p.notes_count == 2 && p.query_summary == "q"
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

/// A segment names the model of the session when it started. The first
/// model comes from `session_initialized`; an empty name is no answer.
#[test]
fn a_segment_names_the_model_and_the_usage_of_its_turn() {
    let transcript = TranscriptFold::of_events(&[
        event(
            "session_initialized",
            json!({"model": "", "mode": "normal", "agent_name": null, "workspace_path": "/w"}),
        ),
        event("user_message", json!({"message_id": "m1", "content": "q"})),
        event(
            "message_complete",
            json!({"message_id": "m1", "full_response": "a"}),
        ),
        event(
            "session_initialized",
            json!({"model": "first", "mode": "normal", "agent_name": null, "workspace_path": "/w"}),
        ),
        event("user_message", json!({"message_id": "m2", "content": "q"})),
        event(
            "message_complete",
            json!({"message_id": "m2", "full_response": "b", "prompt_tokens": 10,
                   "completion_tokens": 5, "total_tokens": 15, "cache_read_tokens": 3}),
        ),
        event(
            "model_switched",
            json!({"model_id": "second", "provider": "p"}),
        ),
        event("user_message", json!({"message_id": "m3", "content": "q"})),
        event(
            "message_complete",
            json!({"message_id": "m3", "full_response": "c"}),
        ),
    ]);
    let segment = |id: &str| match &transcript.items.iter().find(|i| i.id == id).unwrap().body {
        ItemBody::AssistantSegment { model, usage, .. } => (model.clone(), *usage),
        other => panic!("{id} is not a segment: {other:?}"),
    };
    assert_eq!(segment("m1-seg-0").0, None);
    assert_eq!(segment("m2-seg-0").0.as_deref(), Some("first"));
    assert_eq!(segment("m2-seg-0").1.unwrap().cache_read_tokens, Some(3));
    assert_eq!(segment("m3-seg-0").0.as_deref(), Some("second"));
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

/// The rows that every client draws for each golden transcript: the TUI, the
/// web client and `cru acp`. Each client test draws the golden transcript
/// and compares its own rows with this file, so the three clients agree on
/// the turns, the segments and the tool cards. The rules:
///
/// - a user turn is `user:<content>`;
/// - an answer segment is `segment:<words>`, the first 32 letters and digits
///   of its text, because a client renders markdown; a segment with no text
///   and no reasoning draws nothing;
/// - a tool card is `tool:<call id>`;
/// - a notice is `notice`;
/// - a delegation and injected context draw no row.
///
/// `CRUCIBLE_WRITE_GOLDEN=1` writes the files instead of comparing them.
#[test]
fn each_golden_transcript_has_its_client_rows() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/golden/transcript");
    let write = std::env::var_os("CRUCIBLE_WRITE_GOLDEN").is_some();
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    for path in names {
        let transcript: Transcript =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let rows: Vec<String> = transcript.items.iter().filter_map(client_row).collect();
        let got = serde_json::to_string_pretty(&rows).unwrap();
        let rows_path = dir.join("rows").join(path.file_name().unwrap());
        if write {
            std::fs::create_dir_all(rows_path.parent().unwrap()).unwrap();
            std::fs::write(&rows_path, format!("{got}\n")).unwrap();
            continue;
        }
        let golden = std::fs::read_to_string(&rows_path).unwrap_or_default();
        assert_eq!(
            got.trim(),
            golden.trim(),
            "{} differs. The rows are:\n{got}",
            rows_path.display()
        );
    }
}

fn client_row(item: &TranscriptItem) -> Option<String> {
    match &item.body {
        ItemBody::UserTurn { content, .. } => Some(format!("user:{content}")),
        ItemBody::AssistantSegment { text, thinking, .. } => {
            if text.trim().is_empty() && thinking.is_empty() {
                return None;
            }
            let words: String = text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .take(32)
                .collect();
            Some(format!("segment:{words}"))
        }
        ItemBody::ToolCard { call_id, .. } => Some(format!("tool:{call_id}")),
        ItemBody::Notice { .. } => Some("notice".to_string()),
        ItemBody::Delegation { .. } | ItemBody::InjectedContext { .. } => None,
    }
}

/// Each recording that carries an end-of-stream reasoning replay keeps
/// exactly its other thoughts. Each number is the count of `thinking` events
/// in the fixture minus its replays, counted against the raw JSONL:
///
///   demo                  68 thinking events −  2 replays (156, 119 chars)
///   parity-test           59                 −  1        (266)
///   reproduce            173                 −  4        (515, 79, 161, 87)
///   reproduce-formatting 162                 −  4        (272, 146, 183, 144)
///
/// A constant has no model of the rule to get wrong. It fails when a replay
/// is kept, and when the run is not consumed on a match, because a later
/// replay in the same turn then stops matching.
#[test]
fn recorded_fixtures_keep_exactly_their_non_replayed_thoughts() {
    for (name, expected) in [
        ("demo.jsonl", 66),
        ("parity-test.jsonl", 58),
        ("reproduce.jsonl", 169),
        ("reproduce-formatting.jsonl", 158),
    ] {
        let mut fold = TranscriptFold::new();
        let kept = fixture(name)
            .iter()
            .flat_map(|event| fold.apply(event))
            .filter(|op| {
                matches!(
                    op,
                    TranscriptOp::Append {
                        field: TextField::Thinking,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(kept, expected, "{name}");
    }
}

/// The segments of a turn, joined, are the whole answer that
/// `message_complete` carries: no text shows twice, and none is lost. Tool
/// output once bled into the answer text of the delegation recording.
#[test]
fn the_segments_of_a_turn_join_into_its_whole_answer() {
    // `demo.jsonl` is not here: its recording scrubbed a home path in the
    // whole answer but not in the streamed deltas.
    for name in [
        "delegation-demo.jsonl",
        "acp_parity_internal.jsonl",
        "acp_parity_delegated.jsonl",
        "session_log_wire.jsonl",
    ] {
        let events = fixture(name);
        let full: String = events
            .iter()
            .filter_map(|e| match e.payload() {
                Ok(SessionEventPayload::Turn(TurnPayload::MessageComplete {
                    full_response,
                    ..
                })) => Some(full_response),
                _ => None,
            })
            .collect();
        let transcript = TranscriptFold::of_events(&events);
        let joined: String = transcript
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ItemBody::AssistantSegment { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!full.is_empty(), "{name}: the recording has an answer");
        assert_eq!(joined, full, "{name}");
    }
}
