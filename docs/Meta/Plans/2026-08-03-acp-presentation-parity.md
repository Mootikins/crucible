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

**Known pre-existing breakage (not caused by this branch).** `just clippy` fails under `-D warnings`
on `clippy::cloned_ref_to_slice_refs` at `daemon_plugins/tests.rs:395` and `skills/discovery.rs:383`
— a newer clippy than the code was written against. `just ci` will fail at final verification until
these are fixed; fix them in a separate commit rather than folding them into a parity task.

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

Run: `just test-crate-filter crucible-daemon 'acp_'`
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

**Why this one is legitimate** (unlike a `TurnEvent`-level equality assertion): the fixtures are
`SessionEvent` JSONL, i.e. the layer *after* the two agents converge. Pumping both through
`StoryRuntime` exercises the single shared renderer, so any frame difference is a genuine
presentation divergence rather than the by-design `owns_history` asymmetry.

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

Run: `just test-crate-filter crucible-cli 'acp_and_internal_agents_render'`

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
