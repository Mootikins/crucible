---
title: Attributed Diff Review
description: Per-tool-call change attribution in cru web — a composed-hunk review queue that knows which tool call, turn, and prompt produced each line.
tags:
  - meta
  - plan
  - web
  - sessions
status: draft
created: 2026-08-09
---

# Attributed Diff Review

> Web only. Terminal link detection is per-emulator and uncontrollable, so
> the TUI cannot host this. See [[Web User Stories]].

The original question was how an agent could cite `src/foo.rs:42` in chat and
have it open in an editor. That question dissolves: with a controlled renderer
the model does not need to cooperate, and for *edits* there is a better source
of truth than anything the model writes — the changes it actually made.

Two mechanisms, two purposes. Do not merge them:

| | Source | Trust | Covers |
|---|---|---|---|
| **Attribution** | tool calls + workspace trees | ground truth | code the agent **changed** |
| **Linkify** | `path:line` in message text | best-effort | code the agent **referenced** |

Linkify resolves candidates against the session's allowlisted roots at render
time and renders non-resolving refs as plain code. The model is never told the
feature exists, so it cannot misuse it — **linkability is a property of
resolution, not of syntax**. Bare names with no separator resolve as wikilinks
against the kiln, never as file paths, and render differently from code refs.
See [[Workspace and Runtime Targets]] for the root-resolution rules.

The rest of this document is attribution.

## Verified state

Everything below was read against `82a0cff0c` on 2026-08-09.

| Fact | Evidence |
|---|---|
| Turn-level snapshots already exist | `crucible-daemon/src/workspace_snapshot.rs` — `SnapshotMap`, keyed `(session_id, node_id)` |
| Snapshot mechanism is temp-index + `write-tree` + `commit-tree`, **not** `git stash create` | `workspace_snapshot.rs:206`; untracked files ARE captured via `git add -A` |
| The module doc-comment contradicts its own implementation | `workspace_snapshot.rs:11` still claims "Uses `git stash create`" — stale |
| `SnapshotMap`'s only consumer is `AgentManager` | graphify neighborhood; field at `agent_manager/mod.rs:657` |
| Turn snapshot is captured on send | `agent_manager/messaging/send.rs:149` |
| Restore path (undo) | `agent_manager/models.rs:969` |
| Session cleanup drops snapshots | `agent_manager/mod.rs:1329` |
| Gate seam already exists | `agent_manager/messaging/tool_call.rs:10` — "`pre_tool_call` is the enforcement point for gate-style plugins" |
| **`pre_tool_call` cannot gate ACP agents** | `tool_call.rs:15-21` — an external agent runs tools in its own process; denial arrives after the fact |
| The precedent for refusing unenforceable gates | `session_lifecycle.rs:159` `unenforceable_isolation()`, called at `:100` |
| `tool_call.rs:20` misdirects to `rpc/dispatch.rs` for `unenforceable_isolation` | it lives in `session_lifecycle.rs` — stale cross-reference in the source |
| Modes are open string IDs, not an enum | `crucible-core/src/types/mode.rs` — `ModeDescriptor { id: String, .. }`, `default_internal_modes()` |
| A hardcoded mode check already exists | `agent_factory.rs:293` — `if mode == "plan"` |
| RPC methods are an explicit allowlist + match arm | `rpc/dispatch.rs:45-70` (list), `:367` (routing) |
| Web has the render targets already | `DiffViewer.tsx`, `MultiEditDiff.tsx`, `FileViewerPanel.tsx` (accepts `scrollToLine`), `ToolCard.tsx` |
| Panels register by region | `web/src/lib/register-panels.tsx:22-34` |

## Design

### §1 Ledger and composed hunks

Append-only, per session, daemon-side:

```
Interval { tool_call_id, node_id, before_tree, after_tree, roots_touched }
```

Written after any tool call that could write; deduped by tree SHA, so a tool
call that changed nothing produces no interval and no card.

