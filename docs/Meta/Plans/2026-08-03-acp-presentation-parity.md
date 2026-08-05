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
`TurnEvent`, consumed by the shared `agent_manager/messaging/stream.rs`, which emits
`SessionEventMessage` → RPC → `chat_runner/commands.rs::session_event_to_chat_msgs()` →
`ChatAppMsg` → `ContainerList` → render. **From `ChatAppMsg` onward there is no ACP/internal branch
anywhere in `crates/crucible-cli/src/tui/oil/` — there is exactly one renderer.**

**The parity boundary is `SessionEventMessage`, not `TurnEvent`.** This distinction was wrong in
the first draft of this plan and matters:

- At the `TurnEvent` layer the two agents differ **by design**. The internal agent yields
  `ToolCall` + `ToolBatchEnd` and lets the daemon dispatch the tool, receiving the result back
  *inbound* (`stream.rs:715`). An ACP agent (`owns_history`) runs its own tool loop and yields
  `ToolCall` + `ToolResult` *outbound*. `genai_handle.rs:1363` only ever matches `ToolResult` as
  inbound; it never yields one. A direct `assert_eq!(turn_events(acp), turn_events(internal))` is
  therefore structurally impossible and must never be written.
- Both paths **converge** on `SessionEventMessage::tool_result(session_id, id, name, {"result"|"error": …})`
  — internal from `tool_call.rs:739`, ACP from `stream.rs:786`. That is the shared vocabulary the
  renderer consumes, and the only honest place to assert cross-agent equality.

So: `TurnEvent`-level tests are **per-agent expectations** (does ACP emit the events its own
contract requires), and cross-agent parity is asserted at `SessionEventMessage` and at the
rendered frame.

**Tech Stack:** Rust, cargo-nextest, `insta` snapshots, `StoryRuntime`/`Vt100TestRuntime`,
the existing ACP replay transport (`acp::client::replay::ReplayFixture`) and `mock-acp-agent`.

**Prerequisites.** Run `just web-build` once in a fresh worktree — the nextest setup script fails
otherwise with `#[derive(RustEmbed)] folder crucible-web/web/dist does not exist`, which blocks
*every* daemon test. Note also that `just test-crate <crate>` takes only a crate name; to filter,
use `just test-crate-filter <crate> <filter>`. nextest's `test()` matcher runs against the test's
path *within* the binary, not the binary name — so a filter like `acp_` will not select
`support::parity::tests::*`.

**Verification status.** `just ci` **passes** on this branch (fmt, clippy, size gate, nextest, web
unit + e2e). An earlier note here claimed it could not, because two `clippy::cloned_ref_to_slice_refs`
warnings exist at `daemon_plugins/tests.rs` and `skills/discovery.rs`. That was wrong — they are
warnings under `just clippy`'s flags, not errors, and do not fail the gate. They are still worth
cleaning up, but they never blocked this branch.

**Two environment traps that cost this branch real time**, both self-inflicted and worth knowing:
a subagent used the scratchpad as a `CARGO_TARGET_DIR`, putting 33 GB of Rust artifacts into a
RAM-backed tmpfs — which surfaced as `ld terminated with signal 7 [Bus error]` on *doctests only*,
in files the branch never touched. And a CPU-saturation experiment left 128 orphaned busy-loop
shells running for two days at load average 144. Between them they produced the "load-dependent
e2e flakes with fixed 5s deadlines" this plan previously recorded as a property of the repo. It is
not one. If daemon e2e tests flake, check `df -h /tmp` and the load average before believing the
test is at fault.

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
| All 8 `assets/fixtures/*.jsonl` | **Zero contain ACP-shaped data**: no `"source":"acp"`, no `tool_call_diff_update`, no `"diffs"`. *(Closed by Task 7: `acp_parity_delegated.jsonl` carries `"source":"Acp:claude"` and a `tool_call_diff_update` whose payload is a `diffs` array; `acp_parity_internal.jsonl` carries `diffs` on the `tool_call` itself.)* |
| `user_story_tests/` | No ACP/delegation file. *(Closed: `acp_parity_tests.rs`.)* |
| `docs/Meta/TUI User Stories.md` | No ACP story. By the doc's own governance rule, the delegated-agent surface is untested by definition. *(Closed: US-307.)* |
| `assets/fixtures/parity-test.jsonl` (74 lines) | Orphaned — nothing references it. *(Closed by Task 6, which adopted it into the thinking-delta count table — so Task 7 did **not** seed from it.)* |

**Nothing anywhere asserts a rendered frame for an ACP-sourced turn.** *(Closed by Task 7.)*

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
- **A3 — statusline degrades on ACP sessions. RESOLVED — see Task 15 (`f70542b19`).**
  **Correction (found while implementing Task 9):** the stated cause was only half the story, and
  the other half made it fixable. `context_limit_resolved` needs an `endpoint` and a non-empty
  `model`, which a delegating session has neither of — so it can never fire. But ACP agents put
  the context window on the wire in a `usage_update` frame and Crucible discarded it. Worse than
  "unhandled": `SessionUpdate::UsageUpdate` is behind the `unstable_session_usage` cargo feature,
  which the daemon does not enable, and `SessionUpdate` is internally tagged on `sessionUpdate` —
  so the frame failed to *deserialize at all* and died at the "Failed to parse SessionNotification"
  warning, never reaching any match arm. **Any** unstable update type an agent sends is dropped
  wholesale by the same mechanism. Task 15 extracts the fields from the raw JSON (the house
  pattern already used by `acp/client/usage.rs`) rather than enabling the feature, and emits the
  existing `context_limit_resolved` with a new `ContextLimitSource::Agent` — so the statusline
  lights up on delegated sessions with **zero** TUI changes. Original text follows for the record:
  `providers_listed` and `context_limit_resolved`
  are internal-only (`server/session/mod.rs:207-238`), so `current_provider` is never set and
  `context_total` stays 0 — the context indicator renders its "no data" path (US-205).
