---
title: ACP Presentation Parity
description: Test plan closing the coverage gap between ACP-delegated agents and the internal agent
tags:
  - meta
  - plan
  - acp
  - testing
---

# ACP Presentation Parity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make a delegated ACP agent (`cru chat -a claude`) render identically to the internal
agent for equivalent behavior, and pin that with tests at both the event and frame level.

**Architecture:** `AcpAgentHandle` and `GenaiAgentHandle` both `impl AgentHandle` and both emit
`TurnEvent`, which a *single shared* consumer (`agent_manager/messaging/stream.rs`) converts to
`SessionEventMessage` → RPC → `chat_runner/commands.rs::session_event_to_chat_msgs()` →
`ChatAppMsg` → `ContainerList` → render. **From `ChatAppMsg` onward there is no ACP/internal branch
anywhere in `crates/crucible-cli/src/tui/oil/` — there is exactly one renderer.** Presentation
parity therefore reduces to a contract on the events reaching that renderer. We assert at the event
boundary (cheap, deterministic, localizing) and again at the rendered frame (proves it lands).

**Tech Stack:** Rust, cargo-nextest, `insta` snapshots, `StoryRuntime`/`Vt100TestRuntime`,
the existing ACP replay transport (`acp::client::replay::ReplayFixture`) and `mock-acp-agent`.

**Scope note — two ACP directions.** Crucible is an ACP *client* (daemon spawns
claude/opencode/codex; `crucible-daemon/src/acp/` + `acp_handle.rs`) **and** an ACP *agent*
(`cru acp`, hosted by Zed/Neovim; `crucible-cli/src/commands/acp/`). **This plan covers the client
direction only** — that is where presentation parity lives. The agent direction has its own
mapping table (`commands/acp/translate.rs:291-500`, 14 tests).

---

## Background: what is actually covered today

| Asset | Reality |
|---|---|
| `tests/acp_integration/display_parity.rs` (1101 lines) | Despite the name, stops at `StreamingChunk`. Never reaches `TurnEvent`, let alone a frame. |
| `acp_integration/{tool_roundtrip,streaming_chat,permission_flow}.rs`, `acp_fixture_replay.rs` | All assert on `StreamingChunk`/`FileDiff` values. **None render a frame.** |
| `acp_fixture_replay.rs` + `tests/fixtures/acp/recorded/*` | Only `claude/basic-chat.jsonl` is referenced. `opencode` (68 KB), `codex` (10 KB), `cursor` (1.8 KB), `gemini` (305 B) are dead files. |
| `fixture_replay_tests.rs:339 replay_acp_demo_80x24` | Asserts "no invariant violations", snapshots nothing, and **silently returns if the fixture is missing**. Worse: `acp-demo.jsonl` is misnamed — it has no `source` field and its tool names are pre-humanized, so the test exercises nothing ACP-specific. |
| All 8 `assets/fixtures/*.jsonl` | **Zero contain ACP-shaped data**: no `"source":"acp"`, no `tool_call_diff_update`, no `"diffs"`. |
| `user_story_tests/` | No ACP/delegation file. |
| `docs/Meta/TUI User Stories.md` | No ACP story. By the doc's own governance rule, the delegated-agent surface is untested by definition. |
| `assets/fixtures/parity-test.jsonl` (74 lines) | Orphaned — nothing references it. |

**Nothing anywhere asserts a rendered frame for an ACP-sourced turn.**

## Verified divergences

Grouped by where they bite. Each becomes a RED test.

### Group A — renderer receives less information on ACP

- **A1 — no `"acp"` arm in `parse_tool_source`.** `chat_app/message_handlers.rs:15-27` maps
  `"Core"|"Crucible"|"Mcp:*"|"Plugin:*"`; anything else → `None`. The daemon sends
  `source: Some("acp")` (`stream.rs:476`), so it falls through to `None` and
  `render_source_badge` (`tool_render.rs:91`) emits nothing. **ACP tool cards carry no provenance
  badge at all.** Untested.
- **A2 — ACP tool calls carry no description, primary arg, or auto-approval.** The
  `agent_owns_tools` fork (`stream.rs:292`, ACP arm `:454-480`) emits `description: None`,
  `lua_primary_arg: None`, `auto_approved: None`. Internal calls get a registry description, Lua
  display hints and the auto-approval reason (`tool_call.rs:488-557`). The `[auto]` marker
  (`tool_render.rs:101`) is structurally unreachable on ACP.
