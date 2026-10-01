#[path = "common/allocations.rs"]
mod allocations;

use crucible_core::protocol::SessionEventMessage;
use crucible_core::transcript::{ItemBody, TextField, TranscriptFold, TranscriptOp};
use serde_json::json;

#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;

#[test]
fn streaming_deltas_do_not_copy_accumulated_text_or_thinking() {
    let mut fold = TranscriptFold::new();
    let prefix = "x".repeat(256 * 1024);
    for name in ["text_delta", "thinking"] {
        fold.apply(&SessionEventMessage::new(
            "s1",
            name,
            json!({"content": prefix}),
        ));
    }
    let events: Vec<_> = (0..128)
        .map(|index| {
            SessionEventMessage::new(
                "s1",
                if index % 2 == 0 {
                    "text_delta"
                } else {
                    "thinking"
                },
                json!({"content": "é"}),
            )
        })
        .collect();
    let bytes = allocations::allocated_bytes(|| {
        for (index, event) in events.iter().enumerate() {
            let ops = fold.apply(event);
            assert!(matches!(
                ops.as_slice(),
                [TranscriptOp::Append { field, at, text, .. }]
                    if *field == if index % 2 == 0 { TextField::Text } else { TextField::Thinking }
                        && *at == prefix.len() + (index / 2) * "é".len()
                        && text == "é"
            ));
        }
    });
    println!("128 streaming deltas allocated {bytes} bytes");
    // Allows both String buffers to grow once, plus event decoding and ops,
    // but cannot accommodate copying either growing field on every delta.
    assert!(bytes < 2 * 1024 * 1024, "streaming allocated {bytes} bytes");
    let snapshot = fold.snapshot();
    let ItemBody::AssistantSegment { text, thinking, .. } = &snapshot.items[0].body else {
        panic!("expected an assistant segment");
    };
    let expected = prefix + &"é".repeat(64);
    assert_eq!(text, &expected);
    assert_eq!(thinking, &expected);
}

#[test]
fn twenty_long_sessions_stream_without_copying_their_history() {
    const SESSIONS: usize = 20;
    const TURNS: usize = 100;
    const WAVES: usize = 4;
    const DELTAS: usize = 128;
    let answer = "a".repeat(32 * 1024);
    let reasoning = "r".repeat(8 * 1024);
    let prefix = "p".repeat(256 * 1024);
    let mut sessions: Vec<_> = (0..SESSIONS)
        .map(|index| (format!("s{index}"), TranscriptFold::new()))
        .collect();
    for (id, fold) in &mut sessions {
        for turn in 0..TURNS {
            for (name, data) in [
                (
                    "user_message",
                    json!({"message_id": format!("m{turn}"), "content": "continue"}),
                ),
                ("thinking", json!({"content": reasoning})),
                ("message_complete", json!({"full_response": answer})),
                ("turn_finished", json!({"status": "completed"})),
            ] {
                fold.apply(&SessionEventMessage::new(id.as_str(), name, data));
            }
        }
        fold.apply(&SessionEventMessage::new(
            id.as_str(),
            "user_message",
            json!({"message_id": "active", "content": "continue"}),
        ));
        for name in ["text_delta", "thinking"] {
            fold.apply(&SessionEventMessage::new(
                id.as_str(),
                name,
                json!({"content": prefix}),
            ));
        }
    }
    // Keep the complete histories alive while every session receives deltas.
    // Construct input before measuring so the budget covers only fold work.
    let events: Vec<_> = sessions
        .iter()
        .map(|(id, _)| {
            (0..DELTAS)
                .map(|index| {
                    SessionEventMessage::new(
                        id.as_str(),
                        if index % 2 == 0 {
                            "text_delta"
                        } else {
                            "thinking"
                        },
                        json!({"content": "é".repeat(32)}),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    println!(
        "{SESSIONS} resident sessions, {TURNS} completed turns each; history text/thinking: {} MiB",
        SESSIONS * TURNS * (answer.len() + reasoning.len()) / 1024 / 1024
    );
    println!(
        "before streaming: RSS bytes {:?}",
        allocations::resident_bytes()
    );
    for wave in 0..WAVES {
        let start = std::time::Instant::now();
        let bytes = allocations::allocated_bytes(|| {
            for index in 0..DELTAS {
                for ((_, fold), events) in sessions.iter_mut().zip(&events) {
                    let ops = fold.apply(&events[index]);
                    assert!(matches!(ops.as_slice(), [TranscriptOp::Append { at, .. }]
                        if *at == prefix.len() + (wave * DELTAS / 2 + index / 2) * 64));
                }
            }
        });
        println!(
            "wave {wave}: {} deltas, {bytes} allocated bytes, {:?}",
            SESSIONS * DELTAS,
            start.elapsed()
        );
        println!(
            "after wave {wave}: RSS bytes {:?}",
            allocations::resident_bytes()
        );
        // The first wave may grow text, thinking and the reasoning replay
        // buffer once per session. Later waves fit those existing buffers.
        let budget = if wave == 0 {
            SESSIONS * 2 * 1024 * 1024
        } else {
            4 * 1024 * 1024
        };
        assert!(
            bytes < budget,
            "wave {wave} allocated {bytes} bytes (budget {budget})"
        );
    }
    for (_, fold) in &sessions {
        let snapshot = fold.snapshot();
        assert_eq!(snapshot.items.len(), (TURNS + 1) * 2);
        let ItemBody::AssistantSegment { text, thinking, .. } =
            &snapshot.items.last().unwrap().body
        else {
            panic!("expected the active segment");
        };
        let expected = prefix.clone() + &"é".repeat(WAVES * DELTAS / 2 * 32);
        assert_eq!(text, &expected);
        assert_eq!(thinking, &expected);
    }
}
