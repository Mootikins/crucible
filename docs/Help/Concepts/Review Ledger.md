---
title: Review Ledger
description: Per-session attributed changes, the composed diff, the review queue, and the gate that holds unreviewed writes
status: implemented
tags:
  - review
  - sessions
  - attribution
  - web
  - agents
---

# Review Ledger

The review ledger answers two questions about an agent session: *what changed*, and *which tool call did it*. Every writing tool call is bracketed by git tree snapshots of the session's roots, and the difference between the session's starting tree and the worktree now — the **composed diff** — becomes a queue of hunks you accept or reject. In `normal` mode, a **review gate** holds any further write to a file until its unreviewed hunks are dealt with.

The evidence is the filesystem, not the agent's claims: changes are keyed on git tree SHAs rather than on what a call reported, so attribution works the same for the internal agent and for external [[Agent Client Protocol|ACP]] agents.

## What gets tracked

When a session's first message is sent, the daemon opens a ledger over the session's workspace, its [[Kilns|kiln]], and any connected kilns. Each root is normalised to its git repository top level and the current tree is captured once as `session_base`. Roots outside git are skipped — there is nothing to diff — and a session with no git-backed root simply has no ledger and no gate.

Around each tool call that could write (any tool not known to be read-only), the daemon records the tree before and after. If the trees differ, that becomes an **interval** attributed to that call's `tool_call_id`. A call that wrote nothing produces no interval. Two brackets open on the same root at the same time — typically a parent and a delegated child — are marked *contested*, and contested intervals are excluded from attribution rather than guessed at.

Two kinds of change deliberately get no attribution:

- **Your own edits.** Anything changed outside a bracket surfaces as an *external* hunk — listed so the diff stays honest, but never blockable and never rejectable.
- **Binary files.** Non-UTF-8 files have no line hunks and are skipped.

`bash` is bracketed like every other tool the daemon cannot prove read-only, so what a shell command writes is attributed to that call — but it is never *gated* (see below).

## The composed diff

The review surface is not the stream of intervals — it is the composed diff, `session_base` → worktree, recomputed on demand. Each hunk is a zero-context change cluster, which makes every hunk independently revertible: rejecting one restores that hunk's base text regardless of how many tool calls contributed to it, with no three-way merge.

Attribution intersects the two: an interval's changed lines are projected into the composed diff's coordinates, so one hunk can carry several tool calls, one call can span several hunks, and a call whose work was later overwritten attributes to nothing — the *superseded* signal the web tool cards render. Hunk identity is derived from content and base position, not worktree position, so decisions survive adjacent edits and daemon restarts.

## The review queue

Each hunk is `unreviewed`, `accepted`, or `rejected`. Absent a recorded decision, a hunk is unreviewed — decisions never transfer to lines the reviewer did not see.

- **Accept** records the decision, silently.
- **Reject** is one operation: the daemon reverts the hunk on disk immediately, records the rejection, and injects a user-role note into the session's conversation naming the file and lines — so the agent learns the revert happened and does not re-apply the edit. If it applies the same change again anyway, the hunk returns to the queue flagged as *reapplied*.
- **Comments** anchor to a line range (changed or not), and can be resolved.

Every queue movement emits a `review_changed` event, so open clients refresh without polling.

## The review gate

Whether a write waits on review is a property of the session's mode:

| Mode | Policy | Effect |
|---|---|---|
| `plan` | none | nothing gated, nothing owed |
| `normal` | pre-write | a write to a file with unreviewed hunks waits until they are reviewed |
| `auto` | post-turn | nothing is held; changes land in the queue for review after the fact |

Unknown mode ids fail closed to pre-write. External ACP agents degrade pre-write to post-turn — the daemon cannot hold a tool the external agent already ran — so an ACP session in `normal` mode reviews at turn end rather than being gated.