- **A3 — statusline degrades on ACP sessions.** `providers_listed` and `context_limit_resolved`
  are internal-only (`server/session/mod.rs:207-238`), so `current_provider` is never set and
  `context_total` stays 0 — the context indicator renders its "no data" path (US-205).

### Group B — event-shape divergences in `acp_handle.rs`

- **B1 — no `ToolBatchEnd`.** `acp_handle.rs:519-571` never yields it; `genai_handle.rs:1344`
  does. Acknowledged at `crucible-core/src/traits/chat.rs:102-107`. Depth-cap ticking and the Lua
  `terminate` flag are dead on every ACP session.
- **B2 — tool results are stringified.** `acp_handle.rs:559` wraps in `Value::String(...)`; the
  internal path carries structured JSON. Result cards receive different types.
- **B3 — `StopReason` is always `EndTurn`.** `acp_handle.rs:606`. Notably the client *does*
  implement cancellation (`state.cancelled` → `session/cancel`, `client/streaming.rs:225-248`),
  but that never reaches the stop reason — so a cancelled ACP turn renders as a normal completion.
  `Empty` and `MaxToolDepth` are likewise unreachable.
- **B4 — orphaned `ToolEnd` renders `"unknown_tool"`.** `acp_handle.rs:549`.

### Group C — behavioral divergences

- **C1 — thinking is dropped after the first text delta.** `chat_runner/stream.rs:42-46` was
  written for internal providers' late thinking summaries. An ACP agent that interleaves
  `AgentThoughtChunk` *after* text has those thoughts silently discarded. No test covers
  ACP-shaped interleaving.
- **C2 — late diffs can be dropped entirely.** Only ACP produces `tool_call_diff_update`
  (`acp_handle.rs:569` → `stream.rs:801` → `commands.rs:252` → `containers.rs:369`). If the tool
  already graduated to scrollback, `update_tool_by_call_id` logs a warning and drops the diff
  (`containers.rs:382`). The existing tests (`message_routing_tests.rs:177,216`) check
  `tools[0].diffs` state but never render a frame.
- **C3 — permission modal shows a coarse tool name.** The live ACP gate matches on ACP `ToolKind`
  via `acp_tool_name` (`agent_manager/messaging/permission.rs:30-52`) because **ACP carries no
  tool name on the wire** — only prose `title` and a `kind`. So the modal shows `read`/`edit`/
  `bash`/`acp_tool` where internal shows the real tool name. This is a deliberate, documented
  tradeoff, not a bug — but it is a parity difference users see, and it must be pinned so the
  schema-1.6.0 `unstable_tool_call_name` upgrade can improve it deliberately.

### Group D — dead code creating false confidence

- **D1 — `CrucibleClient` is unreachable.** Only re-exported (`acp/mod.rs:30`), never constructed
  outside its own `#[cfg(test)]` block. `AcpAgentHandle` uses the hand-rolled `acp/client/` loop
  instead. Its `request_permission` builds `PermRequest::tool(tool_call_id, <raw ACP struct>)`
  with no diffs — a serious presentation bug *if it ran*. ~13 tests pass against it. This is the
  exact failure mode the `acp_tool_name` doc comment calls out: "its unit tests passed because
  they build `PermRequest::tool` shapes that production never produces."
- **D2 — `acp/protocol.rs`'s `MessageHandler`/`ProtocolVersion` are tested but unused.**

---

## Task 1: Parity harness — normalized event capture

**Files:**
- Create: `crates/crucible-daemon/tests/acp_support/parity.rs`
- Modify: `crates/crucible-daemon/tests/acp_support/mod.rs`

**Step 1: Write the harness**

Normalize away values that legitimately differ (ids, timings) so two streams compare on *shape*,
which is what drives rendering.

