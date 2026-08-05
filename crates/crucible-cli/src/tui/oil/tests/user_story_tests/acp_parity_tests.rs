//! US-307: delegated (ACP) turns must present like internal ones.
//!
//! A tool call executed inside an external agent's own loop still shows up
//! as a card in our transcript. Without a provenance badge the card is
//! indistinguishable from a tool Crucible ran itself, which hides the fact
//! that another process — under another permission gate — touched the
//! workspace. See `docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`.
//!
//! These cover the *consumer* half of the badge contract. The producer half —
//! the daemon stamping `Acp:<agent>` onto the `tool_call` event — is pinned in
//! `crucible-daemon/src/agent_manager/tests/messaging.rs`, because nothing in
//! this crate can fail when that literal regresses.
//!
//! The two thinking tests at the end belong to **US-203** (thinking display),
//! not US-307: they are about which thoughts reach the screen, which has
//! nothing to do with provenance. They live here because a delegated agent is
//! the shape that exposes the behaviour — it runs its own tool loop, so it
//! alternates reasoning and narration inside one turn.

use serde_json::json;

use super::support::StoryRuntime;
use super::vocab::{
    announce_tool_call, attach_late_diff, complete_tool_call, hydrate_from_recording,
    relay_session_event, relay_session_turn, send_user_message,
};

const READ_ARGS: &str = r#"{"path":"README.md"}"#;

#[test]
fn acp_tool_call_renders_a_provenance_badge() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    announce_tool_call(&mut story, "read_file", READ_ARGS, Some("Acp:claude"));

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "delegated tool card showed no provenance badge:\n{frame}"
    );
}

#[test]
fn acp_tool_call_badge_survives_the_daemon_event_mapping() {
    // The daemon's `tool_call` event carries the agent under `source`; the
    // RPC → `ChatAppMsg` mapping must keep it all the way to the badge.
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    relay_session_event(
        &mut story,
        "tool_call",
        serde_json::json!({
            "tool": "read_file",
            "args": {"path": "README.md"},
            "call_id": "c1",
            "source": "Acp:claude",
        }),
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "the daemon's `source` field did not reach the rendered badge:\n{frame}"
    );
}

#[test]
fn a_pre_badge_recording_still_badges_the_delegated_card() {
    // Sessions recorded before the daemon named the agent carry a bare
    // lowercase `acp`. Replaying one must still say "another agent ran this",
    // even though it can no longer say which one.
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    announce_tool_call(&mut story, "read_file", READ_ARGS, Some("acp"));

    let frame = story.fresh_screen();
    assert!(
        frame.contains("[acp]"),
        "a pre-badge ACP recording replayed with no provenance badge:\n{frame}"
    );
}

/// US-203: a delegated agent runs its own tool loop, so it narrates and reasons
/// in alternation across a single turn. Every one of those thoughts is content
/// the user asked to see (`show_thinking` defaults on), not a restatement.
#[test]
fn a_delegated_agent_second_thought_reaches_the_screen() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "why is the build slow?");
    relay_session_turn(
        &mut story,
        &[
            ("thinking", json!({"content": "profile the build first"})),
            ("text_delta", json!({"content": "Checking the build. "})),
            ("thinking", json!({"content": "link time dominates"})),
            (
                "text_delta",
                json!({"content": "Linking is the bottleneck."}),
            ),
            (
                "message_complete",
                json!({"message_id": "m1", "full_response": "Checking the build. Linking is the bottleneck."}),
            ),
        ],
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("link time dominates"),
        "the delegated agent's second thought never rendered:\n{frame}"
    );
    assert!(
        frame.contains("profile the build first") && frame.contains("Linking is the bottleneck."),
        "letting the second thought through lost earlier turn content:\n{frame}"
    );
}

/// US-203, the counterweight: a provider that streams its reasoning and *then*
/// replays the whole block at stream end must not paint it twice. This is what
/// the original `saw_text_delta` guard bought, and it has to survive the
/// narrowing.
#[test]
fn an_end_of_stream_reasoning_replay_is_not_painted_twice() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "hello");
    relay_session_turn(
        &mut story,
        &[
            ("thinking", json!({"content": "weigh "})),
            ("thinking", json!({"content": "the tradeoffs"})),
            ("text_delta", json!({"content": "Here goes."})),
            ("thinking", json!({"content": "weigh the tradeoffs"})),
            (
                "message_complete",
                json!({"message_id": "m1", "full_response": "Here goes."}),
            ),
        ],
    );

    let frame = story.fresh_screen();
    assert_eq!(
        frame.matches("the tradeoffs").count(),
        1,
        "the end-of-stream reasoning replay rendered a second copy:\n{frame}"
    );
}

