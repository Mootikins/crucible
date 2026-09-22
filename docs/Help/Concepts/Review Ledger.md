---
title: Review Ledger
description: Per-session attributed changes, the composed diff, and the review record of a session
status: implemented
tags:
  - review
  - sessions
  - attribution
  - web
  - agents
---

# Review Ledger

The review ledger answers two questions about an agent session: *what changed*, and *which tool call did it*. Every writing tool call is bracketed by git tree snapshots of the session's roots, and the difference between the session's starting tree and the worktree now — the **composed diff** — becomes the session's review record. The record is read-only: you read it and comment on it.

The evidence is the filesystem, not the agent's claims: changes are keyed on git tree SHAs rather than on what a call reported, so attribution works the same for the internal agent and for external [[Agent Client Protocol|ACP]] agents.

## What gets tracked

When a session's first message is sent, the daemon opens a ledger over the session's workspace and every [[Kilns|kiln]] it is attached to. The daemon snapshots each root once as `session_base`, through one of two backends. A root inside a git repository is normalised to the repository top level and the snapshot is the tree `git write-tree` produced. A root outside one — a kiln outside git is the expected shape — is snapshotted into a plain store under the daemon data root, as a manifest of one content hash per file; a stat key of size, mtime and inode keeps an unchanged file from being read again. The snapshot id says which store holds it, so a session recorded by an older build keeps replaying. Only a root that is not there at all is skipped, and a session with no reachable root has no ledger.

Around each tool call that could write (any tool not known to be read-only), the daemon records the tree before and after. If the trees differ, that becomes an **interval** attributed to that call's `tool_call_id`. A call that wrote nothing produces no interval. Two brackets open on the same root at the same time — typically a parent and a delegated child — are marked *contested*, and contested intervals are excluded from attribution rather than guessed at.

Two kinds of change deliberately get no attribution:

- **Your own edits.** Anything changed outside a bracket surfaces as an *external* hunk — listed so the diff stays honest.
- **Binary files.** Non-UTF-8 files have no line hunks and are skipped.

`bash` is bracketed like every other tool the daemon cannot prove read-only, so what a shell command writes is attributed to that call.

## The composed diff

The review surface is not the stream of intervals — it is the composed diff, `session_base` → worktree, recomputed on demand. Each hunk is a zero-context change cluster.

Attribution intersects the two: an interval's changed lines are projected into the composed diff's coordinates, so one hunk can carry several tool calls, one call can span several hunks, and a call whose work was later overwritten attributes to nothing — the *superseded* signal the web tool cards render. Hunk identity is derived from content and base position, not worktree position, so decisions survive adjacent edits and daemon restarts.

## The review record

The daemon does not accept, reject or revert a hunk. A reject that reverted text on disk could remove the text of another writer, and a gate that held a write raced with the other writers. A note write that needs a decision uses `propose` mode, which makes a proposal and leaves the disk as it is (see [[Help/CLI/proposal|cru proposal]]).

- **Comments** anchor to a line range (changed or not), and can be resolved.
- A hunk still carries a `state`. An old journal can hold decisions, and the listing shows them. Nothing records a new decision.

A listing has a **scope**: `session` lists every hunk since `session_base`; `turn` lists only the hunks a tool call of the current turn touched. The turn starts at the last user message on the conversation's current path, so the daemon needs no marker. Under `turn`, external hunks are not listed, and a session with no turn yet lists nothing.

Every comment emits a `review_changed` event, so open clients refresh without polling.

Structural damage degrades a root: an unreadable `review.jsonl`, a tracked root that is gone, or a base tree lost to `git gc`. A degraded root contributes no hunks, and the listing names it with a reason instead of reporting an empty record.

## Where you meet it

**The web console.** The Changes panel lists the composed diff grouped root → file → hunk. Each hunk expands into a read-only CodeMirror merge view (`HunkMergeView`) that shows the hunk's base text against its worktree text. A hunk row offers a comment and no decision. A **Session / Turn** control chooses the listing scope. The file viewer tones unreviewed, accepted, and external lines inline. On a phone the panel opens from the More menu, and every control is a 44 px target. The panel talks to three session-scoped routes:

```text
GET  /api/session/{id}/review/hunks?scope=session|turn
POST /api/session/{id}/review/comment
POST /api/session/{id}/review/comment/{comment_id}/resolve
```

These forward to the daemon's `review.list_hunks`, `review.comment`, and `review.resolve_comment` RPC methods. `list_hunks` returns the hunks plus `comments`, `degraded` roots, journal `integrity`, and the `scope` it answered under; an unknown scope is refused before the daemon is asked. There is no TUI review panel.