```rust
//! Shared parity harness: compare an ACP agent's TurnEvent stream against the
//! internal agent's for equivalent behavior.

use crucible_core::turn::{StopReason, TurnEvent};
use futures::{Stream, StreamExt};

/// A rendering-relevant projection of a `TurnEvent`.
///
/// Tool-call ids and token counts differ run to run and between agents; they
/// are not what the user sees. Everything retained here changes the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventShape {
    Text(String),
    Thinking(String),
    /// `args_is_object` distinguishes structured args from a bare string —
    /// the difference that drives B2.
    ToolCall { name: String, args_is_object: bool, diff_count: usize },
    ToolResult { name: String, result_is_string: bool, is_error: bool },
    ToolCallDiffUpdate { diff_count: usize },
    ToolBatchEnd,
    Usage,
    Done(StopReason),
    Error,
}

pub fn shape(ev: &TurnEvent) -> Option<EventShape> {
    Some(match ev {
        TurnEvent::TextDelta(t) => EventShape::Text(t.clone()),
        TurnEvent::Thinking(t) => EventShape::Thinking(t.clone()),
        TurnEvent::ToolCall { name, args, diffs, .. } => EventShape::ToolCall {
            name: name.clone(),
            args_is_object: args.is_object(),
            diff_count: diffs.len(),
        },
        TurnEvent::ToolResult { name, result, error, .. } => EventShape::ToolResult {
            name: name.clone(),
            result_is_string: result.is_string(),
            is_error: error.is_some(),
        },
        TurnEvent::ToolCallDiffUpdate { diffs, .. } => {
            EventShape::ToolCallDiffUpdate { diff_count: diffs.len() }
        }
        TurnEvent::ToolBatchEnd => EventShape::ToolBatchEnd,
        TurnEvent::Usage(_) => EventShape::Usage,
        TurnEvent::Done { stop_reason } => EventShape::Done(stop_reason.clone()),
        TurnEvent::Error(_) => EventShape::Error,
        // Inbound-only variants never appear in an outbound stream.
        _ => return None,
    })
}

/// Drain a turn stream into its rendering-relevant shape sequence.
pub async fn shapes<S>(stream: S) -> Vec<EventShape>
where
    S: Stream<Item = TurnEvent>,
{
    futures::pin_mut!(stream);
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        if let Some(s) = shape(&ev) {
            out.push(s);
        }
    }
    out
}

/// Coalesce adjacent text deltas. Chunk boundaries are a transport artifact;
/// the rendered paragraph is not.
pub fn coalesce(shapes: Vec<EventShape>) -> Vec<EventShape> {
    let mut out: Vec<EventShape> = Vec::new();
    for s in shapes {
        match (out.last_mut(), &s) {
            (Some(EventShape::Text(prev)), EventShape::Text(next)) => prev.push_str(next),
            (Some(EventShape::Thinking(prev)), EventShape::Thinking(next)) => prev.push_str(next),
            _ => out.push(s),
        }
    }
    out
}
```

**Step 2: Register the module**

In `crates/crucible-daemon/tests/acp_support/mod.rs`, add `pub mod parity;`

**Step 3: Verify it compiles**

Run: `just test-crate crucible-daemon -E 'test(acp_)'`
Expected: compiles, existing ACP tests still pass.

**Step 4: Commit**

```bash
git add crates/crucible-daemon/tests/acp_support/
git commit -m "test(acp): add TurnEvent parity harness"
```

---

## Task 2: A1 — ACP tool cards must render a provenance badge

Highest user-visible impact and the cheapest fix. Do it first.

**Files:**
- Test: `crates/crucible-cli/src/tui/oil/chat_app/message_handlers.rs` (inline `#[cfg(test)]`)
- Test: `crates/crucible-cli/src/tui/oil/tests/user_story_tests/acp_parity_tests.rs` (new)
- Modify: `crates/crucible-cli/src/tui/oil/chat_app/message_handlers.rs:15-27`
- Modify: `crates/crucible-cli/src/tui/oil/viewport_cache.rs:10-29`

**Step 1: Write the failing unit test**

```rust
#[test]
fn acp_source_parses_to_a_displayable_source() {
    // The daemon tags delegated tool calls `source: Some("acp")`
    // (agent_manager/messaging/stream.rs:476). Falling through to None
    // means the card renders with no provenance at all.
    assert!(
        parse_tool_source(Some("acp")).is_some(),
        "`acp` fell through to None, so delegated tool cards render no badge"
    );
}
```

**Step 2: Run to confirm it fails**

Run: `just test-crate crucible-cli -E 'test(acp_source_parses)'`
Expected: FAIL — `parse_tool_source` returns `None`.

**Step 3: Add the variant and the arm**