The **review surface** is the *composed* diff (`session_base` → worktree),
not the intervals. Attribution is an overlay computed on demand by
intersecting interval diffs against composed hunks — many-to-many.

**The action unit is the composed hunk, never a tool call's contribution.**
Rejecting an early tool call's edit after later edits touched the same lines
is a three-way merge, and it fails in exactly the cases where it matters most.
Every composed hunk is independently revertible against `session_base` by
construction. Attribution stays informational.

Review state per composed hunk: `unreviewed | accepted | rejected`.
Session-scoped daemon state, so web and TUI agree and it survives resume.

**Hunk identity must be content-derived, not positional.** The composed diff is
recomputed on every worktree change. If a hunk is keyed by `(path, line)`, an
agent edit anywhere above it shifts the line and the review state either
evaporates or — worse — lands on a *different* hunk, letting unreviewed agent
code through the gate. Key on a hash of
`(root, path, before_content, after_content, base_range)` — where `base_range`
is the hunk's `LineRange` in **`session_base` coordinates**, which never move
because `session_base` is immutable for the session's lifetime. **Fail
closed**: a hunk whose identity is not recognised is `unreviewed`.

> **Corrected 2026-08-10.** This section originally specified "an ordinal
> disambiguator for identical repeats". That ordinal is positional in the
> *current worktree*, so removing a sibling renumbers the survivor onto the
> removed hunk's identity and hands it that hunk's `accepted` decision. The
> implementation did exactly what was written here. See
> [[2026-08-10-attributed-diff-review-remediation]] §C1.

The one documented exception to fail-closed: a gate query for a file under
**no** tracked root returns `false` rather than blocking. No hunk exists, so no
human action could satisfy the gate, and `hold_for_review` has no timeout by
design — blocking there is an unreleasable hang, not a safe default.

`session_base` is persisted with the session, never recomputed on resume —
re-deriving it would silently empty the composed diff of everything done before
the restart.

**Gate/browser decay needs no mechanism.** The queue *is* the unreviewed
subset. It drains as you review and empties on commit; the ledger persists.
No mode switch, no completion state.

### §2 Surfaces

| Surface | Role | Change |
|---|---|---|
| `ToolCard.tsx` (chat) | index entry — "what this call did" | add superseded indicator; accept/reject **only** for hunks still live in the composed diff |
| `ChangesPanel` (new, right) | the queue — roots → files → hunks | new panel, registered alongside Activity/Backlinks |
| `FileViewerPanel.tsx` (center) | where review happens | add a CodeMirror decoration layer + per-hunk gutter |

Review happens inline in the real buffer, not a side-by-side pane — real
highlighting, real folding, real go-to-definition, and your spatial memory of
the file survives. A decoration set plus a gutter widget is far less work than
a second viewer.

Superseded detection (interval hunks that no longer intersect any composed
hunk) is cheap, keeps the ToolCard honest, and doubles as an agent-thrash
signal. When a hunk is superseded its accept/reject buttons vanish — the
action unit is gone.

Gutter carries the attribution chip (`turn 7 · Edit`); clicking it scrolls
chat to the ToolCard and its reasoning.

### §3 Gating

Fires before a **writing** tool call whose target file has unreviewed hunks —
not on any unreviewed hunk anywhere, which would be maximally annoying. The
agent keeps working elsewhere.

For `delegate_session`, whose targets are unknowable, fall back to blocking if
the root has any unreviewed hunk **from a previous turn**. `Bash` does *not*
take this fallback: almost every turn ends in a build or test command, so
blocking `Bash` on the root would block the agent on the edits it just made,
every turn, making `normal` mode feel broken.

> **Corrected 2026-08-10.** This originally said `Bash` "may write
> unattributably". `Bash` *is* bracketed, so its synchronous writes are
> attributed; only writes landing *after* it returns (async formatters, LSP,
> watch-mode compilers) escape. Those fall to the §5 watcher — which is a
> staleness fix, not a correctness one, since an unaccounted-for write is
> already `external` by construction.

