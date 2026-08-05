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

/// Collect the thinking text a translated event stream would render.
fn thinking_of(msgs: &[ChatAppMsg]) -> Vec<String> {
    msgs.iter()
        .filter_map(|m| match m {
            ChatAppMsg::ThinkingDelta(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn interleaved_thinking_after_text_is_not_dropped() {
    // An agent that runs its own tool loop (any `owns_history` ACP agent, and
    // the internal agent between tool batches) legitimately goes
    // think → speak → think again inside one turn. Those later thoughts are
    // new content, not a replay of what already rendered.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "why is the build slow?"})),
        ("thinking", json!({"content": "check the profile first"})),
        ("text_delta", json!({"content": "Let me check. "})),
        ("thinking", json!({"content": "link time dominates"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["check the profile first", "link time dominates"],
        "a thought that arrived after the first text delta was discarded"
    );
}

#[test]
fn end_of_stream_thinking_replay_is_dropped() {
    // Providers that stream reasoning incrementally *also* hand back the whole
    // block at stream end. Rendering that replay duplicates the entire thought.
    // The daemon dedupes this at the source since `ReasoningEmissionState`
    // (genai_handle.rs), but every session recorded before that still carries
    // the replay, and replay is the path this converter serves.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "first I "})),
        ("thinking", json!({"content": "consider the options"})),
        ("text_delta", json!({"content": "Here goes."})),
        (
            "thinking",
            json!({"content": "first I consider the options"}),
        ),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["first I ", "consider the options"],
        "the end-of-stream reasoning replay was rendered a second time"
    );
}

#[test]
fn thinking_replay_is_dropped_even_without_an_intervening_text_delta() {
    // The same replay lands mid-turn with no text between it and the run it
    // repeats — `reproduce.jsonl` carries two of each flavour. The old
    // `saw_text_delta` guard only caught the flavour that followed text.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "weigh "})),
        ("thinking", json!({"content": "the tradeoffs"})),
        ("thinking", json!({"content": "weigh the tradeoffs"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["weigh ", "the tradeoffs"],
        "a mid-turn reasoning replay rendered the block twice"
    );
}

#[test]
fn a_repeated_thought_in_a_later_turn_still_renders() {
    // Replay suppression is turn-scoped: the same reasoning text in the next
    // turn is a genuinely new thought and must survive.
    //
    // Turn "a" streams the thought as two deltas on purpose. That makes its run
    // replay-eligible *and* byte-equal to turn "b"'s single thought, so the only
    // thing standing between turn "b" and deletion is the per-turn reset — which
    // is what this test claims to pin. Repeating the thought inside turn "a"
    // instead would trip the replay rule there and clear the run as a side
    // effect, leaving nothing for the reset to do.
    let mut stream = SessionEventStream::new();
    for (t, d) in [
        ("user_message", json!({"content": "a"})),
        ("thinking", json!({"content": "same "})),
        ("thinking", json!({"content": "thought"})),
    ] {
        let _ = stream.translate(t, &d);
    }

    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "b"})),
        ("thinking", json!({"content": "same thought"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(thinking_of(&msgs), vec!["same thought"]);
}

#[test]
fn a_thought_that_merely_repeats_the_one_before_it_still_renders() {
    // A replay is the *concatenation* of a streamed run, so it takes at least
    // two deltas to make one. Matching a one-delta run as well deleted real
    // content: an agent that said "Hmm." three times rendered only twice,
    // alternating drop and render as the run reset itself.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "Hmm."})),
        ("thinking", json!({"content": "Hmm."})),
        ("thinking", json!({"content": "Hmm."})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["Hmm.", "Hmm.", "Hmm."],
        "a repeated short thought was deleted as if it were a stream replay"
    );
}

#[test]
fn a_single_delta_thought_is_never_a_replay_boundary() {
    // The same hole at its smallest: one thought, then the identical thought.
    // Nothing has been *streamed* yet, so there is no run for the second to be
    // a replay of — and the window was open at exactly the moment a turn
    // starts, where the old `saw_text_delta` guard never dropped anything.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "check the lockfile"})),
        ("thinking", json!({"content": "check the lockfile"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["check the lockfile", "check the lockfile"],
        "a thought was deleted for repeating a single-delta predecessor"
    );
}

#[test]
fn a_replay_that_is_not_byte_exact_renders_twice_on_purpose() {
    // A replay whose bytes differ from the run — here by one trailing newline —
    // is rendered, painting the reasoning block a second time. That is the
    // intended fall of the rule, not an oversight, and this test exists to keep
    // it a decision rather than an accident.
    //
    // Byte-equality is the only predicate here with a *reason* behind it: a
    // replay is by construction the concatenation of the run that preceded it,
    // so equality is a statement about how the payload was produced. Every
    // looser predicate — equal modulo trailing whitespace, prefix-of, "close
    // enough" — is a similarity heuristic, and each one admits a fresh class of
    // genuine reasoning into a code path that deletes without a trace.
    //
    // The two failure modes are not symmetric, which is what settles it. A
    // replay that renders is a visible duplicate: the user reads it, recognises
    // it, scrolls past. A real thought that is dropped is gone, and the user has
    // no way to know it existed. `MIN_REPLAY_RUN_DELTAS` was added for exactly
    // that reason, and loosening the comparison reopens the same hazard from
    // the other side.
    //
    // Nor is the loosening earning anything today: all 11 replays across the
    // four recordings in `assets/fixtures` are byte-identical to their runs, so
    // no recording in this repo — including the pre-2026-04-27 ones, which are
    // the whole reason this converter still dedupes at all — needs it. A
    // provider that normalizes its `captured_reasoning_content` would show a
    // duplicated block, and the fix then belongs at the source, next to
    // `ReasoningEmissionState`, where the exact normalization is known.
    //
    // The non-match also leaves the run in place and appends to it, so the cost
    // of guessing wrong stays bounded at "rendered twice" — the following
    // assertion pins that no thought after it is deleted either.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "read "})),
        ("thinking", json!({"content": "the config"})),
        ("thinking", json!({"content": "read the config\n"})),
        ("thinking", json!({"content": "now run it"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(
        thinking_of(&msgs),
        vec!["read ", "the config", "read the config\n", "now run it"],
        "the byte-exactness rule changed direction: a replay that differs from \
         its run is now suppressed, which trades a visible duplicate for a \
         silent deletion"
    );
}

#[test]
fn a_replay_is_still_dropped_at_the_minimum_run_length() {
    // The floor is two deltas, not more: a run of exactly two must still have
    // its replay suppressed, or the fix would have traded one silent deletion
    // for a duplicated thought.
    let mut stream = SessionEventStream::new();
    let mut msgs = Vec::new();
    for (t, d) in [
        ("user_message", json!({"content": "hi"})),
        ("thinking", json!({"content": "read "})),
        ("thinking", json!({"content": "the config"})),
        ("thinking", json!({"content": "read the config"})),
    ] {
        msgs.extend(stream.translate(t, &d));
    }

    assert_eq!(thinking_of(&msgs), vec!["read ", "the config"]);
}

#[test]
fn recorded_fixtures_render_exactly_their_non_replayed_thoughts() {
    // Pins the rule against every recording in the repo that carries an
    // end-of-stream reasoning replay, as a **count** of rendered
    // `ThinkingDelta`s rather than a self-checking oracle.
    //
    // The oracle version of this test was blind: it re-derived the "run so far"
    // itself but never reset that accumulator when the converter dropped a
    // payload, so after the first replay its model diverged from production and
    // it could no longer see a second one. Deleting the run-consumption inside
    // the converter's match arm left it green while regressing `demo.jsonl`,
    // whose second 119-char replay would then render.
    //
    // A constant has no model to get wrong. Each number is the count of
    // `thinking` events in that fixture minus its replays, verified
    // independently against the raw JSONL:
    //
    //   demo                  68 thinking events −  2 replays (156, 119 chars)
    //   parity-test           59                 −  1        (266)
    //   reproduce            173                 −  4        (515, 79, 161, 87)
    //   reproduce-formatting 162                 −  4        (272, 146, 183, 144)
    //
    // Mutation-verified: it dies both when a replay is never dropped (counts
    // rise) and when the run is not consumed on a match (later replays in the
    // same turn stop matching, counts rise). It does **not** catch a missing
    // `user_message` reset — no recording here happens to open a turn with a
    // thought equal to the previous turn's whole run — so that mutation is
    // owned by `a_repeated_thought_in_a_later_turn_still_renders`, which
    // constructs the collision on purpose.
    for (fixture, expected) in [
        ("demo.jsonl", 66),
        ("parity-test.jsonl", 58),
        ("reproduce.jsonl", 169),
        ("reproduce-formatting.jsonl", 158),
    ] {
        let jsonl = crate::tui::oil::tests::helpers::read_fixture(fixture);
        let mut stream = SessionEventStream::new();
        let mut rendered = 0usize;

        for line in jsonl.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(event_type) = v.get("event").and_then(|x| x.as_str()) else {
                continue;
            };
            let data = v.get("data").cloned().unwrap_or_default();
            rendered += thinking_of(&stream.translate(event_type, &data)).len();
        }

        assert_eq!(
            rendered, expected,
            "{fixture} rendered {rendered} thinking deltas, expected {expected}"
        );
    }
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
    let jsonl = crate::tui::oil::tests::helpers::read_fixture("delegation-demo.jsonl");
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