Add `Acp { agent: Arc<str> }` to `ToolSourceDisplay` (`viewport_cache.rs:10`) with
`badge_label()` returning `Some(format!("acp:{agent}"))`, matching the existing `mcp:`/`plugin:`
grammar. Parse `"Acp:<agent>"` (and bare `"acp"`) in `parse_tool_source`.

This requires the daemon to send the agent name. In `stream.rs:476`, replace the literal
`Some("acp")` with `Some(format!("Acp:{}", agent_name))` so the badge can name *which* agent —
`[acp:claude]` is the useful badge, `[acp]` is not.

**Step 4: Add the frame-level test**

```rust
/// US-307: a delegated tool call shows which agent ran it.
#[test]
fn acp_tool_call_renders_a_provenance_badge() {
    let mut r = StoryRuntime::new(80, 24);
    r.send(acp_tool_call_msg("read_file", "Acp:claude"));
    let frame = r.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "delegated tool card showed no provenance badge:\n{frame}"
    );
}
```

**Step 5: Run both, confirm PASS**

Run: `just test-crate crucible-cli -E 'test(acp_source_parses) or test(acp_tool_call_renders)'`

**Step 6: Commit**

```bash
git commit -am "fix(tui): badge delegated tool calls with their ACP agent"
```

---

## Task 3: B1 — ACP must emit `ToolBatchEnd`

**Files:**
- Test: `crates/crucible-daemon/tests/acp_integration/turn_event_parity.rs` (new)
- Modify: `crates/crucible-daemon/src/acp_handle.rs:519-571`
- Modify: `crates/crucible-core/src/traits/chat.rs:102-107`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn acp_emits_tool_batch_end_after_tool_calls() {
    let shapes = coalesce(acp_shapes_for_scripted_tool_call().await);
    assert!(
        shapes.contains(&EventShape::ToolBatchEnd),
        "ACP turn emitted no ToolBatchEnd, so depth-cap and Lua `terminate` \
         are dead on delegated sessions; got {shapes:#?}"
    );
}
```

**Step 2: Run to confirm it fails**

Run: `just test-crate crucible-daemon -E 'test(acp_emits_tool_batch_end)'`

**Step 3: Emit the event**

In `acp_handle.rs`, after the `chunk_rx` drain loop and before consuming `result_rx`:

```rust
if !announced_ids.is_empty() {
    yield TurnEvent::ToolBatchEnd;
}
```

**Step 4: Run and check the depth-cap consumer**

Run: `just test-crate crucible-daemon -E 'test(acp_)'`
`stream.rs:823` now ticks depth for ACP — confirm nothing regresses on tool-depth counting.

**Step 5: Update the stale doc comment** at `traits/chat.rs:102-107`.

**Step 6: Commit**

```bash
git commit -am "fix(acp): emit ToolBatchEnd so depth-cap and terminate apply to delegated turns"
```

---

## Task 4: B2 — preserve tool-result structure

**Files:**
- Test: `crates/crucible-daemon/tests/acp_integration/turn_event_parity.rs`
- Modify: `crates/crucible-daemon/src/acp_handle.rs:559`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn acp_tool_result_preserves_json_structure() {
    let shapes = coalesce(acp_shapes_for_json_tool_result(r#"{"matches": 3}"#).await);
    let result_is_string = shapes.iter().find_map(|s| match s {
        EventShape::ToolResult { result_is_string, .. } => Some(*result_is_string),
        _ => None,
    });
    assert_eq!(
        result_is_string, Some(false),
        "ACP stringified a structured tool result; the internal agent keeps it \
         structured, so result cards receive different types"
    );
}
```

**Step 2: Run to confirm FAIL**

**Step 3: Parse when the payload is JSON**

Plain prose stays a string — correct, and matches the internal path for text tools.

```rust
let result_value = match result {
    Some(raw) => serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw)),
    None => serde_json::Value::String(String::new()),
};
```

**Step 4: Run to confirm PASS**

**Step 5: Commit**

```bash
git commit -am "fix(acp): keep structured tool results structured for result rendering"
```

---

## Task 5: B3/B4 — honest stop reasons and no `unknown_tool`

**Files:**
- Test: `crates/crucible-daemon/tests/acp_integration/turn_event_parity.rs`
- Modify: `crates/crucible-daemon/src/acp_handle.rs:549,606`

**Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn acp_cancelled_turn_reports_cancelled_stop_reason() {
    // The client already sends session/cancel (client/streaming.rs:225).
    // The stop reason never reflects it, so a cancelled turn renders as a
    // normal completion.
    let shapes = coalesce(acp_shapes_for_cancelled_turn().await);
    assert_eq!(shapes.last(), Some(&EventShape::Done(StopReason::Cancelled)));
}

#[tokio::test]
async fn acp_empty_turn_reports_empty_stop_reason() {
    let shapes = coalesce(acp_shapes_for_empty_turn().await);
    assert_eq!(shapes.last(), Some(&EventShape::Done(StopReason::Empty)));
}

#[tokio::test]
async fn acp_orphaned_tool_end_does_not_invent_a_tool_name() {
    let shapes = coalesce(acp_shapes_for_orphaned_tool_end().await);
    assert!(
        !shapes.iter().any(|s| matches!(
            s, EventShape::ToolResult { name, .. } if name == "unknown_tool"
        )),
        "a tool_call_update with no matching tool_call rendered a card titled `unknown_tool`"
    );
}
```

**Step 2: Run to confirm all three fail**

**Step 3: Implement**

- Thread `StreamingState.cancelled` out of the client so the handle can emit `StopReason::Cancelled`.
- Track whether any text/thinking/tool event was yielded; emit `Empty` when none were.
- For an orphaned `ToolEnd`, skip the `ToolResult` rather than fabricating a name.

**Step 4: Run to confirm PASS. Step 5: Commit**

```bash
git commit -am "fix(acp): report Cancelled and Empty stop reasons, drop orphaned tool results"
```

---

## Task 6: C1 — ACP thinking after text must survive

**Files:**
- Test: `crates/crucible-cli/src/tui/oil/tests/session_event_stream_tests.rs`
- Modify: `crates/crucible-cli/src/tui/oil/chat_runner/stream.rs:42-46`

**Step 1: Write the failing test**

```rust
#[test]
fn acp_thinking_after_text_is_not_dropped() {
    // The saw_text_delta guard exists to suppress internal providers' late
    // thinking *summaries*. ACP agents legitimately interleave thoughts with
    // text, and those are real content.
    let mut stream = SessionEventStream::new();
    stream.translate("text_delta", &json!({"content": "Let me check. "}));
    let msgs = stream.translate("thinking", &json!({"content": "checking config"}));
    assert!(
        msgs.iter().any(|m| matches!(m, ChatAppMsg::ThinkingDelta(_))),
        "interleaved ACP thinking was discarded"
    );
}
```

**Step 2: Run to confirm FAIL**

**Step 3: Scope the guard to the case it was written for**

The guard must not key on "text was seen" alone. Narrow it — suppress only the single trailing
thinking block that arrives with `message_complete`, not mid-stream thoughts. If that cannot be
distinguished from the event stream alone, gate on the agent's `owns_history` capability, which
the daemon already knows, and pass it through.

**Step 4: Run the full TUI suite** — this guard exists for a reason; confirm no thinking-duplication
regression in `rendering_regression_tests.rs`.

Run: `just test-crate crucible-cli`

**Step 5: Commit**

```bash
git commit -am "fix(tui): stop discarding interleaved thinking on delegated sessions"
```

---

## Task 7: The frame-level parity test

The payoff: same behavior, both sources, identical pixels.

**Files:**
- Create: `crates/crucible-cli/src/tui/oil/tests/user_story_tests/acp_parity_tests.rs`
- Modify: `crates/crucible-cli/src/tui/oil/tests/user_story_tests/mod.rs`
- Create: `assets/fixtures/acp_parity_internal.jsonl`, `assets/fixtures/acp_parity_delegated.jsonl`

**Step 1: Build the two fixtures**

Two `SessionEvent` JSONL fixtures describing the *same* behavior — text preamble, one `edit_file`
call carrying a diff, its result, completion. One shaped as the internal agent emits it (seed from
the currently orphaned `assets/fixtures/parity-test.jsonl`), one as the ACP path emits it —
**including `"source":"Acp:claude"` and a late `tool_call_diff_update`**, neither of which appears
in any fixture in the repo today.

**Step 2: Write the failing test**

```rust
/// US-307: a delegated ACP agent renders identically to the internal agent.
#[test]
fn acp_and_internal_agents_render_identical_frames() {
    let mut internal = StoryRuntime::new(80, 24);
    internal.pump_fixture("acp_parity_internal.jsonl");
    let internal_frame = internal.fresh_screen();

    let mut delegated = StoryRuntime::new(80, 24);
    delegated.pump_fixture("acp_parity_delegated.jsonl");
    let delegated_frame = delegated.fresh_screen();

    assert_eq!(
        internal_frame, delegated_frame,
        "the same agent behavior renders differently when delegated over ACP"
    );
}
```

**Step 3: Run it and read the diff carefully**

Run: `just test-crate crucible-cli -E 'test(acp_and_internal_agents_render)'`

Expect it to fail on the A2 gap (missing description / primary arg). Two legitimate outcomes:
- **Fixable** — make the ACP arm populate what it can (`stream.rs:454-480` can look up a
  description from the tool registry by `acp_tool_name`).
- **Deliberate** — the provenance badge from Task 2 *should* differ. Encode that as an explicit
  expected difference, not a weakened substring assertion.

**Step 4: Add a rendered late-diff test** (C2 — no existing test renders this)

```rust
#[test]
fn late_acp_diff_appears_in_the_rendered_tool_card() {
    let mut r = StoryRuntime::new(120, 40);
    r.send(acp_tool_call_msg_without_diffs("edit_file", "call-1"));
    r.send(acp_late_diff_msg("call-1"));
    let frame = r.fresh_screen();
    assert!(frame.contains("-old line"), "late ACP diff never rendered:\n{frame}");
}
```

**Step 5: Snapshot the delegated frame**

```rust
#[test]
fn acp_delegated_turn_frame() {
    let mut r = StoryRuntime::new(80, 24);
    r.pump_fixture("acp_parity_delegated.jsonl");
    insta::assert_snapshot!(r.fresh_screen());
}
```

Per project rules: **read the generated `.snap` and verify glyphs, layout and colors before
accepting it.** A passing snapshot proves stability, not correctness.

**Step 6: Commit**

```bash
git add crates/crucible-cli assets/fixtures
git commit -m "test(tui): assert ACP-delegated turns render identically to internal ones"
```

---

## Task 8: Fix the misnamed, silently-skipping replay test

**Files:**
- Modify: `crates/crucible-cli/src/tui/oil/tests/fixture_replay_tests.rs:339`
- Modify or replace: `assets/fixtures/acp-demo.jsonl`

**Step 1: Make a missing fixture a failure**

`replay_acp_demo_80x24` `eprintln!`s and returns when the fixture is absent — it reports success
while testing nothing. Replace the early return with a panic.

**Step 2: Make the fixture actually ACP-shaped**

It currently has no `source` field and pre-humanized tool names, so it exercises no ACP path.
Either re-record it from a real ACP session or point the test at
`acp_parity_delegated.jsonl` from Task 7 and delete the misleading file.

**Step 3: RED-verify**

Temporarily rename the fixture and confirm the test fails rather than passing. Restore it.

**Step 4: Commit**

```bash
git commit -am "test(tui): fail rather than skip when the ACP replay fixture is missing"
```

---

## Task 9: Assert the four dead recorded fixtures

**Files:**
- Modify: `crates/crucible-daemon/tests/acp_fixture_replay.rs`

**Step 1: Table-drive the claude test across all five agents**

`codex`, `cursor`, `gemini`, `opencode` fixtures exist but nothing loads them. Extend the replay
test to cover each, asserting each produces a well-formed shape sequence via the Task 1 harness.

**Step 2: Re-record the thin fixtures**

`gemini` is 305 bytes and `cursor` 1.8 KB — too thin to prove anything. Re-record with a prompt
exercising text + a tool call + a diff:

```sh
CRUCIBLE_ACP_RECORD_DIR=/tmp/acp cru session create -a <agent> --permissions allow
cru session send <id> "read README.md and tell me the first heading" --permissions allow
```

Sanitize `/home/moot` → `<HOME>` as the existing header documents. Re-recording needs the real
agent binaries, so gate the *recipe*, not the replay test — replay stays hermetic.

**Step 3: Commit**

```bash
git commit -am "test(acp): replay every recorded agent fixture, not just claude"
```

---

## Task 10: Pin the permission-modal difference (C3)

**Files:**
- Test: `crates/crucible-cli/src/tui/oil/tests/user_story_tests/acp_parity_tests.rs`

ACP carries no tool name on the wire, so the modal shows a `ToolKind`-derived name
(`read`/`edit`/`bash`/`acp_tool`) where internal shows the real name. That is deliberate and
documented at `agent_manager/messaging/permission.rs:30-52`. **Do not "fix" it** — pin it, so the
schema-1.6.0 `unstable_tool_call_name` upgrade is a deliberate improvement rather than an
accidental snapshot churn.

Add a frame test asserting the ACP permission modal renders the coarse name *and* its diff body
(the `d` toggle from US-401), with a comment pointing at the upgrade path.

```bash
git commit -am "test(tui): pin the ACP permission modal's coarse tool naming"
```

---

## Task 11: Resolve the dead `CrucibleClient` path (D1)

**Files:**
- Modify or delete: `crates/crucible-daemon/src/acp/acp_client.rs`
- Modify: `crates/crucible-daemon/src/acp/mod.rs:30`

`CrucibleClient` is never constructed outside its own test module. Its `request_permission` builds
`PermRequest::tool(tool_call_id, <raw ACP struct>)` with no diffs — a serious presentation bug if
it ran. ~13 tests pass against it.

Per YAGNI, delete it with its tests. If it is a deliberate seed for a future in-process ACP client,
keep it but fix `request_permission` to mirror the live path (`acp_tool_name()`, `raw_input`,
`synthesize_diffs`) so the tests stop asserting a shape production never produces. Consider
`acp/protocol.rs`'s unused `MessageHandler`/`ProtocolVersion` (D2) in the same pass.

**Decide with the maintainer before acting** — this is a deletion, not a refactor.

```bash
git commit -am "refactor(acp): remove the unreachable CrucibleClient permission path"
```

---

## Task 12: Document the story and the contract

**Files:**
- Modify: `docs/Meta/TUI User Stories.md` (new section 3 entry)
- Modify: `docs/Meta/Analysis/Systems.md`

**Step 1: Add the user story**

```markdown
### US-307: Delegated agent presentation parity
**As a user**, an ACP-delegated agent (`cru chat -a claude`) renders like the internal agent —
same tool cards, thinking blocks, diffs and stop states — with delegation surfaced only where it
is deliberate (an `[acp:<agent>]` provenance badge).
**Acceptance:** equivalent behavior produces equivalent `TurnEvent` shape sequences from
`AcpAgentHandle` and `GenaiAgentHandle`; frames rendered from both differ only in the provenance
badge; delegated tool calls carry a source badge; late `tool_call_diff_update` diffs render into
the existing card; interleaved thinking is not discarded; structured tool results stay structured;
cancelled and empty turns report honest stop reasons. Known deliberate difference: the permission
modal shows a `ToolKind`-derived name because ACP carries no tool name on the wire.
**Tests:** T1 shape-sequence parity in `acp_integration/turn_event_parity.rs` + `parse_tool_source`
unit tests; T2 identical-frame assertion, badge, late-diff render and snapshot in
`user_story_tests/acp_parity_tests.rs`; T3 recorded-fixture replay for all five agents in
`acp_fixture_replay.rs`.
```

**Step 2: Record the contract in `Systems.md`**

State that `TurnEvent` is the parity boundary: everything downstream is shared, so a new
`AgentHandle` gets correct presentation for free *if and only if* it emits the same event shapes
and populates the same `tool_call` metadata. Note that `display_parity.rs` tests the
`StreamingChunk` layer, which sits *above* this boundary and cannot prove parity alone.

**Step 3: Commit**

```bash
git add docs/
git commit -m "docs: add US-307 delegated agent presentation parity and the TurnEvent contract"
```

---

## Final verification

Run: `just ci`
Expected: fmt, clippy, size gate, nextest, web unit + e2e all pass.

Then re-read every changed `.snap` per the project's snapshot rule before considering this done.

## See Also
- [[Meta/TUI User Stories]] — US-307 and the tier definitions
- [[Meta/Analysis/Systems]] — system boundaries
- [[Help/Concepts/Agent Client Protocol]] — ACP reference
- [[Help/Concepts/Delegation]] — delegation model