Only hunks the ledger owns participate in gating. `external` hunks (§5) never
block: they are the user's own edits, and blocking the agent on them is
nonsense.

A blocked agent must never look stalled: `SessionStatusChips` gets a distinct
"waiting on review" state. No default timeout — a blocked agent is a correct
agent — but the state has to be loud.

**Rejection is a conversation event.** If the agent is not told, it re-applies
the same edit and you are in a loop. Rejection emits back into the session:
*"user rejected the edit to `src/foo.rs:88-94`; reverted."* Accept is silent —
no feedback, no tokens.

Reject writes to disk immediately (revert against `session_base`). Deferred
marks would need conflict handling while the agent keeps editing.

**`external` hunks are displayed but not rejectable.** Reverting one would
destroy the user's own concurrent edit while reporting that an agent edit was
undone. Show them for context — the composed diff must stay honest — with no
reject affordance.

### §4 Modes carry the policy

Do not `match` on mode inside the gate — that is how `agent_factory.rs:293`'s
`if mode == "plan"` metastasizes. Attach policy to the descriptor:

```
ReviewPolicy { gate: None | PreWrite | PostTurn }
```

| Mode | Policy | Why |
|---|---|---|
| `plan` | `None` | read-only; gate is vacuous |
| `normal` | `PreWrite` | §3 |
| `auto` | `PostTurn` | see below |
| unknown / agent-advertised | `PreWrite` | conservative default |

**Auto-approve should mean "don't interrupt me," not "don't review."** Today
auto means changes go unexamined forever. With a ledger, auto keeps the receipt
and surfaces the queue at turn end. Strictly better, and free once the ledger
exists. Lua- and config-defined modes get review policy without touching the
gate.

### §5 Capture point

Hang capture off the **write path**, not the session-turn path. Delegated ACP
sessions and workflow steps write to the same workspace without producing turns
in the parent, so a parent-only bracket collapses them into one opaque interval.

- **Nested ledgers.** A delegated session captures its own intervals against
  the same root. The parent's `delegate_session` interval holds a *reference*
  to the child ledger, not a blob. Expanding a delegation card expands into the
  child's tool calls. Attribution depth follows session depth.
- **Watcher backstop.** `watch/` + `file_watch_bridge.rs`, debounced on
  workspace roots, catches what bracketing cannot: async formatters landing
  after a `Bash` call returns, Lua plugins writing directly, and the user
  editing in their own editor mid-session. Unowned changes are labeled
  `external` rather than misattributed. Required regardless, or the composed
  diff quietly lies.
- **Parallel writers.** Two delegations editing one workspace concurrently make
  bracketing unsound. Rule: **concurrent sub-sessions get separate worktrees.**
  The ledger detects the violation (overlapping open intervals on one root) and
  degrades to `external` rather than emitting confident wrong attribution.

### §6 Comments

Anchored to **ranges, not hunks**. A hunk comment is a comment whose range
equals a hunk. Ranges also allow commenting on unchanged code, spanning several
hunks, and later extending to note ranges in a kiln.

```
Comment { root, path, base_tree, line_range, body, author: Human | Agent, resolved }
```

Anchor drift is the known-hard part: anchor to the composed diff's base,
re-project through later diffs, mark `outdated` when it no longer projects —
the GitHub behavior, which users already understand.

Comment and reject compose but stay distinct: comment = "change this",
reject = "undo this", reject-with-comment = both. Reject-with-comment is inline
code review feeding a live agent loop, and it is the payoff of the whole
design: attribution tells you *why the line exists*, comments let you answer.

Because a delegating agent may itself be the reviewer, **review is an RPC + Lua
tool surface first and a web panel second.** Daemon owns the logic; see
[[Systems]].

## Type mapping

New types, canonical locations per the one-location rule:

| Type | Crate / module | Notes |
|---|---|---|
| `Interval` | `crucible-core/src/session/types/` | crosses the RPC boundary; serde |
| `Ledger` | `crucible-core/src/session/types/` | `Vec<Interval>` + child ledger refs |
| `ComposedHunk` | `crucible-core/src/session/types/` | id, root, path, line ranges, `Vec<tool_call_id>` |
| `ReviewState` | `crucible-core/src/session/types/` | `Unreviewed \| Accepted \| Rejected` |
| `ReviewPolicy` | `crucible-core/src/types/mode.rs` | new field on `ModeDescriptor` / `SessionMode` |
| `Comment` | `crucible-core/src/session/types/` | range-anchored |
| `ReviewError` | `crucible-daemon/src/review/` | thiserror at the RPC boundary only |
| `ReviewLedgers` | `crucible-daemon/src/review/` | `DashMap`, mirrors `SnapshotMap` |

Reuse, do not duplicate:

- `WorkspaceSnapshot` — already produces the tree SHAs. `commit_id` **is**
  `before_tree`/`after_tree`. Do not invent a second snapshot type.
- `SessionMode` / `ModeDescriptor` — extend, do not parallel.
- `ToolCall` (`crucible-core/src/events/session_event/tool_call.rs`) — the
  `tool_call_id` key. Do not mint a new identity.
- `FileDiff` (`crucible-core/src/types/acp.rs:319`) — already imported by
  `tool_call.rs`; check before adding a diff payload type.

RPC surface (allowlist `rpc/dispatch.rs:45-70`, arm at `:367`, handlers in
`server/session/`):

```
review.list_hunks      review.set_state     review.revert_hunk
review.comment         review.resolve_comment
```

Event: a `review_changed` constructor on `SessionEventMessage`
(`crucible-core/src/protocol/rpc.rs:81`, following `mode_changed` at `:324`) —
the second-highest-degree node in the graph, so the event belongs there rather
than in a side channel.

R5 must also expose the same five operations as Lua tools, not only RPC. §6
requires a delegating agent to be able to review a sub-session; an RPC-only
surface does not deliver that.

## Graphify analysis

Graph built 2026-08-09 over `crucible-daemon` + `crucible-core` (685 code
files, AST-only — no LLM, zero tokens): **15,081 nodes / 35,505 edges /
565 communities**, in `graphify-out/`.