A diffset owns each comment, not a session. `review.comment` and `review.resolve_comment` are aliases of `diff.comment` and `diff.resolve_comment` on the session record of the session. `diff.comments` lists the comments of a diffset. It moves the range of a comment when its quoted text moves, and marks the comment `outdated` when the text is gone. The web routes are `POST /api/diff/comment`, `POST /api/diff/comment/resolve` and `GET /api/diff/comments`.

**A plugin pass's own session.** The [[Reflection Pass|reflection and consolidation passes]] write their kiln notes with `create_note` and `update_note` in a session of their own, in `propose` mode. In that mode a note write goes into a proposal, not into this queue, and the note on disk does not change. The proposal waits in the Inbox until a person accepts, rejects or dismisses it. A pass in a mode that applies its writes still puts a hunk in its own queue. The web sessions list carries a **Reflections** section, on the desktop shell and on the phone, so the transcript of a pass is reachable from either.

**The bundled `review` plugin** (`runtime/plugins/review/`) exposes the same operations as agent-callable tools: `review_list_hunks`, `review_comment`, `review_resolve_comment`. Every tool takes an explicit `session_id` because the session under review is usually not the caller's own: a delegating agent gets `child_session_id` from `delegate_session`'s result and reads the child's diff. Hunk bodies are truncated at 2000 characters — an agent reading a long hunk should open the file.

## Delegation

`delegate_session` is not bracketed — the child session keeps its own ledger over the same roots, and a parent bracket would mark every child interval contested. Instead the parent records a link to the child's ledger, and when the child ends, its intervals over roots the parent also tracks are folded into the parent's ledger. Attribution depth follows session depth: the delegated work shows up in the parent's composed diff, stamped with the child session it came from. See [[Delegation]].

## Lifecycle and persistence

- **Open** — on the session's first send, `session_base` is captured once per root and never recomputed. The backend is chosen here, per root, and recorded in the snapshot id, so every later read of that root goes to the same store. A `rebase` record that an old daemon wrote still moves the base on replay.
- **Journal** — every mutation appends eagerly to `review.jsonl` in the session's storage directory, next to `session.jsonl`. On daemon restart (or when the web panel opens a resumed session), the ledger is replayed from the journal; a fresh base is never silently captured over an existing journal, because that would report the agent changed nothing.
- **Damage is graded** — a journal line that will not parse is skipped and recorded, scoping the loss to one root where possible and to the whole session otherwise. The `rejected` and `undone` records of an old daemon are read and skipped, so an old journal still replays. Decisions are stamped with a fingerprint of the hunk arithmetic; a decision made under different arithmetic returns its hunk to the queue rather than landing on lines you never saw.
- **Retention** — the trees the ledger records are unreferenced git objects, so each session pins them with one tree-valued ref per repository, `refs/crucible/sessions/{session_id}` — invisible to `git log`, dropped when the session is deleted, and swept when a session directory disappears out of band. A plain root's snapshots are claimed the same way, by one keep file per root under the daemon's snapshot store. Nothing outside the daemon collects those, so the daemon sweeps the store itself on the same pass: a snapshot or a blob no live session claims is removed, and a claim is released only by its session going away, never by age. One thing does read the clock: a snapshot written in the last hour is never collected, however unclaimed it looks, and the pass reads it for the blobs it names, so its contents are spared with it. A capture writes its snapshot before the ledger records the call that claims it, and a sweep landing inside an open bracket would take a state no later capture reproduces. `git gc` guards the same window the same way, with `gc.pruneExpire`.
- **End** — session teardown clears the in-memory ledger but leaves the journal on disk; the queue is still there when the session is resumed.
- **Archive** — the auto-archive sweep does not archive a session whose queue still holds an undecided hunk. It holds the session and says so in the log. Archiving takes a session out of the daemon's resident map, and a ledger is restored only for a session that map answers for, so an archived session's hunks would have no door left while the edits they describe are still on disk. A plugin pass is the case this protects: it writes its notes, ends, and nobody opens the queue for days.

## Current limits

- Writes no bracket saw — your editor, background processes — are attributed to nobody; they appear as external hunks.
- A root that becomes a git repository after its ledger opened keeps the plain store. A root whose repository goes away keeps the git backend the same way. The snapshot id is the record, and it does not follow the disk.
- The archive sweep reads the ledger the daemon holds in memory. A daemon restart before an undecided queue is decided drops that protection, and a later sweep archives the session.