- **A4 — delegated tool cards lose their result summary. FIXED.** Found while reviewing Task 7.
  `summarize_tool_result` and `collapse_result` (`components/tool_render.rs`) matched `self.name`
  **literally** — `"read_file" | "mcp_read"`, `"glob"`, `"grep"`, `"edit"`, `"write"`,
  `"bash"` — every one of them an internal spelling. ACP carries no tool name on the wire, so a
  delegated card's `name` is `humanize_tool_title(title)` (`"Read File"`, `"Read"`), which
  equals none of them. Consequence: an internal `read_file` card collapsed to
  `→ [3 lines read, 3 total]` while the delegated card for the *same file* fell through both
  tables — `collapse_result`'s generic branch needs one line of ≤60 chars, which a file read is
  not — and painted the file body into the transcript. Same for `glob`/`grep`/`bash`.

  **Task 7's fixture pair could not see this.** `Replaced 1 occurrence(s)` is 23 characters on
  one line, so both arms hit the generic short-result branch and agreed for a reason that has
  nothing to do with the tool's name. That is luck, not design: it proved parity for one benign
  tool shape.

  **Fix:** both tables now key on `summary_key(name)` = `humanize_tool_title(name)` — the same
  function `CachedToolCall::display_name` already uses for the card header, so the summary and
  the name it sits next to can no longer disagree. It is idempotent on a clean title, which is
  what lets one arm serve `read_file`, `mcp_read`, `mcp__crucible__read_file` and a prose
  `Read`. Arm membership is preserved exactly (`read_file` → `Read File`, `mcp_read` → `Read`,
  `edit`/`mcp_edit` → `Edit`, …): `edit_file` and `write_file` stay *outside* the `Edit`/`Write`
  arms as before, which costs no parity because both answer with one short line that
  `collapse_result` returns verbatim before reaching the table.

  **Tests:** a second fixture pair over `read_file`
  (`assets/fixtures/acp_parity_read_{internal,delegated}.jsonl`, generated by the same script)
  plus `acp_and_internal_read_turns_render_identical_frames` and
  `both_read_cards_collapse_their_result_to_a_summary` in `acp_parity_tests.rs`; per-spelling
  unit tests on both tables in `tool_render.rs`, including a negative case pinning that the
  normalization does not silently widen the table.

  **One existing snapshot was recording the bug.** `styled_snapshot_tool_call` drives a tool
  named `Read File` — the ACP spelling — and its `.snap` showed the file body painted into the
  card. It now shows `→ 3 lines`. The body-line styling it used to be the only cover for moved
  to a new `styled_snapshot_tool_call_with_body`, which uses `bash` (deliberately unsummarized
  for a multi-line result) so that coverage is not lost to the fix.

### Group B — event-shape divergences in `acp_handle.rs`

- **B1 — no `ToolBatchEnd`.** `acp_handle.rs:519-571` never yields it; `genai_handle.rs:1344`
  does. Acknowledged at `crucible-core/src/traits/chat.rs:102-107`.
  **Correction (found while implementing Task 3):** the original claim that this is *why* depth-cap
  and the Lua `terminate` flag are dead on ACP sessions is **wrong**. Both are dead for an
  independent reason — the `agent_owns_tools` arm `continue`s at `stream.rs:506`, *before*
  `in_tool_batch = true` (`:535`) and `tool_depth += 1` (`:555`), so `tool_depth` never advances;
  and `batch_terminate_signals` is only pushed at `:699` inside the dispatch-result block, which an
  ACP agent never reaches because it executes tools in its own process. Emitting `ToolBatchEnd`
  therefore changes **no** daemon control flow — its two assignments are no-ops on values already
  false. It is still worth emitting for contract consistency, and Task 3 pins that it does not end
  the turn. Making depth-cap meaningful for delegated agents is a separate design question: the
  runtime does not dispatch, and `DepthCapHit` is sent to an inbound channel the ACP agent has
  dropped.
- **~~B2 — tool results are stringified.~~ WITHDRAWN — this was a false premise.** `acp_handle.rs:559`
  does wrap in `Value::String(...)`, but so does the internal path (`stream.rs:717`), and both
  converge on the same `{"result": <string>}` envelope at the `SessionEventMessage` boundary
  (`tool_call.rs:739` vs `stream.rs:786`). There is no divergence here. Task 4 is dropped.
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
- **C4 — ACP thinking never reached the screen at all.** Found while reviewing Task 6, and the
  largest parity gap in this plan. `SessionUpdate::AgentThoughtChunk` was **never matched** in
  `acp/client/streaming.rs`: both `apply_session_update_with_callback` and `apply_session_update`
  handled `AgentMessageChunk` and let thought chunks fall into the terminal
  `other => tracing::debug!("Ignoring session update: …")` arm. Consequence:
  `StreamingChunk::Thinking` (`acp/streaming.rs:27`) had **no production producer** — its only
  non-test reference was its consumer at `acp_handle.rs:550` — so the entire
  `StreamingChunk::Thinking` → `TurnEvent::Thinking` → `thinking` SessionEvent →
  `ThinkingComponent` chain was dead on delegated sessions. Claude Code, Gemini and every other
  conforming ACP agent stream reasoning this way, so a delegated session showed the user **no
  thinking blocks whatsoever** while the internal agent showed them. The only `AgentThoughtChunk`
  references anywhere in the repo were in `crucible-cli/src/commands/acp/translate.rs`, i.e. the
  *outbound* `cru acp` direction. Fixed in Task 13.