A held call **blocks rather than being denied**: a denial is text the model reads and retries; waiting is what the situation is. There is no gate timeout — the turn's own execution timeout and your cancel bound it — and the block is observable: a `review_gate` event fires on block and release, and `review.list_hunks` reports the current block under `gate`, naming the tool and the file it waits on. `delegate_session` names no file, so it is gated against any hunk left unreviewed by an *earlier* turn — a delegation is never blocked by the edits of the turn that issued it. `bash` is the other special case: its targets cannot be known from its arguments, and gating it session-wide would block almost every turn on its own edits (turns end in build and test commands), so it is deliberately never held — its writes are still captured and attributed.

The gate fails closed on structural damage: an unreadable `review.jsonl`, a tracked root that is gone, or a base tree lost to `git gc` degrades the root, and writes under it are held with a reason instead of slipping through unattributed. The release for that state is an explicit **rebase** (`review.rebase` / `POST …/review/rebase`): accept the worktree as it stands as the new base. That empties the queue for those roots, so it is always a deliberate human action.

## Where you meet it

**The web console.** The Changes panel lists the composed diff grouped root → file → hunk with accept/reject per hunk; the file viewer tones unreviewed, accepted, and external lines inline; status chips show the effective review policy and a "waiting on review" chip while the gate holds a call. It talks to five session-scoped routes:

```text
GET  /api/session/{id}/review/hunks
POST /api/session/{id}/review/rebase
POST /api/session/{id}/review/state
POST /api/session/{id}/review/comment
POST /api/session/{id}/review/comment/{comment_id}/resolve
```

These forward to the daemon's `review.list_hunks`, `review.rebase`, `review.set_state`, `review.comment`, and `review.resolve_comment` RPC methods. `list_hunks` returns the hunks plus `comments`, `degraded` roots, journal `integrity`, and the current `gate` block. There is no TUI review panel.

**The bundled `review` plugin** (`runtime/plugins/review/`) exposes the same operations as agent-callable tools: `review_list_hunks`, `review_set_state`, `review_comment`, `review_resolve_comment`. Every tool takes an explicit `session_id` because the session under review is usually not the caller's own: a delegating agent gets `child_session_id` from `delegate_session`'s result and reviews the child's diff before accepting it. Hunk bodies are truncated at 2000 characters — an agent deciding on a long hunk should open the file.

## Delegation

`delegate_session` is not bracketed — the child session keeps its own ledger over the same roots, and a parent bracket would mark every child interval contested. Instead the parent records a link to the child's ledger, and when the child ends, its intervals over roots the parent also tracks are folded into the parent's ledger. Attribution depth follows session depth: the delegated work shows up in the parent's composed diff, stamped with the child session it came from. See [[Delegation]].

## Lifecycle and persistence

- **Open** — on the session's first send, `session_base` is captured once per root and never recomputed.
- **Journal** — every mutation appends eagerly to `review.jsonl` in the session's storage directory, next to `session.jsonl`. On daemon restart (or when the web panel opens a resumed session), the ledger is replayed from the journal; a fresh base is never silently captured over an existing journal, because that would report the agent changed nothing.
- **Damage is graded** — a journal line that will not parse is skipped and recorded, scoping the resulting hold to one root where possible and to the whole session otherwise. Decisions are stamped with a fingerprint of the hunk arithmetic; a decision made under different arithmetic returns its hunk to the queue rather than landing on lines you never saw.
- **Retention** — the trees the ledger records are unreferenced git objects, so each session pins them with one tree-valued ref per repository, `refs/crucible/sessions/{session_id}` — invisible to `git log`, dropped when the session is deleted, and swept when a session directory disappears out of band.
- **End** — session teardown clears the in-memory ledger but leaves the journal on disk; the queue is still there when the session is resumed.

## Current limits

- The gate can only hold the internal agent's tool calls. External ACP agents are post-turn review only.
- Modes declared in Lua or config cannot yet declare a weaker policy for themselves; they take the conservative pre-write default.
- Writes no bracket saw — your editor, background processes — are attributed to nobody; they appear as external hunks, reviewable by eye but not gated or rejectable.