/// The `[acp:<agent>]` badge is the *only* sanctioned frame difference between
/// a delegated turn and an internal one. Removing it lets the rest of the two
/// frames be compared byte-for-byte; a weakened `contains` assertion would let
/// any other divergence through.
///
/// **Exactly one occurrence, deliberately.** A global `replace` would happily
/// scrub a second badge a future fixture grew, and with it any divergence that
/// happened to sit on that card — the normalization would start hiding what it
/// exists to expose. Each fixture here has one delegated tool call; if that
/// ever changes, this assertion is the place to decide what "modulo the badge"
/// should mean rather than silently widening it.
fn without_the_provenance_badge(frame: &str) -> String {
    const BADGE: &str = " [acp:claude]";
    assert_eq!(
        frame.matches(BADGE).count(),
        1,
        "expected exactly one `{BADGE}` to normalize away, so that stripping it \
         cannot mask an unrelated difference:\n{frame}"
    );
    frame.replacen(BADGE, "", 1)
}

/// US-307: the same agent behaviour renders the same whoever performed it.
///
/// The two fixtures are the *post-convergence* layer — `SessionEvent` JSONL,
/// after `AcpAgentHandle` and `GenaiAgentHandle` have both funnelled into
/// `SessionEventMessage`. Pumping them through one `StoryRuntime` each
/// exercises the single shared renderer, so any surviving frame difference is
/// a real presentation divergence and not the by-design `owns_history`
/// asymmetry at the `TurnEvent` layer.
///
/// Both fixtures are the daemon's own broadcast output, replayed on demand by
/// `crucible-daemon`'s `agent_manager::tests::parity_capture` so they cannot
/// outlive the shape they pin; see `scripts/gen-acp-parity-fixtures.py`. They
/// include the two fields the plan's divergence A2 named:
///
/// - `description` — the internal fixture carries the registry text, the
///   delegated one has none. The TUI does render descriptions when it has one:
///   `render_description` paints a dimmed indented line and `CachedToolCall`
///   keeps the field. What breaks the chain is a single hard-coded
///   `let description = None` in `session_event_to_chat_msgs`
///   (`chat_runner/commands.rs`: "not shown during live streaming … omit on
///   resume for consistency"), and that converter is the only producer of
///   `ChatAppMsg::ToolCall`. So the asymmetry costs no pixels *today*; wire
///   the daemon's description through for one arm only and this test fails,
///   which is the point of leaving it in the fixtures.
/// - `lua_primary_arg` / `auto_approved` — absent from both, because neither
///   is a property of the *behaviour*. A registry tool with no Lua display
///   plugin emits no hint, and an interactively approved call earns no
///   `[auto]` marker. Baking either into the internal side alone would assert
///   a difference this pair does not describe.
#[test]
fn acp_and_internal_agents_render_identical_frames() {
    let mut internal = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut internal, "acp_parity_internal.jsonl");
    let internal_frame = internal.fresh_screen();

    let mut delegated = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut delegated, "acp_parity_delegated.jsonl");
    let delegated_frame = delegated.fresh_screen();

    assert_ne!(
        internal_frame, delegated_frame,
        "the delegated frame is byte-identical to the internal one, so the \
         provenance badge is missing — see `acp_tool_call_renders_a_provenance_badge`"
    );
    assert_eq!(
        internal_frame,
        without_the_provenance_badge(&delegated_frame),
        "the same agent behaviour renders differently when delegated over ACP, \
         beyond the provenance badge"
    );
}

/// The counterweight to the normalization above: proving the frames match once
/// the badge is stripped is only meaningful if the badge was there to strip.
#[test]
fn the_delegated_frame_still_names_the_agent() {
    let mut delegated = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut delegated, "acp_parity_delegated.jsonl");

    let frame = delegated.fresh_screen();
    assert!(
        frame.contains("[acp:claude]"),
        "the delegated tool card lost its provenance badge:\n{frame}"
    );
}