- **C3 — permission modal shows a coarse tool name.** The live ACP gate matches on ACP `ToolKind`
  via `acp_tool_name` (`agent_manager/messaging/permission.rs:30-52`) because **ACP carries no
  tool name on the wire** — only prose `title` and a `kind`. So the modal shows `read`/`edit`/
  `bash`/`acp_tool` where internal shows the real tool name. This is a deliberate, documented
  tradeoff, not a bug — but it is a parity difference users see, and it must be pinned so the
  schema-1.6.0 `unstable_tool_call_name` upgrade can improve it deliberately.
  **Corrections (found while implementing Task 10, by rendering real frames):**
  1. The earlier claim that ACP "attaches no diffs" is **wrong**. Both sites build the identical
     `PermRequest::tool(name, args).with_diffs(synthesize_diffs(..))` — ACP at
     `permission.rs:212-225`, internal at `:934-940` — and `diff_synth::normalize_tool_name`
     already accepts the coarse spellings (`"edit_file" | "edit" | "Edit" | …`), so the derived
     name lands on the same synthesizer arm and the diff survives. Args are byte-identical
     (ACP's come straight from `raw_input`). **The `unstable_tool_call_name` upgrade must keep
     that true.**
  2. For `ToolKind::Execute` the divergence is **invisible** — `render_perm_interaction`'s
     `ToolDisplayKind::Command` arm renders the command line and drops the tool name entirely.
     The naming difference only costs anything for non-command kinds.
  3. **Open gap:** `ToolKind::Other`/absent derives `acp_tool`, which `normalize_tool_name` does
     not recognise, so an unkinded ACP edit renders **no diff** and is approved without one.
     Correct in that an unidentified call must not inherit file-op treatment, but it means diff
     visibility depends on the agent supplying a `kind`. Worth its own decision.

### Group D — dead code creating false confidence

- **D1 — `CrucibleClient` is unreachable.** Only re-exported (`acp/mod.rs:30`), never constructed
  outside its own `#[cfg(test)]` block. `AcpAgentHandle` uses the hand-rolled `acp/client/` loop
  instead. Its `request_permission` builds `PermRequest::tool(tool_call_id, <raw ACP struct>)`
  with no diffs — a serious presentation bug *if it ran*. ~13 tests pass against it. This is the
  exact failure mode the `acp_tool_name` doc comment calls out: "its unit tests passed because
  they build `PermRequest::tool` shapes that production never produces."
  *(Resolved by Task 11: `acp_client.rs` deleted, 740 lines / 15 tests.)*
- **D2 — `acp/protocol.rs`'s `MessageHandler`/`ProtocolVersion` are tested but unused.**
  *(Confirmed and resolved by Task 11: the whole file was dead — `ACP_VERSION` included — and was
  deleted, 129 lines / 3 tests, in its own commit.)*

---

## Task 1: Parity harness — normalized event capture — DONE (`b0c9cfe6b`, `46b11823d`)

> Shipped with review fixes: ids normalize to first-seen ordinals (`call#0`) rather than being
> erased, the inbound-only arms are enumerated so a new outbound variant breaks the build, and
> `diff_paths` replaced a bare count. `args_is_object`/`result_is_string` were removed with B2.

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

Run: `just test-crate-filter crucible-daemon 'acp_'`
Expected: compiles, existing ACP tests still pass.

**Step 4: Commit**

```bash
git add crates/crucible-daemon/tests/acp_support/
git commit -m "test(acp): add TurnEvent parity harness"
```

---

## Task 2: A1 — ACP tool cards must render a provenance badge — DONE (`e6bec7f54`, `140e672f5`)

> Shipped with review fixes: `ToolSource::Acp { agent }` added in `crucible-core` and routed
> through the canonical `format_tool_source`, so the `Acp:` grammar has one producer; a
> daemon-side test pins the wire contract (reverting the producer previously left every CLI test
> green); lowercase `acp` from pre-badge recordings also parses.

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

Run: `just test-crate-filter crucible-cli 'acp_source_parses'`
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

Run: `just test-crate-filter crucible-cli 'acp_'`

**Step 6: Commit**

```bash
git commit -am "fix(tui): badge delegated tool calls with their ACP agent"
```

---

## Task 3: B1 — ACP must emit `ToolBatchEnd` — DONE (`7926e5035`)

**Outcome differed from the plan; read this before citing Task 3.** The original body claimed
emitting `ToolBatchEnd` would revive depth-capping and the Lua `terminate` flag for delegated
sessions. That is false, and was disproven while implementing (see the correction on B1 above).
Emitting the event is **control-flow neutral** — reviewer independently traced every write site of
all three variables the `stream.rs:839` handler touches (`in_tool_batch`, `capped_this_batch`,
`batch_terminate_signals`) and confirmed each is unreachable on an `owns_history` turn.

It was implemented anyway, on contract-consistency grounds: `AcpAgentHandle` should honour the same
`TurnEvent` contract as `GenaiAgentHandle`, so a future consumer of the batch boundary is not
silently wrong on delegated turns.

**What shipped:**
- `acp_handle.rs` yields `ToolBatchEnd` **after** the post-stream replay loop, not before
  `result_rx` as originally sketched. Emitting earlier would announce a batch close ahead of
  `ToolCall`s the same turn is about to yield from the replay path.
- Guard is "≥1 `ToolCall` was yielded this turn"; error/timeout arms deliberately do not emit,
  since those `return StreamOutcome::Failed` immediately and all batch state is loop-local.
- `traits/chat.rs:102-107` rewritten to name the real reason depth-cap and `terminate` are dead
  (the `agent_owns_tools` fork), rather than blaming the missing event.
- Non-regression test `owns_history_tool_batch_end_does_not_end_the_turn`
  (`agent_manager/tests/messaging.rs`) — mutation-verified: dropping the `!is_empty()` guard at
  `stream.rs:851` makes it fail, because an empty `all()` returns true and would end every
  delegated turn at its first tool call.
- Test scaffolding: `AcpAgentHandle` has no transport seam (it always spawns a process), so the
  test drives a spawned `mock-acp-agent` via the `acp_smoke.rs` idiom rather than the in-process
  duplex transport. Added a child-scoped `CRU_MOCK_STREAM_TOOL_CALL` env hook.

**Follow-on design question, deliberately not answered here:** making depth-cap meaningful for
delegated agents. The runtime does not dispatch their tools, and `DepthCapHit` is sent to an
inbound channel the ACP agent has dropped — so a cap would need to be expressed on the ACP wire,
not as another `TurnEvent`.

---

## Task 4: WITHDRAWN — B2 was a false premise

**Do not implement.** The original claim was that ACP stringifies tool results while the internal
agent keeps them structured. It does not hold:

- ACP: `acp_handle.rs:559` wraps in `Value::String`.
- Internal: `stream.rs:717` *also* wraps in `Value::String` when feeding the result back inbound.
- Both converge on the same `{"result": <string>}` / `{"error": <string>}` envelope at the
  `SessionEventMessage` boundary — `tool_call.rs:739` (internal) and `stream.rs:786` (ACP).

The only real difference in that envelope is the internal path's optional `summary` field from Lua
display hints (`tool_call.rs:729-734`), which is already captured as **A2**.

Renumbering is deliberately avoided so review comments and commits keep referring to stable task
numbers. Skip straight to Task 5.

## Task 5: B3/B4 — honest stop reasons and no `unknown_tool` — DONE

**Two of the three prescribed implementation steps were wrong; read this before citing Task 5.**

- **"Thread `StreamingState.cancelled` out of the client" is dead plumbing, and was not done.**
  That flag is set when a streaming callback returns `false`, and this handle's callback is
  `channel_callback` (`acp/streaming.rs:71`), which returns `false` only when `chunk_rx` has been
  dropped — i.e. when the `stream!` body that would yield `Done` no longer exists. A stop reason
  derived from it can never be read by anyone. The observable signal is the one already sitting in
  the handle unused: ACP mandates that an agent which saw `session/cancel` answer with
  `stopReason: cancelled`, and `acp_handle.rs` was discarding the whole `PromptResponse` as
  `_response`. The fix reads it (`turn_stop_reason`, unit-tested for all five ACP variants —
  the enum is `#[non_exhaustive]`, so the mapping needs a wildcard arm regardless).
- **"Skip the orphaned `ToolResult`" is right only for a minority of the cases.** A `ToolEnd`
  whose id has no *live* name covers four situations, and only one of them is a genuine orphan:
  1. **Late title** — the `tool_call_update` carries a `title`, so the client records it and the
     handle's post-stream replay announces the call, **after** the result has gone out.
  2. **Repeat completion** — ACP updates are partial-field merges, so an agent may send
     `completed` twice for the same call, the later frame usually carrying the fuller
     `rawOutput`. Both renderers key a result on the **call id** (`containers.rs::update_tool`,
     the web's `updateToolMessage`), so the second result belongs on the same card.
  3. **Out of order** — a bare completed update arrives before the `tool_call` that names it.
  4. **Truly unnameable** — nothing in the turn ever names that id.

  Only (4) is dropped. The name table is therefore **read, never consumed** (an early
  `remove()` turned (2) into a false orphan and silently swallowed the real answer), and the
  deferred drain falls back from the replay's names to the live table, which is what recovers
  (3). The deferred list is capped at 256 — each entry pins an un-truncated payload for the rest
  of the turn.

  **Dropping (4) is a real loss, not a no-op.** Beyond the missing card, the row never reaches
  `{kiln}/.crucible/sessions/{id}/session.jsonl` (`should_persist` persists `tool_result`) or
  `recording.jsonl` (which records every broadcast event unfiltered), never fires a Lua
  `tool_result` handler or redactor (`agent_manager/messaging/stream.rs`), and never becomes a
  `ResponsePart::ToolResult` for `cru.sessions.send_and_collect` (`session_bridge.rs`), which
  some plugins render standalone. It is still the right trade — the row names a `call_id` no
  other row in the turn mentions, so no renderer has anywhere to put it — but the drop logs the
  **payload** at `warn!`, not just the id, because the daemon log is the only place it survives.
- **Replayed names are humanized like live ones.** The live `ToolStart` carries
  `humanize_tool_title(&tool_call.title)` (`acp/client/streaming.rs`) while `record_tool_call`
  stores the raw title, so a replayed call used to read `mcp__crucible__semantic_search` where
  the same tool announced live read `Semantic Search`. The replay path is *always* taken for a
  late-titled orphan, so the divergence was hot rather than latent. `humanize_tool_title` is
  idempotent on a clean title, so the fix is safe at the replay site.
- The `ToolBatchEnd` hole Task 3 flagged is closed as a consequence: every `ToolResult` a turn
  yields now belongs to a `ToolCall` the same turn yielded, so `announced_any` can no longer
  disagree with what was reported. The identical gate in the non-callback `apply_session_update`
  needed no change — that path has no callback and so emits no `ToolEnd` chunks at all.
- The added code pushed `acp_handle.rs` past the 1000-line budget
  (`architecture_tests::no_new_oversized_modules`, and the ledger only shrinks), so the free
  functions that translate the ACP wire into `TurnEvent`s — `turn_stop_reason`,
  `replay_unannounced_tool_calls`, `acp_prompt_text` — moved with their unit tests into
  `acp_handle/translate.rs`. That is the file's natural seam: everything in it is pure, so the
  wire-shape decisions are testable without spawning an agent.
- **`StopReason::Empty` was fixed on the *internal* side, not faked on the ACP side.** The first
  draft of this section claimed the internal agent emits `Empty` for a contentless turn. It did
  not: `genai_handle.rs` emitted `Empty` at exactly one site — an *unexpected stream close* — and
  a well-formed turn that produced nothing fell through to `EndTurn`. Reporting `Empty` on ACP
  alone would have created a **new** divergence inside the parity task. `Empty` is the more
  honest value (the enum's own doc), so `genai_handle` now tracks whether the turn yielded
  non-blank text, thinking or a tool call across the whole `'turn` loop and reports `Empty` when
  it did not. Both agents agree, and `StopReason::Empty`'s doc was widened from "no text and no
  tool calls" to "nothing the user can see", which is what both implementations mean.
- **Neither new stop-reason value has a reader.** `terminal_stop_reason`
  (`agent_manager/messaging/stream.rs`) is only ever tested for `is_none()`, nothing matches on
  the value, and the daemon-proxy path re-fabricates `EndTurn`
  (`rpc_client/agent/convert.rs:206,239`). Earlier drafts of this section named consumers ("retry,
  validation, statusline") that do not exist. This lands for contract consistency with
  `GenaiAgentHandle` — the same honest framing Task 3 used for `ToolBatchEnd`.
- **`MaxTokens` / `MaxTurnRequests` / `Refusal` still collapse to `EndTurn`**, which repeats B3's
  own failure mode: a truncated answer reported as a natural completion. No variant is added
  because parity holds — the internal agent has no upstream reason to carry either. They are
  listed explicitly before the wildcard and `debug!`-logged at the collapse site so the loss is
  visible and a future `#[non_exhaustive]` ACP variant is not swallowed into this set. When such
  a turn produced *nothing*, `Empty` still wins — a contentless refusal is a blank screen, not a
  completed answer.
- Mock hooks added (child-scoped env, value-read not presence-read): `CRU_MOCK_STOP_REASON`
  (`cancelled` picks the `PromptTurn::cancelled` ending the mock already built) and
  `CRU_MOCK_ORPHAN_TOOL_END` (`bare` | `titled` | `repeat` | `out_of_order`, the four flavors
  above). The empty-turn case needed no hook — an unconfigured mock turn already produces
  nothing.

**Files:**
- Test: `crates/crucible-daemon/tests/acp_integration/turn_event_parity.rs`
- Modify: `crates/crucible-daemon/src/acp_handle.rs` (the `ToolEnd` arm and the post-stream drain),
  `crates/crucible-daemon/src/acp_handle/translate.rs`,
  `crates/crucible-daemon/src/provider/genai_handle.rs` (the contentless-turn `Done`),
  `crates/crucible-core/src/turn/mod.rs` (`StopReason::Empty` doc)

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

## Task 6: C1 — ACP thinking after text must survive — DONE (`1795a937e`, reviewed)

**The implementation took a third route, not either option this task prescribed.** The plan offered
"suppress only the trailing block that arrives with `message_complete`" or "gate on `owns_history`".
Neither was used:

- The trailing-block rule is unimplementable from the event stream. The replay does **not** arrive
  attached to `message_complete`; it is an ordinary `thinking` event that lands mid-turn, and
  `reproduce.jsonl` carries two flavours of it — one after a `text_delta` and one with no text in
  between at all. Position cannot separate it from a real thought.
- Gating on `owns_history` would have been a per-agent branch in the one place the whole plan says
  there is exactly one renderer, and it would still have been wrong for the internal agent, which
  also interleaves thinking and text between tool batches.

What shipped instead is **content-based, turn-scoped replay detection**: keep the concatenation of
the thinking rendered since the last boundary, and drop a `thinking` payload that equals it exactly,
consuming the run on a match so several reasoning blocks in one turn each get their own replay
suppressed. Interleaving is then free — a new thought never equals the run before it.

**The live path no longer needs any of this, and that is the honest framing.** The daemon fixed the
duplication at its source: `ReasoningEmissionState` (`provider/genai_handle.rs:202`, commit
`acab76636`) tracks whether reasoning chunks were emitted live and suppresses the `End`-time replay
when they were. So on a current session this heuristic has nothing to catch. What it serves is
**replay of recordings made before that fix** — every `session.jsonl` and `recording.jsonl` in the
wild predating 2026-04-27, including the four fixtures in `assets/fixtures` — which is precisely
what `SessionEventStream` exists for.

**Accepted downside risk, stated explicitly:** the rule also runs live, where it can only cost. If a
model ever streams a reasoning block and then genuinely thinks the identical thing again in the same
turn, we delete the second one. That is judged acceptable because the rule is content-exact,
turn-scoped, and floored at a two-delta run (below) — but it is a deletion, not a suppression, and
it is unrecoverable. The alternative, running the heuristic only in replay mode, would mean the
replay path and the live path no longer render a recording identically, which is a worse property
to lose than this risk is to carry.

**Review findings, fixed in a follow-up commit:**
- **I1 — the rule had a silent-deletion window.** Matching *any* non-empty run meant a thought that
  merely repeated its predecessor was deleted: `["Hmm.", "Hmm.", "Hmm."]` rendered only twice,
  alternating drop and render as the run reset itself, and a single-delta thought followed by an
  identical one vanished. The window was open exactly when the run was short — at turn start and
  right after every drop — i.e. where the old `saw_text_delta` guard had never dropped anything, so
  the narrowing was strictly *less* safe there. Fixed with `MIN_REPLAY_RUN_DELTAS = 2`: a replay is
  by construction the concatenation of a streamed run, so it takes at least two deltas to make one.
  Costs nothing on real data — all 11 replays across the four fixtures follow runs of **17–103**
  deltas and carry 79–515 chars.
- **I6 — the per-turn reset test was vacuous.** `a_repeated_thought_in_a_later_turn_still_renders`
  fired the same thought twice inside turn "a", which tripped the replay rule *there* and cleared
  the run as a side effect, so the reset it claimed to pin was never exercised — deleting
  `thinking_run.clear()` from the `user_message` arm left every test green. Turn "a" now streams the
  thought as two deltas instead, making its run both replay-eligible and byte-equal to turn "b"'s
  thought. Mutation-verified: the test now dies when that reset is removed.
- **I7 — run-consumption was untested and the fixture oracle was blind.** The oracle re-derived the
  run itself but never reset its accumulator when the converter dropped a payload, so after the
  first replay its model diverged from production and it could not see a second. Deleting the
  in-match `thinking_run.clear()` left all tests green while regressing `demo.jsonl`. Replaced with
  a **count** of rendered `ThinkingDelta`s per fixture (demo 66, parity-test 58, reproduce 169,
  reproduce-formatting 158) — a constant has no model to get wrong. Mutation-verified against
  never-drop and no-run-consume. It does **not** catch a missing turn reset (no recording opens a
  turn with a thought equal to the previous turn's whole run), which is why I6's test is separate.
- **I5 — a third private copy of the old guard.**
  `user_story_tests/support.rs::load_fixture` mapped fixtures through raw
  `session_event_to_chat_msgs`, with no replay dedup and no `message_complete` suppression. Harmless
  while only thinking-free fixtures were pumped, but Task 7's parity fixtures go through exactly
  that helper. Converted to the production `SessionEventStream`, matching the other two sites.
- **M2/M3/M4 —** two stale comments in `inter_frame_invariant_tests.rs` (neither test guards replay
  suppression; `check_no_duplicate_thought_lines` only fires on adjacent identical collapsed
  headers), cross-references between `relay_session_event` and `relay_session_turn` in `vocab.rs`
  including the fresh-stream-per-call caveat, and a cwd-relative fixture path replaced by a single
  shared `helpers::fixture_path`/`read_fixture` (three private copies collapsed into one).

**Story attribution:** the two new frame tests are **US-203** (thinking display), not US-307. They
sit in `acp_parity_tests.rs` because a delegated agent is the shape that exposes the behaviour, but
US-203 is where the acceptance criteria and test references now live.

**Files:** `crates/crucible-cli/src/tui/oil/chat_runner/stream.rs`,
`tests/session_event_stream_tests.rs`, `tests/helpers.rs`, `tests/fixture_replay_tests.rs`,
`tests/inter_frame_invariant_tests.rs`, `tests/user_story_tests/{support,vocab,acp_parity_tests}.rs`,
`docs/Meta/TUI User Stories.md` (US-203).

---

## Task 7: The frame-level parity test — DONE

The payoff: same behavior, both sources, identical pixels.

**Why this one is legitimate** (unlike a `TurnEvent`-level equality assertion): the fixtures are
`SessionEvent` JSONL, i.e. the layer *after* the two agents converge. Pumping both through
`StoryRuntime` exercises the single shared renderer, so any frame difference is a genuine
presentation divergence rather than the by-design `owns_history` asymmetry.

**What shipped:**

- `assets/fixtures/acp_parity_internal.jsonl` and `acp_parity_delegated.jsonl`: one `edit_file`
  turn (text preamble → tool card with a diff → result → coda) told twice.
- `scripts/gen_acp_parity_fixtures.py` regenerates both. **Neither was written from the docs.**
  Both event streams were captured from the daemon's live broadcast channel via a throwaway
  `ReactorTestHarness` test — `StreamingMockAgent` + the real `WorkspaceTools` dispatcher and the
  real permission gate for the internal arm, `OwnsToolsMockAgent` with `agent_name: "claude"` for
  the delegated arm — and the emitted JSON was copied field for field, including the
  `tool_call_with_metadata`-computed `display` object and the `terminate: false` on every
  `tool_result`. The capture test was deleted; the script's comments name each field's origin.
- Four new tests in `user_story_tests/acp_parity_tests.rs` (file now 9 tests):
  `acp_and_internal_agents_render_identical_frames`, `the_delegated_frame_still_names_the_agent`,
  `a_late_acp_diff_appears_in_the_rendered_tool_card`, `acp_delegated_turn_frame` (insta).
- Two new `vocab.rs` verbs: `attach_late_diff` (ACP `tool_call_diff_update`) and
  `complete_tool_call` (the delta + complete pair a `tool_result` maps to).
- US-307 in `docs/Meta/TUI User Stories.md` widened from "tool provenance" to "presentation
  parity" and given the new legs.

**The specification of "parity": every difference, classified.**

| # | Difference | Classification |
|---|---|---|
| 1 | `source`: `Core` vs `Acp:claude` → ` [acp:claude]` on the card | **Deliberate.** This is the badge Task 2 added; erasing it is the failure mode, not the fix. Encoded explicitly: the test asserts `assert_ne!` on the raw frames *and* `assert_eq!` after removing exactly the string `" [acp:claude]"`. Both halves are load-bearing — mutation-verified by flipping the delegated fixture's source to `Core` (kills the `assert_ne!`). |
| 2 | `description`: registry text on internal, absent on ACP (divergence **A2**) | **Not frame-observable, and the plan's expectation here was wrong.** The task predicted the equality test would fail on this. It does not, because `session_event_to_chat_msgs` hard-codes `let description = None` for *every* path ("not shown during live streaming … omit on resume for consistency"), and it is the **only** producer of `ChatAppMsg::ToolCall` — the live TUI shares the converter with replay. So A2's description half is a `crucible-web` concern, not a TUI one. **No daemon fix was made:** the "fixable" option (look up a description in the ACP arm) would add a field the TUI still discards, i.e. speculative work behind an unobservable seam. The asymmetry is deliberately *left in the fixtures* so that wiring descriptions through for one arm only breaks this test. |
| 3 | diff delivery: on the `tool_call` (internal, `diff_synth`) vs a later `tool_call_diff_update` (ACP) | **Fixed by design, now pinned.** Both land on the same card and render the same body. Mutation-verified by deleting the `tool_call_diff_update` line from the delegated fixture. |
| 4 | `tool`: `edit_file` vs `Edit File` | **Deliberate and invisible.** ACP carries no tool name on the wire, only a prose `title`; the client stores `humanize_tool_title(title)`. The renderer's `display_name()` applies the same (idempotent) humanizer, so both render `Edit File`. `ToolDisplay::of` also picks `greeting.rs` for both — verified in the captures, not assumed. |
| 5 | `tool_result` payload: `{"result":"{\"result\":\"Replaced 1 occurrence(s)\"}"}` vs `{"result":"Replaced 1 occurrence(s)"}` | **Deliberate and invisible.** The internal envelope really is doubly wrapped (the tool returns JSON, the event wraps it again); ACP's `extract_tool_result` stringifies `rawOutput` flat. `unwrap_json_result` normalizes both to the same text. Kept in the fixtures rather than smoothed over, because smoothing it would stop testing the normalizer. |
| 6 | `lua_primary_arg` / `auto_approved` (rest of **A2**) | **Out of the pair, deliberately.** Neither is a property of the *behaviour*: a registry tool with no Lua display plugin emits no hint, and an interactively approved `edit_file` earns no `[auto]` marker. Putting either on the internal side alone would assert a difference the pair does not describe. The `[auto]` marker genuinely is unreachable on ACP — correctly so, since Crucible granted nothing; the delegated agent ran its own gate in its own process. |
| 7 | `interaction_requested` present only on the internal side | **Deliberate and inert.** It is in the internal fixture because a real recording of a gated edit contains it. The converter has no arm for it, so it produces no `ChatAppMsg` and no pixels. |
| 8 | Statusline: `— ctx` "no data" (divergence **A3**) | **Not covered by this pair — and A3 itself is since RESOLVED by Task 15 (`f70542b19`), which this pair predates.** `providers_listed` / `context_limit_resolved` are absent from *both* fixtures, so both render the no-data path and the test is silent either way. The fix has its own leg: `acp_integration/context_usage.rs`. Regenerating these fixtures against a delegated session that now emits `context_limit_resolved` would change both arms and is deliberately not done here. |

**Reachability of the graduated-diff drop path (`containers.rs::update_tool_by_call_id`'s warning).**
Not reachable from any ordering the ACP client can produce. Graduation only runs inside
`drain_completed` at render time, and `is_graduatable` refuses to graduate a `ToolGroup` while
`turn_active`. So stranding a late diff needs *all* of: turn ended → a render → diff arrives. Every
`tool_call_update` precedes the prompt response that ends the turn, so the third step cannot follow
the first. `a_late_acp_diff_appears_in_the_rendered_tool_card` renders a frame between the card and
the diff to pin exactly that: an intervening render does not strand it. (The warning is still worth
keeping — a resumed session replaying events into an idle app could hit it.)

**Snapshot verification** (`acp_delegated_turn_frame.snap`, read line by line before accepting):
80-column rows for all chrome rows; `▄` U+2584 / `▀` U+2580 half-block frames around the user
message and the input prompt, identical to `undo_flow_frame_sequence.snap`; ` ● ` U+25CF assistant
bullet on the first segment and a 3-column indent on the post-tool continuation (` Done.`), i.e.
one response split by the tool group, not two responses; tool header
` ✓ Edit File [acp:claude] greeting.rs → Replaced 1 occurrence(s)` with U+2713 and U+2192 exactly
where `render_complete` composes them; diff header `edit greeting.rs  +1 -1` (action from
`diff_action`, counts matching the one-line change) and a unified body whose context lines carry a
leading space and whose changed lines carry `-`/`+`; `— ctx` U+2014 in the status row. No duplicate
assistant text — `message_complete`'s `full_response` snapshot was suppressed by
`SessionEventStream`, which is Task 6's machinery doing its job on a fixture written after it. The
snapshot is plain `screen_contents()`, so it carries no ANSI attributes to verify.

**Two things in the task description did not match reality:**
- `assets/fixtures/parity-test.jsonl` is **no longer orphaned** — Task 6 adopted it into
  `session_event_stream_tests.rs`'s thinking-delta count table. The new fixtures were built from a
  fresh capture instead, which is stronger grounding anyway.
- The predicted A2 failure did not occur, for the reason in row 2 above.

**The original pair proved parity for exactly one benign tool shape.** Reviewing this task turned
up divergence **A4**: the tool-card summary tables key on the tool *name*, and every name they
listed was an internal spelling, so no delegated card could ever reach them. The `edit_file` pair
converged anyway because `Replaced 1 occurrence(s)` is 23 characters on one line and
`collapse_result`'s generic short-result branch renders that identically whatever the tool is
called — the two arms agreed for a reason unrelated to the thing under test. A second pair over
`read_file` (`acp_parity_read_{internal,delegated}.jsonl`), whose result is multi-line and
therefore *does* reach the tables, failed immediately. Read A4 in Group A for the fix. The lesson
for future pairs: pick a behaviour whose output exercises the code path you mean to pin, and add a
counterweight assertion (`both_read_cards_collapse_their_result_to_a_summary`) so "the two frames
match" cannot be satisfied by both being wrong.

**Review findings, fixed in a follow-up commit:**

- **I1 — the fixtures' provenance was unverifiable.** The capture harness was written, run and
  *deleted*, so nothing watched the producers. `tool_call_with_metadata`, the `agent_owns_tools`
  pass-through arm and `call_tool_result_to_value` could all change shape while the CLI tests
  stayed green against bytes the daemon had stopped emitting — the `CrucibleClient`/D1 failure
  mode this plan indicts, reintroduced by the task meant to close it. The capture is now a
  permanent test: `crucible-daemon`'s `agent_manager::tests::parity_capture` drives the same
  `ReactorTestHarness` (real `WorkspaceTools` dispatcher, real permission gate for the internal
  arm; `OwnsToolsMockAgent` with `agent_name: "claude"` for the delegated one) and asserts the
  broadcast events equal the committed fixtures, id-normalized. Mutation-verified twice —
  replacing `data["display"]` with just its `kind` kills all four arms, and adding a field to the
  ACP pass-through `tool_result` envelope kills exactly the two delegated ones. It is **not**
  `#[ignore]`d: `#[ignore]` is for tests needing a daemon, Ollama or an agent binary, and this
  needs only mocks and a `TempDir`. Gating it would defeat the point, which is that CI notices.
  A fifth test pins that the delegated read fixture quotes what `read_file` actually returns, so
  the pair cannot drift into comparing a delegated agent against a fiction.
- **I2 — the fixtures were not byte-faithful.** The internal `tool_result` carried
  `"{\"result\": \"Replaced 1 occurrence(s)\"}"` — a space after the colon, which is Python's
  `json.dumps` default. The daemon produces that string with `serde_json::Value::to_string()`,
  which is compact. Fixed with a `compact()` helper (`separators=(",", ":")`), and the "copied
  field for field from a live capture" wording in the script and `assets/fixtures/README.md`
  softened: these are the daemon's output driven by two *named mock agents*, not a transcript of
  a real `claude` session.
- **M1 —** the header comment in `acp_parity_tests.rs` said the TUI "discards" tool descriptions.
  It does not: `render_description` paints a dimmed indented line and `CachedToolCall.description`
  is populated. The dead link is one hard-coded `let description = None` in
  `chat_runner/commands.rs`. Reworded.
- **M2 —** the internal fixture's `interaction_requested` omitted `diffs`, but a real gated
  `edit_file` builds `PermRequest::tool(..).with_diffs(synthesize_diffs(..))` — the same call that
  put `diffs` on the `tool_call`. Inert (the converter has no arm for the event) but it contradicted
  the fixture's own comment. Now present, and the capture test proves it.
- **M3 —** the delegated fixture's `args` used `path`; a real Claude Code edit arrives as
  `rawInput` keyed `file_path`. Both are in `ToolDisplay`'s `PATH_KEYS`, so the frame is unchanged
  — which is now *tested* rather than assumed, since each arm carries its own real key.
- **M4 —** `without_the_provenance_badge` used a global `.replace`. It now asserts exactly one
  occurrence and uses `replacen(.., 1)`, so a future two-badge fixture cannot let the
  normalization scrub an unrelated divergence along with the badge.
- **M5 —** none of the parity fixtures were pumped through the invariant sweeps, though they carry
  the only ACP-shaped payloads in `assets/fixtures`. Added to both
  (`replay_acp_parity_fixtures_80x24`, `invariant_acp_parity_fixtures_every_frame`).
  `check_spacing_between_non_tool_containers` is deliberately excluded from the second: it
  classifies any `● `-prefixed line as a tool card, but that glyph is also the assistant's
  response bullet, so a paragraph followed by a pending tool reads as two adjacent tools and it
  demands they be flush. The blank line between them is correct; the checker cannot tell the two
  bullets apart from stripped text.
- **M6 —** `scripts/gen_acp_parity_fixtures.py` had a shebang but no execute bit, and underscores
  where the other executable tools in `scripts/` use hyphens. Now
  `scripts/gen-acp-parity-fixtures.py`, `+x`, matching `gen-third-party-notices.py`.

**Verification:** `cargo nextest run -p crucible-cli` — 1945 passed, 71 skipped at the time
(1970 after A4 and the review fixes). `cargo fmt --all --check` and
`cargo clippy -p crucible-cli --all-targets` clean. The daemon was not modified by Task 7 itself;
the review's I1 added `agent_manager/tests/parity_capture.rs` (tests only).

---

## Task 8: Fix the misnamed, silently-skipping replay test — DONE (`f425be299`)

> **Outcome differed from the plan.** The silent-skip pattern was in **9** tests, not 1 — with the
> recordings removed, nine passed against an empty fixture directory. Fixed at the choke point:
> `helpers::fixture_path` now asserts existence, making "name a fixture, get a non-existent path"
> unrepresentable, which deleted 7 guards and 2 hand-rolled path blocks.
>
> **Option 1 was not available.** `acp-demo.jsonl` has five dependents (`assets/acp-demo.tape`,
> `justfile:160`, `scripts/validate-demos.sh`, the fixtures README, `docs/Help/Concepts/Session
> Replay.md`), so deleting it would break the demo tooling and the demo validator. Took Option 2.
>
> **The fixture is also malformed** — 26 `tool_call` events are 13 calls emitted twice (second copy
> sharing the `call_id`, different title, arriving *after* the answer), and every `tool_result`
> carries a UUID matching no call, so no result ever attaches. The test is renamed
> `replay_malformed_acp_demo_recording_80x24` and reframed as a corrupt-transcript robustness
> input. **No snapshot was taken** — the render is visibly wrong, and snapshotting would pin
> artifacts as truth.

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

## Task 9: Assert the four dead recorded fixtures — DONE (`b39f37d68`)

> **Outcome differed from the plan.** 1 test → 5, mutation-verified. The parity harness does *not*
> reach this layer: it projects `TurnEvent`, and this test drives `CrucibleAcpClient` (whose output
> is a `StreamingChunk` callback) because `AcpAgentHandle` has no transport seam. A local
> `ChunkShape` applies the same idea one layer down.
>
> **Cursor is not "too thin"** as the plan claimed — it is the only recorded capture of the
> refusal/produced-nothing path and now carries real assertions. **Gemini genuinely is unusable**:
> 305 bytes, one outbound `initialize`, no agent response at all. It got a structural guard that
> fails if someone re-records it, rather than a test pretending to cover it. A `record-acp-fixture`
> recipe documents the procedure (it stops any running daemon first — the recorder is daemon-side).
>
> **This task produced the branch's two most valuable findings**, both from reading the wire:
> `usage_update` (→ Task 15) and the dropped error detail (→ Task 16). Also: only opencode streams
> thought chunks, making it the natural regression fixture for C4.

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

## Task 10: Pin the permission-modal difference (C3) — DONE (`fbe7cf465`)

> Four frame tests pin the coarse `kind`-derived name, the surviving diff body, the internal
> contrast leg, and the `Execute` boundary — under a comment naming `unstable_tool_call_name` /
> schema 1.6.0 as the upgrade that *should* change them. See the corrections recorded on C3: the
> plan's "ACP attaches no diffs" was wrong, `Execute` hides the divergence entirely, and
> `ToolKind::Other` → `acp_tool` renders no diff (an open gap).

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

## Task 11: Resolve the dead `CrucibleClient` path (D1) — DONE

**What shipped** (maintainer approved the deletion; two commits so the decisions revert
independently):

- **`crates/crucible-daemon/src/acp/acp_client.rs` deleted — 740 lines, 15 tests** (the plan said
  "~13"; the real count was 15). It held `CrucibleClient`, `WriteInfo`, and a free `spawn_agent`.
  Verified unreachable by grepping the whole workspace — `crucible-cli`, `crucible-web`, `tests/`,
  `benches/`, `examples/`, Lua and docs — not just the daemon: the only hits outside the file were
  its own re-export at `acp/mod.rs:30`. `WriteInfo` had no consumer outside `acp_client.rs`
  either, and the free `spawn_agent` was shadowed everywhere by the *method*
  `CrucibleAcpClient::spawn_agent` in `acp/client/connection.rs`. The daemon's `tokio-util`
  `compat` feature went with it — `acp_client.rs` was its only user in that crate.
- **`crates/crucible-daemon/src/acp/protocol.rs` deleted — 129 lines, 3 tests (D2).** The claim
  checked out and went further than the plan stated: `ACP_VERSION` was dead too, read only by this
  module's own `Default` impl and its own tests. Production negotiates the wire version through
  `agent_client_protocol` (`InitializeRequest::new(1u16.into())`, `acp/client/connection.rs:265`)
  and never consults the local tuple. The one out-of-module consumer,
  `tests/acp_integration/error_propagation.rs::test_error_protocol_version_mismatch_is_reported`,
  compared two `ProtocolVersion`s through a `protocol_guard` helper defined in that same test file
  — a test of test-local logic. Removed, with a comment in its place recording why. Note the name
  collision: `acp/client/protocol.rs` is the *live* handshake module and stays.

**Net:** 869 lines and 19 tests removed (`cargo nextest list -p crucible-daemon --features
test-utils`: 2670 → 2651). No production behavior changed; `cargo check --workspace --all-targets`
and clippy are clean.

**Original task text follows.**

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

## Task 12: Document the story and the contract — DONE

Most of this landed incrementally: US-307 was written by Task 2 and widened by Task 7, and Task 6
gave US-203 its interleaving criteria. What was left was the part the earlier tasks could not do
for themselves — checking that the story still describes what shipped, and writing down the
contract.

**Two things in the task description were wrong; do not cite them.**

- **Step 1's draft acceptance text does not match reality.** It promised "equivalent `TurnEvent`
  shape sequences from `AcpAgentHandle` and `GenaiAgentHandle`", which this plan's own header
  rules out as structurally impossible (see below), and "structured tool results stay
  structured", which Task 4 withdrew as a false premise. Neither is in the shipped story.
- **Step 2 names the wrong boundary.** It says "`TurnEvent` is the parity boundary". It is not —
  `SessionEventMessage` is. The header of this plan corrected that in its second draft; Task 12's
  body was never updated to match. `Systems.md` records the corrected version.

**What shipped:**

- **US-307 narrowed to what the tests prove.** It had claimed "the badge is the only sanctioned
  frame difference" flatly. The evidence is two fixture pairs, so the claim is now explicitly
  per-behaviour: an `edit_file` turn with a late diff and a `read_file` turn with a multi-line
  result, each recorded from both agents. A2's `read_file` half — A4's resolution — is named:
  a delegated tool's result collapses into the card header exactly as the internal tool's does,
  because the summary table keys on the humanized name both spellings share. A new **Not
  claimed** line states what the pair deliberately does not cover: the coarse permission-modal
  name (C3, pinned separately), the statusline (A3/US-205 — since RESOLVED by Task 15; both
  fixtures omit the limit events, so this pair is silent on it either way),
  and the `description` asymmetry, which costs no pixels only because the converter drops
  descriptions on every path.
- **`Systems.md` gained a "Presentation Parity Boundary" section** stating: `SessionEventMessage`
  is the boundary; a new `AgentHandle` gets correct presentation for free iff it emits that
  vocabulary with the same fields populated; `TurnEvent`-level cross-agent equality is
  structurally impossible (internal yields `ToolCall` + `ToolBatchEnd` and receives its result
  **inbound**, an `owns_history` agent yields `ToolCall` + `ToolResult` **outbound**, and
  `GenaiAgentHandle` never yields a `ToolResult` at all) and **must never be asserted** —
  `TurnEvent` tests are per-agent contract expectations; and `display_parity.rs` sits above the
  boundary at `StreamingChunk`, so a green run there says the ACP client parsed the wire, not
  that the turn renders.

---

## Task 13: C4 — ACP thought chunks must reach the screen — DONE

Task 6 narrowed the *converter's* thinking guard so interleaved thoughts survive. Reviewing it
surfaced that on the ACP path there were no thoughts to survive: the client never produced one.

**What shipped:**
- `acp/client/streaming.rs` gained a `SessionUpdate::AgentThoughtChunk` arm in both update
  appliers. The live one (`apply_session_update_with_callback`) mirrors `AgentMessageChunk`'s
  `ContentBlock` match and emits `StreamingChunk::Thinking`, giving that variant its first
  production producer. The rest of the chain needed no change — `acp_handle.rs` already mapped it
  to `TurnEvent::Thinking`.
- **Thought chunks deliberately get no duplicate-resend guard.** `is_duplicate_resend` compares
  against `StreamingState::accumulated_text`, which is the assistant's *answer*; sharing it would
  let a thought suppress an answer chunk and vice versa whenever the two matched. A separate
  thinking twin was not added either: an agent that replays its whole reasoning block is already
  handled downstream, source-agnostically and turn-scoped, by
  `SessionEventStream::is_thinking_replay` — and a second content-equality rule inside the client
  would reintroduce exactly the silent-deletion hazard Task 6's I1 fix closes. A regression test
  (`thought_chunks_stay_out_of_the_answer_text`) pins that the two channels cannot mask each other.
- **`StreamingState` gained no thinking accumulator.** `accumulated_text` exists only to serve
  `is_duplicate_resend` and `formatted_output()` — the answer text. A parallel
  `accumulated_thinking` would have no reader: the live path emits per chunk through the callback,
  and `acp_handle.rs` discards the assembled `_content` entirely. Adding one would be another
  `CrucibleClient`-shaped dead path (D1). The non-callback `apply_session_update` therefore matches
  the variant only to keep it out of the terminal ignore arm and `trace!`s it, with a comment
  saying why reasoning is not folded into `formatted_output()`.
- Mock hook `CRU_MOCK_STREAM_THOUGHTS` (child-scoped env, value-read not presence-read):
  `;`-separated thoughts, the first emitted *before* the text chunks and the rest *after*, which is
  the think → narrate → think shape a delegated agent actually produces.

**Tests:** `acp_thought_chunks_reach_the_turn_stream` and
`mock_thinking_hook_set_to_empty_scripts_no_thoughts`
(`tests/acp_integration/turn_event_parity.rs`); `thought_chunks_stay_out_of_the_answer_text`
(`acp/client/streaming.rs`). The rest of the chain was already pinned —
`TurnEvent::Thinking` → `thinking` SessionEvent in `agent_manager/tests/messaging.rs`, and
`thinking` → a rendered block in
`user_story_tests/acp_parity_tests.rs::a_delegated_agent_second_thought_reaches_the_screen`.

---

## Task 15: A3 — consume ACP `usage_update` — DONE (`f70542b19`)

**Not in the original plan.** Found while table-driving the recorded fixtures in Task 9: real
agents put context-window data on the wire and Crucible discarded it.

- **The frame never deserialized.** `SessionUpdate::UsageUpdate` is gated behind the
  `unstable_session_usage` cargo feature, which `crucible-daemon` does not enable, and
  `SessionUpdate` is internally tagged on `sessionUpdate`. So the whole `SessionNotification`
  failed to parse and died at a `warn!`, never reaching a match arm. **Any** unstable update type
  an agent sends is lost the same way — worth remembering the next time a field looks "ignored".
- **The feature was deliberately not enabled.** Extracting from the raw JSON follows the pattern
  `acp/client/usage.rs` already establishes, and here it is load-bearing rather than stylistic:
  enabling the flag only moves the failure to the next unstable variant an agent sends.
- **Zero TUI changes.** `size` → the existing `SessionEventMessage::context_limit_resolved` (new
  `ContextLimitSource::Agent`); the client arms in `chat_runner/commands.rs`,
  `chat_app/message_handlers.rs` and `chat_runner/stream.rs` already existed. `used` rides the
  existing `extract_usage`/`take_last_usage` → `TurnEvent::Usage` path.
- **`used` and the internal agent's `total_tokens` mean the same thing**, one moment apart:
  measured against the fixtures, claude's `used` = total − 6 and opencode's = input + cacheRead
  exactly (total additionally includes that turn's output). `used` is occupancy *before* the
  response is appended. Good for parity — `context_used` means the same thing on both agent types.
  A `used`-derived floor seeds `last_usage` only when no `Usage` event arrives, so the statusline
  never draws a confident `0% ctx`.
- **`cost` dropped** — no consumer anywhere (YAGNI).
- The recorded fixtures now assert exact context-window numbers from real claude/opencode traffic.

**Tests:** `acp_integration/context_usage.rs`, three in `turn_event_parity.rs`, four in
`acp/client/usage.rs`, plus a pin that `usage_update` does *not* deserialize as a typed
`SessionNotification` — which will start failing usefully if the variant is ever stabilized.

---

## Task 16: surface the ACP error detail agents actually send — DONE (`0a962a978`)

**Not in the original plan.** Also found in Task 9. `acp/client/streaming.rs` read only
`error.message`, so a codex failure surfaced as `Internal error (code: -32603)` while the
actionable sentence sat in `error.data`.

The nesting was deeper than first described: `data.message` is itself a **stringified JSON
envelope**, so the sentence is two keys down. Folding in `data.message` naively would have dumped
a JSON blob at the user. `describe_rpc_error` therefore recurses (`message` → `error`, parsing any
string that is itself a JSON object), is depth-bounded at 4, contributes nothing for unreadable
shapes (`{}`, `null`, arrays, non-string `message`), and drops the detail when the base message
already contains it. Both read sites fixed.

Codex is the only recorded fixture with a JSON-RPC error frame at all, so every other `data` shape
is covered by unit test rather than fixture. Nothing downstream parses this text — the one piece of
string surgery (`rpc_client/agent/convert.rs`) only strips *leading* Display labels.

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
