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

use super::super::helpers::recorded_claude_code_title;
use super::support::StoryRuntime;
use super::vocab::{
    announce_tool_call, attach_late_diff, complete_tool_call, hydrate_from_recording,
    open_tool_permission, relay_session_event, relay_session_turn, send_user_message,
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

/// US-307 (A4, second pass): the titles a *real* agent sends must collapse too.
///
/// The read pair above proves the tables answer to `Read File`. That is a
/// spelling the mock behind `acp_parity_read_delegated.jsonl` was written to
/// emit — and it is also, as it happens, one Claude Code really sends, which is
/// how the first fix passed while covering almost nothing. The recording says
/// the same agent titles its glob `Find` and its bash `Terminal`, neither of
/// which the table listed, so a delegated glob painted its whole file list into
/// the transcript.
///
/// The titles come from [`recorded_claude_code_title`], which fails if
/// `malformed-acp-recording.jsonl` stops containing them — the pair cannot
/// drift into testing a spelling nothing produces.
///
/// The card *header* legitimately differs (`Glob` vs `Find`): ACP carries no
/// tool name on the wire, only prose, so the two cards are entitled to
/// different names for the same tool. What must not differ is the body — one
/// summary line, not the file list.
#[test]
fn a_delegated_glob_collapses_its_file_list_like_the_internal_one() {
    // A newline-separated list, which is what both Crucible's `glob` and
    // Claude Code's Find answer with.
    const FILES: &str = "alpha.rs\nbeta.rs\ngamma.rs";

    for (tool, source) in [
        ("glob", "Core"),
        (recorded_claude_code_title("Find"), "Acp:claude"),
    ] {
        let mut story = StoryRuntime::new(80, 24);
        send_user_message(&mut story, "find the rust files");
        announce_tool_call(&mut story, tool, r#"{"pattern":"**/*.rs"}"#, Some(source));
        complete_tool_call(&mut story, tool, &format!("{tool}-1"), FILES);

        let frame = story.fresh_screen();
        assert!(
            frame.contains("\u{2192} 3 files"),
            "`{tool}` did not collapse its file list into the card header:\n{frame}"
        );
        assert!(
            !frame.contains("beta.rs"),
            "`{tool}` painted the file list into the transcript instead of \
             summarizing it:\n{frame}"
        );
    }
}

/// The counter-case, pinned so it is not "fixed" by reflex.
///
/// The same recording titles Claude Code's bash `Terminal`, which no arm of the
/// summary table lists either — but unlike the glob, that costs nothing. The
/// `Bash` arm only answers for a result of one line under 60 characters, and
/// that is precisely when `collapse_result`'s *name-independent* short-result
/// branch answers with the same string. So a delegated shell command already
/// collapses exactly like the internal one, and adding `Terminal` to the arm
/// would change no pixel while looking like coverage.
///
/// This fails if that short-result branch ever becomes name-dependent, which is
/// what would make the synonym necessary.
#[test]
fn a_delegated_shell_command_needs_no_synonym_to_match_the_internal_one() {
    for (tool, source) in [
        ("bash", "Core"),
        (recorded_claude_code_title("Terminal"), "Acp:claude"),
    ] {
        let mut story = StoryRuntime::new(80, 24);
        send_user_message(&mut story, "check the version");
        announce_tool_call(
            &mut story,
            tool,
            r#"{"command":"cru --version"}"#,
            Some(source),
        );
        complete_tool_call(&mut story, tool, &format!("{tool}-1"), "cru 0.21.0");

        let frame = story.fresh_screen();
        assert!(
            frame.contains("\u{2192} cru 0.21.0"),
            "`{tool}` did not collapse its output into the card header:\n{frame}"
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

// ---------------------------------------------------------------------------
// Divergence C3: the permission modal's tool name on the ACP path.
//
// ACP carries no tool name on the wire. `ToolCallUpdateFields` has `title`
// (prose for a human — "Read src/main.rs") and `kind` (a category), so the
// daemon's gate derives a name from `kind`: `read`/`edit`/`bash`/`acp_tool`
// (`acp_tool_name`, `agent_manager/messaging/permission.rs`). Keying on
// `title` was tried and abandoned — it made `is_safe` never true and left the
// whole gate unreachable. The modal therefore shows a coarse category where an
// internally-run tool shows its real name.
//
// These tests pin that, deliberately. **The upgrade path is schema 1.6.0's
// `unstable_tool_call_name`**, which would replace the derivation outright at
// the cost of moving `agent-client-protocol` 0.10 → 2.0 and opting into a
// field its own docs call removable. When that lands, these tests are the
// thing that should change — read `acp_tool_name`'s doc comment, then update
// the expected name here. Failing after that upgrade is the intended outcome,
// not a regression.
// ---------------------------------------------------------------------------

// There is deliberately no on-disk fixture here any more. `synthesize_diffs`
// renders from the tool arguments alone, so these legs need no file to diff
// against — and an edit naming a path that does not exist previews exactly
// like one that does.

/// The edit an agent asks permission for. Identical on both legs: the *only*
/// difference under test is the tool name the daemon had available.
fn edit_args() -> serde_json::Value {
    json!({
        "path": "main.rs",
        "old_string": "    old();",
        "new_string": "    fresh();",
    })
}

#[test]
fn an_acp_permission_modal_names_the_tool_kind_not_the_tool() {
    let mut story = StoryRuntime::new(80, 30);
    send_user_message(&mut story, "fix main.rs");
    // `ToolKind::Edit` is all the wire carried, so this is the name the gate
    // matched rules against and the name the user is shown.
    let _ = open_tool_permission(&mut story, "req-1", "edit", edit_args());

    let frame = story.fresh_screen();
    // Pinned, not endorsed: see the C3 comment above. `unstable_tool_call_name`
    // (schema 1.6.0) is what would make this read `edit_file` — changing this
    // literal is the deliberate signal that the upgrade happened.
    assert!(
        frame.contains(r#"edit (path="main.rs""#),
        "the ACP permission modal stopped showing the kind-derived name:\n{frame}"
    );
    assert!(
        !frame.contains("edit_file"),
        "the ACP modal named a real tool — the wire cannot know one, so \
         something is inventing it:\n{frame}"
    );
}

#[test]
fn an_acp_permission_modal_still_shows_its_diff() {
    let mut story = StoryRuntime::new(80, 30);
    send_user_message(&mut story, "fix main.rs");
    let _ = open_tool_permission(&mut story, "req-1", "edit", edit_args());

    let frame = story.fresh_screen();
    // The coarse name is what `synthesize_diffs` normalizes, and `edit` is one
    // of the names it knows — so the diff survives the naming divergence and
    // US-401's expand/collapse toggle has a body to act on. The toggle is `h`
    // (`interaction_modal/perm.rs`), which is what the hint line below reads.
    assert!(
        frame.contains("-    old();") && frame.contains("+    fresh();"),
        "the ACP permission modal rendered no diff body — a delegated edit is \
         approved blind:\n{frame}"
    );
    assert!(
        frame.contains("press h to expand/collapse diff"),
        "the diff toggle hint is gone, so the body cannot be collapsed:\n{frame}"
    );
}

#[test]
fn an_internal_permission_modal_names_the_real_tool() {
    // The contrast leg: same edit, same args, same synthesized diff — the
    // daemon's own tool loop has the registry name, so it shows it.
    let mut story = StoryRuntime::new(80, 30);
    send_user_message(&mut story, "fix main.rs");
    let _ = open_tool_permission(&mut story, "req-1", "edit_file", edit_args());

    let frame = story.fresh_screen();
    assert!(
        frame.contains(r#"edit_file (path="main.rs""#),
        "the internal permission modal lost the real tool name:\n{frame}"
    );
    assert!(
        frame.contains("-    old();") && frame.contains("+    fresh();"),
        "the internal permission modal rendered no diff body:\n{frame}"
    );
}

#[test]
fn an_acp_shell_permission_modal_shows_the_command_not_the_derived_name() {
    // The divergence's boundary: for `ToolKind::Execute` the modal renders the
    // command line and drops the tool name entirely (`ToolDisplayKind::Command`
    // in `render_perm_interaction`), so the coarse `bash` never reaches the
    // screen and this path costs nothing. Pinned so a change that started
    // printing the derived name — `bash (command="…")` — shows up as a
    // *behaviour* change rather than as cosmetics.
    let mut story = StoryRuntime::new(80, 30);
    send_user_message(&mut story, "clean up");
    let _ = open_tool_permission(
        &mut story,
        "req-1",
        "bash",
        json!({"command": "rm -rf build"}),
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("rm -rf build"),
        "the shell permission modal did not show the command:\n{frame}"
    );
    assert!(
        !frame.contains("bash (command"),
        "the derived tool name leaked into a shell prompt that reads as a \
         command line:\n{frame}"
    );
}