/// US-307 (C2): a diff that arrives *after* the tool card must land on the card.
///
/// Only ACP produces this shape — Claude Code sends the initial `tool_call`
/// notification with empty content and attaches the diff in a follow-up
/// `tool_call_update`. `message_routing_tests` pins the resulting
/// `tools[0].diffs` state, but state is not a frame: nothing until now proved
/// the late diff is actually painted.
///
/// The frame is rendered *before* the update as well as after, so this also
/// pins the boundary of the drop path at `containers.rs`'s
/// `update_tool_by_call_id`: a `ToolGroup` never graduates while the turn is
/// active, so an intervening render cannot strand the diff. Reaching that
/// warning needs a render taken after the turn ended and a diff after that —
/// an ordering the ACP client cannot produce, since every `tool_call_update`
/// precedes the prompt response that ends the turn.
#[test]
fn a_late_acp_diff_appears_in_the_rendered_tool_card() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "fix the greeting");
    announce_tool_call(
        &mut story,
        "Edit File",
        r#"{"path":"greeting.rs"}"#,
        Some("Acp:claude"),
    );

    let before = story.fresh_screen();
    assert!(
        !before.contains("println!"),
        "the tool card already showed a diff before the update arrived, so \
         this test cannot tell whether the late diff landed:\n{before}"
    );

    attach_late_diff(
        &mut story,
        "Edit File-1",
        "greeting.rs",
        "    println!(\"hello\");\n",
        "    println!(\"hello, world\");\n",
    );
    complete_tool_call(
        &mut story,
        "Edit File",
        "Edit File-1",
        "Replaced 1 occurrence(s)",
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("-    println!(\"hello\");"),
        "the late ACP diff's removed line never rendered:\n{frame}"
    );
    assert!(
        frame.contains("+    println!(\"hello, world\");"),
        "the late ACP diff's added line never rendered:\n{frame}"
    );
}

/// US-307 (A4): the same tool, the same output, summarized the same way.
///
/// The edit pair above converges partly by luck: `Replaced 1 occurrence(s)` is
/// 23 characters on one line, which `collapse_result`'s generic short-result
/// branch renders identically whatever the tool is called. A result that does
/// *not* fit on one line goes through `summarize_tool_result`'s per-tool table
/// instead — and that table matched the internal snake_case name (`read_file`)
/// and nothing else, so the delegated card, whose name is
/// `humanize_tool_title(title)`, fell through to painting the file body into
/// the card while the internal one showed `→ [3 lines read, 3 total]`.
#[test]
fn acp_and_internal_read_turns_render_identical_frames() {
    let mut internal = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut internal, "acp_parity_read_internal.jsonl");
    let internal_frame = internal.fresh_screen();

    let mut delegated = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut delegated, "acp_parity_read_delegated.jsonl");
    let delegated_frame = delegated.fresh_screen();

    assert_ne!(
        internal_frame, delegated_frame,
        "the delegated frame is byte-identical to the internal one, so the \
         provenance badge is missing — see `acp_tool_call_renders_a_provenance_badge`"
    );
    assert_eq!(
        internal_frame,
        without_the_provenance_badge(&delegated_frame),
        "a delegated read renders differently from the internal read of the \
         same file, beyond the provenance badge"
    );
}

/// The counterweight: proving the two read frames match is only worth
/// something if both actually collapsed the result. A future change that made
/// *neither* card summarize would keep the equality test green while losing
/// the summary on both sides.
#[test]
fn both_read_cards_collapse_their_result_to_a_summary() {
    for fixture in [
        "acp_parity_read_internal.jsonl",
        "acp_parity_read_delegated.jsonl",
    ] {
        let mut story = StoryRuntime::new(80, 24);
        hydrate_from_recording(&mut story, fixture);
        let frame = story.fresh_screen();
        assert!(
            frame.contains("\u{2192} [3 lines read, 3 total]"),
            "{fixture} did not collapse the read result into the card header:\n{frame}"
        );
        assert!(
            !frame.contains("println!"),
            "{fixture} painted the file body into the transcript instead of \
             summarizing it:\n{frame}"
        );
    }
}

/// The rendered shape of a whole delegated turn, pinned.
///
/// The equality test above proves the delegated frame matches the internal one
/// modulo the badge; it cannot prove either is *right*. This one is the
/// eyeballed reference — read the `.snap` when it changes.
#[test]
fn acp_delegated_turn_frame() {
    let mut story = StoryRuntime::new(80, 24);
    hydrate_from_recording(&mut story, "acp_parity_delegated.jsonl");
    insta::assert_snapshot!(story.fresh_screen());
}
