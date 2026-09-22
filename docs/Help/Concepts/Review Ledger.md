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

Attribution intersects the two: an interval's changed lines are projected into the composed diff's coordinates, so one hunk can carry several tool calls, one call can span several hunks, and a call whose work was later overwritten attributes to nothing. The intervals are the attribution record. Hunk identity is derived from content and base position, not worktree position, so an id survives adjacent edits and daemon restarts.

## The review record

The daemon does not accept, reject or revert a hunk. A reject that reverted text on disk could remove the text of another writer, and a gate that held a write raced with the other writers. A note write that needs a decision uses `propose` mode, which makes a proposal and leaves the disk as it is (see [[Help/CLI/proposal|cru proposal]]).

- **Comments** anchor to a line range (changed or not), and can be resolved.
- A hunk has no state. The `state` records of an old journal are read and skipped.

The **session record** diffset (`diff.get` with the `session_record` source) is how a client reads the record: each file that differs between `session_base` and the disk, with its line counts, and `diff.file` for the two texts of one file. The daemon has no hunk listing RPC.

Every comment emits a `review_changed` event, so open clients refresh without polling.

Structural damage degrades a root: an unreadable `review.jsonl`, a tracked root that is gone, or a base tree lost to `git gc`. A degraded root contributes no files to the session record and no hunks to the attribution.

## Where you meet it

**The web console.** The Changes panel lists the files of the session record grouped root → file, with the status and the line counts of each file. A file opens the session record in the diff pane, which expands that file and scrolls to it. **Open diff** on an Edit or Write tool card does the same for the file of the call. The panel offers no decision. On a phone the panel opens from the More menu. The panel reads `GET /api/diff?session={id}` and `GET /api/diff/comments?session={id}`, and it resolves a comment with `POST /api/diff/comment/resolve`. There is no TUI review panel.

A diffset owns each comment, not a session. `diff.comment` and `diff.resolve_comment` name the diffset by its source, and the session record is one source. `diff.comments` lists the comments of a diffset. It moves the range of a comment when its quoted text moves, and marks the comment `outdated` when the text is gone. The web routes are `POST /api/diff/comment`, `POST /api/diff/comment/resolve` and `GET /api/diff/comments`.

**A plugin pass's own session.** The [[Reflection Pass|reflection and consolidation passes]] write their kiln notes with `create_note` and `update_note` in a session of their own, in `propose` mode. In that mode a note write goes into a proposal, not into this record, and the note on disk does not change. The proposal waits in the Inbox until a person accepts, rejects or dismisses it. A pass in a mode that applies its writes still puts a hunk in its own record. The web sessions list carries a **Reflections** section, on the desktop shell and on the phone, so the transcript of a pass is reachable from either.

**The bundled `review` plugin** (`runtime/plugins/review/`) gives an agent tools over the change of another session: `review_changes` (the files and their line counts), `review_file` (the two texts of one file), `review_comment`, `review_comments`, `review_resolve_comment`, `review_proposals`, `review_accept_proposal` and `review_reject_proposal`. Every tool takes an explicit `session_id` because the session under review is usually not the caller's own: a delegating agent gets `child_session_id` from `delegate_session`'s result. A tool reads the session record of the child by default. With `branch_root` (and `base`), it reads the branch of a child that works in a worktree. With `proposal_id`, it reads one proposal of a child in `propose` mode. The accept and reject tools decide only on an open proposal of that session, and `paths` decides only those files. The tools call `cru.diff.*` and `cru.proposals.*` (see [[Help/Plugins/Lua Runtime API|Lua Runtime API]]). File texts are truncated at 2000 characters — an agent reading a long file should open the file.

## Delegation

`delegate_session` is not bracketed — the child session keeps its own ledger over the same roots, and a parent bracket would mark every child interval contested. Instead the parent records a link to the child's ledger, and when the child ends, its intervals over roots the parent also tracks are folded into the parent's ledger. Attribution depth follows session depth: the delegated work shows up in the parent's composed diff, stamped with the child session it came from. See [[Delegation]].

## Lifecycle and persistence

- **Open** — on the session's first send, `session_base` is captured once per root and never recomputed. The backend is chosen here, per root, and recorded in the snapshot id, so every later read of that root goes to the same store. A `rebase` record that an old daemon wrote still moves the base on replay.
- **Journal** — every mutation appends eagerly to `review.jsonl` in the session's storage directory, next to `session.jsonl`. On daemon restart (or when a client reads the session record of a resumed session), the ledger is replayed from the journal; a fresh base is never silently captured over an existing journal, because that would report the agent changed nothing.
- **Damage is graded** — a journal line that will not parse is skipped and recorded, scoping the loss to one root where possible and to the whole session otherwise. The `state`, `rejected` and `undone` records of an old daemon are read and skipped, so an old journal still replays. The header records a fingerprint of the hunk arithmetic.
- **Retention** — the trees the ledger records are unreferenced git objects, so each session pins them with one tree-valued ref per repository, `refs/crucible/sessions/{session_id}` — invisible to `git log`, dropped when the session is deleted, and swept when a session directory disappears out of band. A plain root's snapshots are claimed the same way, by one keep file per root under the daemon's snapshot store. Nothing outside the daemon collects those, so the daemon sweeps the store itself on the same pass: a snapshot or a blob no live session claims is removed, and a claim is released only by its session going away, never by age. One thing does read the clock: a snapshot written in the last hour is never collected, however unclaimed it looks, and the pass reads it for the blobs it names, so its contents are spared with it. A capture writes its snapshot before the ledger records the call that claims it, and a sweep landing inside an open bracket would take a state no later capture reproduces. `git gc` guards the same window the same way, with `gc.pruneExpire`.
- **End** — session teardown clears the in-memory ledger but leaves the journal on disk; the record is still there when the session is resumed.
- **Archive** — the auto-archive sweep archives a stale session whatever its record holds. No hunk waits for a decision, so the record does not hold a session back.

## Current limits

- Writes no bracket saw — your editor, background processes — are attributed to nobody; they appear as external hunks.
- A root that becomes a git repository after its ledger opened keeps the plain store. A root whose repository goes away keeps the git backend the same way. The snapshot id is the record, and it does not follow the disk.
- The session record diffset does not name a degraded root. The daemon log names it.
