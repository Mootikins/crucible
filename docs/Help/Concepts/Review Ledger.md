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

The review ledger answers two questions about an agent session: *what changed*, and *which tool call did it*. Every writing tool call is bracketed by git tree snapshots of the session's roots, and the difference between the session's starting tree and the worktree now — the **composed diff** — becomes a queue of hunks you accept or reject. In `ask` mode, a **review gate** holds any further write to a file until its unreviewed hunks are dealt with.

The evidence is the filesystem, not the agent's claims: changes are keyed on git tree SHAs rather than on what a call reported, so attribution works the same for the internal agent and for external [[Agent Client Protocol|ACP]] agents.

## What gets tracked

When a session's first message is sent, the daemon opens a ledger over the session's workspace and every [[Kilns|kiln]] it is attached to. The daemon snapshots each root once as `session_base`, through one of two backends. A root inside a git repository is normalised to the repository top level and the snapshot is the tree `git write-tree` produced. A root outside one — a kiln outside git is the expected shape — is snapshotted into a plain store under the daemon data root, as a manifest of one content hash per file; a stat key of size, mtime and inode keeps an unchanged file from being read again. The snapshot id says which store holds it, so a session recorded by an older build keeps replaying. Only a root that is not there at all is skipped, and a session with no reachable root has no ledger and no gate.

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
- **Undo** takes a reject back. Every reject, single or bulk, pushes one batch onto a per-session stack that the journal records, so the stack is multi-level and survives a daemon restart. `review.undo_reject` pops the top batch, puts each hunk's lines back on disk, lists the hunks as unreviewed again, and injects a second note telling the agent the rejection is withdrawn. A hunk whose file moved on since the revert refuses the whole batch as stale; nothing is written and the batch stays on the stack. The stack keeps the last 50 batches.
- **Bulk decisions** (`review.set_states`) accept or reject a list of hunks in one call. The daemon applies the list in order and reports each hunk it refused — unknown, stale, or external — beside the ones it applied; one refusal never stops the rest. A bulk reject is one undo batch and one conversation note. When a repository or I/O error ends the list early, the hunks reverted before it are still named in the note and the event, and the error is answered after them.
- **Comments** anchor to a line range (changed or not), and can be resolved.

A listing has a **scope**: `session` lists every hunk since `session_base`; `turn` lists only the hunks a tool call of the current turn touched. The turn starts at the last user message on the conversation's current path, so the daemon needs no marker. Under `turn`, external hunks are not listed, and a session with no turn yet lists nothing. Decisions and the gate always read the whole session.

Every queue movement emits a `review_changed` event, so open clients refresh without polling.

## The review gate

Whether a write waits on review is a property of the session's mode:

| Mode | Policy | Effect |
|---|---|---|
| `plan` | none | nothing gated, nothing owed |
| `ask` | pre-write | a write to a file with unreviewed hunks waits until they are reviewed |
| `auto` | post-turn | nothing is held; changes land in the queue for review after the fact |

Unknown mode ids fail closed to pre-write. External ACP agents degrade pre-write to post-turn — the daemon cannot hold a tool the external agent already ran — so an ACP session in `ask` mode reviews at turn end rather than being gated.

A held call **blocks rather than being denied**: a denial is text the model reads and retries; waiting is what the situation is. There is no gate timeout — the turn's own execution timeout and your cancel bound it — and the block is observable: a `review_gate` event fires on block and release, and `review.list_hunks` reports the current block under `gate`, naming the tool and the file it waits on. `delegate_session` names no file, so it is gated against any hunk left unreviewed by an *earlier* turn — a delegation is never blocked by the edits of the turn that issued it. `bash` is the other special case: its targets cannot be known from its arguments, and gating it session-wide would block almost every turn on its own edits (turns end in build and test commands), so it is deliberately never held — its writes are still captured and attributed.

The gate fails closed on structural damage: an unreadable `review.jsonl`, a tracked root that is gone, or a base tree lost to `git gc` degrades the root, and writes under it are held with a reason instead of slipping through unattributed. The release for that state is an explicit **rebase** (`review.rebase` / `POST …/review/rebase`): accept the worktree as it stands as the new base. That empties the queue for those roots, so it is always a deliberate human action.

## Where you meet it

**The web console.** The Changes panel lists the composed diff grouped root → file → hunk. Each hunk expands into a CodeMirror merge view (`HunkMergeView`) that shows the hunk's base text against its worktree text, with Accept and Reject controls; every control calls the daemon, never CodeMirror's own chunk action. Each file row and the panel header carry **Accept all** and **Reject all**: one confirm, then one `review.set_states` call for every unreviewed hunk in that file or in the whole review, in composed order. Every reject leaves a notification with an **Undo** action, which calls `review.undo_reject`; a refused hunk is named in one notification. A **Session / Turn** control chooses the listing scope. The file viewer tones unreviewed, accepted, and external lines inline; status chips show the effective review policy and a "waiting on review" chip while the gate holds a call. On a phone the panel opens from the More menu, and every control is a 44 px target. The panel talks to seven session-scoped routes:

```text
GET  /api/session/{id}/review/hunks?scope=session|turn
POST /api/session/{id}/review/rebase
POST /api/session/{id}/review/state
POST /api/session/{id}/review/states
POST /api/session/{id}/review/undo-reject
POST /api/session/{id}/review/comment
POST /api/session/{id}/review/comment/{comment_id}/resolve
```

These forward to the daemon's `review.list_hunks`, `review.rebase`, `review.set_state`, `review.set_states`, `review.undo_reject`, `review.comment`, and `review.resolve_comment` RPC methods. `list_hunks` returns the hunks plus `comments`, `degraded` roots, journal `integrity`, the current `gate` block, and the `scope` it answered under; an unknown scope is refused before the daemon is asked. `set_states` and `undo_reject` answer `applied` and `failed`, each failure naming the hunk and the reason. There is no TUI review panel.

**A plugin pass's own session.** The [[Reflection Pass|reflection and consolidation passes]] write their kiln notes with `create_note` and `update_note` in a session of their own, in `auto` mode, so every note they write is a hunk in that session's queue rather than a file staged somewhere else. The web sessions list carries a **Reflections** section, on the desktop shell and on the phone, so a pass is reachable from either; open it and the Changes panel disposes of its notes the way it disposes of any other session's edits. A reject reverts the note on disk.

**The bundled `review` plugin** (`runtime/plugins/review/`) exposes the same operations as agent-callable tools: `review_list_hunks`, `review_set_state`, `review_comment`, `review_resolve_comment`. Every tool takes an explicit `session_id` because the session under review is usually not the caller's own: a delegating agent gets `child_session_id` from `delegate_session`'s result and reviews the child's diff before accepting it. Hunk bodies are truncated at 2000 characters — an agent deciding on a long hunk should open the file.

## Delegation

`delegate_session` is not bracketed — the child session keeps its own ledger over the same roots, and a parent bracket would mark every child interval contested. Instead the parent records a link to the child's ledger, and when the child ends, its intervals over roots the parent also tracks are folded into the parent's ledger. Attribution depth follows session depth: the delegated work shows up in the parent's composed diff, stamped with the child session it came from. See [[Delegation]].

## Lifecycle and persistence

- **Open** — on the session's first send, `session_base` is captured once per root and never recomputed. The backend is chosen here, per root, and recorded in the snapshot id, so every later read of that root goes to the same store. A **rebase** is the one operation that chooses again.
- **Journal** — every mutation appends eagerly to `review.jsonl` in the session's storage directory, next to `session.jsonl`. On daemon restart (or when the web panel opens a resumed session), the ledger is replayed from the journal; a fresh base is never silently captured over an existing journal, because that would report the agent changed nothing.
- **Damage is graded** — a journal line that will not parse is skipped and recorded, scoping the resulting hold to one root where possible and to the whole session otherwise. Decisions are stamped with a fingerprint of the hunk arithmetic; a decision made under different arithmetic returns its hunk to the queue rather than landing on lines you never saw.
- **Retention** — the trees the ledger records are unreferenced git objects, so each session pins them with one tree-valued ref per repository, `refs/crucible/sessions/{session_id}` — invisible to `git log`, dropped when the session is deleted, and swept when a session directory disappears out of band. A plain root's snapshots are claimed the same way, by one keep file per root under the daemon's snapshot store. Nothing outside the daemon collects those, so the daemon sweeps the store itself on the same pass: a snapshot or a blob no live session claims is removed, and a claim is released only by its session going away, never by age. One thing does read the clock: a snapshot written in the last hour is never collected, however unclaimed it looks. A capture writes its snapshot before the ledger records the call that claims it, and a sweep landing inside an open bracket would take a state no later capture reproduces. `git gc` guards the same window the same way, with `gc.pruneExpire`.
- **End** — session teardown clears the in-memory ledger but leaves the journal on disk; the queue is still there when the session is resumed.
- **Archive** — the auto-archive sweep does not archive a session whose queue still holds an undecided hunk. It holds the session and says so in the log. Archiving takes a session out of the daemon's resident map, and a ledger is restored only for a session that map answers for, so an archived session's hunks would have no door left while the edits they describe are still on disk. A plugin pass is the case this protects: it writes its notes, ends, and nobody opens the queue for days.

## Current limits

- The gate can only hold the internal agent's tool calls. External ACP agents are post-turn review only.
- Modes declared in Lua or config cannot yet declare a weaker policy for themselves; they take the conservative pre-write default.
- Writes no bracket saw — your editor, background processes — are attributed to nobody; they appear as external hunks, reviewable by eye but not gated or rejectable.
- A root that becomes a git repository after its ledger opened keeps the plain store until a rebase. A root whose repository goes away keeps the git backend the same way. The snapshot id is the record, and it does not follow the disk.
- The archive sweep reads the ledger the daemon holds in memory. A daemon restart before an undecided queue is decided drops that protection, and a later sweep archives the session.