*Health warning (surfaced per graphify's honesty rules):* 2,571
dangling-endpoint edges and ~3,100 collapsed edges. Both are benign artifacts
of Rust AST extraction — dangling endpoints are references to stdlib types
(`String`, `Arc`, `Option`) that are not nodes, and collapsed edges are the
same struct referencing the same type across many fields. No action; noted so
the numbers are not read as clean.

**God nodes** (degree):

| Node | Degree |
|---|---|
| `Request` (`core/protocol/rpc.rs`) | 263 |
| `SessionEventMessage` | 186 |
| `Response` | 140 |
| `RpcDispatcher` (`daemon/rpc/dispatch.rs`) | 127 |
| `map_server_resp()` | 109 |
| `DaemonPluginLoader` | 100 |
| `Session` | 93 |
| `KilnManager` | 84 |
| `AgentManager` | 83 |

Three things the graph settled that reading alone did not:

1. **`SnapshotMap` has exactly one consumer** — `AgentManager`. Its
   neighborhood is 17 nodes, entirely self-contained plus that one edge.
   Extending snapshots to per-tool-call granularity touches one owner, not a
   web of callers. This is the cheapest seam in the design.
2. **The RPC quartet dominates everything.** `Request`/`Response`/
   `SessionEventMessage`/`RpcDispatcher` are the top 4 by a wide margin, which
   confirms the review surface belongs there and not in a bespoke channel — and
   also that touching them is high-blast-radius, so the new methods should be
   additive arms only.
3. **`AgentManager` (83) is a hub, not a leaf.** Adding ledger ownership beside
   `snapshots` puts more into an already-central type. Worth watching against
   the "co-locate related state" principle, which argues for it, versus its
   existing size, which argues against.

## Work partition

Partitioned by file; no two groups write the same file.

**R1 — Ledger core.** Owns `daemon/src/review/` (new), `core/src/session/types/`.
Interval capture, composed-diff computation, attribution intersection. Extend
`WorkspaceSnapshot` to expose tree SHAs without restoring. Fix the stale
`workspace_snapshot.rs:11` doc-comment.

**R2 — Capture wiring.** Owns `agent_manager/messaging/send.rs`,
`agent_manager/mod.rs`, `agent_manager/models.rs`. Per-tool-call capture beside
the existing per-turn capture; nested ledgers for delegation.

**R3 — Gate.** Owns `agent_manager/messaging/tool_call.rs`, `core/types/mode.rs`,
`agent_factory.rs`. `ReviewPolicy` on the descriptor; replace the
`if mode == "plan"` check. **Must handle the ACP limitation** (see Open
questions).

**R4 — Watcher backstop.** Owns `watch/`, `file_watch_bridge.rs`. Unowned-change
detection, `external` labeling, overlapping-interval detection.

**R5 — RPC.** Owns `rpc/dispatch.rs`, `server/session/` (new `review.rs`),
`core/protocol/rpc.rs`. Five methods + one event variant.

**R6 — Web.** Owns `ChangesPanel.tsx` (new), `register-panels.tsx`,
`FileViewerPanel.tsx`, `ToolCard.tsx`. Decoration layer, gutter, queue panel.

Testing per [[TUI User Stories]] conventions: R1 gets unit tests on
attribution intersection with `tempfile::TempDir` git fixtures; R6 needs a
story in `docs/Meta/Web User Stories.md` plus vitest coverage.

## Open questions

1. **The gate is unenforceable for ACP agents.** `tool_call.rs:15-21` and
   `rpc/dispatch.rs:2021` are explicit: an external agent executes tools in its
   own process, so a `pre_tool_call` denial arrives after the fact and stops
   nothing. The daemon already refuses to pair an isolation claim with an
   external agent (`session_lifecycle.rs:159` `unenforceable_isolation`). §3's
   `PreWrite` gate inherits this exactly. It must degrade to `PostTurn` for
   external agents and say so in the UI, following that precedent.

   > **Corrected 2026-08-10.** This originally claimed "**the ledger is
   > unaffected** — attribution still works for ACP; only the *blocking* does
   > not." That is false in the shipped code: `stream.rs:465-482` `continue`s
   > on the `agent_owns_tools` arm, so ACP tool calls are **never bracketed at
   > all**. Every write by `cru chat -a claude` is `external` and therefore
   > non-revertible. Bracketing that arm would not fix it either — the
   > notification arrives around execution the daemon does not control. The
   > honest fix is a turn-level bracket for owns-history agents. See
   > [[2026-08-10-attributed-diff-review-remediation]] defect 11.

   This is the one place where §4's "modes carry the policy" is not sufficient
   on its own: the effective policy is `min(mode_policy, agent_capability)`,
   and the UI must show the *effective* one, not the configured one. A mode
   chip reading "normal — gated" on a session that cannot gate is a lie about a
   safety property.
2. ~~**`git add -A` mutates the real index.**~~ **RESOLVED 2026-08-10.**
   `capture_tree` copies the real index to a tempfile and runs everything under
   `GIT_INDEX_FILE`; regression test `capture_leaves_the_real_index_alone` at
   `workspace_snapshot.rs:479-494`. Do not re-fix this. (It was cited as live
   evidence through three hops of remediation planning before being caught —
   which is why it is struck through here rather than deleted.)
3. **Ledger persistence across daemon restart** — unresolved. `SnapshotMap` is
   in-memory and dies with the daemon, which is acceptable for turn undo and
   not for a review queue that should survive resume. Either persist intervals
   to SQLite or recompute the composed diff from `session_base` on resume and
   accept losing per-call attribution.
4. **Snapshot cost at per-call frequency.** Each capture is several `git`
   subprocesses. Thirty edits in a turn on a large repo is real latency.
   Mitigations: capture only after calls that could write, dedupe by tree SHA,
   and consider in-process `gix` (already a dependency).
