---
title: Attributed Diff Review — Remediation
description: Fix plans for the 17 defects found in the attributed-diff-review implementation, clustered by root cause with cross-cluster arbitration.
tags:
  - meta
  - plan
  - sessions
  - web
status: draft
created: 2026-08-10
---

# Attributed Diff Review — Remediation

> Companion to [[2026-08-09-attributed-diff-review]], which this document
> corrects in five places (see [[#Corrections to the design]]).

The implementation landed across 59 files and compiles, but `just ci` is red:
five tests fail, and adversarial review confirmed 13 defects. Planning added
four more and **removed one** — a defect that was already fixed in the tree.

Plans are clustered by **root cause**, not by finding, so each is internally
coherent. Every claim below was read against the working tree.

## Corrections to the design

The original design doc is wrong in five places. Three of them caused defects
in the implementation, because the implementation did what it was told.

| Location | Claim | Reality |
|---|---|---|
| §1 (line 92-93) | identity uses "an ordinal disambiguator for identical repeats" | **the ordinal IS the defect.** Positional in the current worktree, so removing a sibling renumbers the survivor onto its identity |
| Open Q1 | "the ledger is unaffected — attribution still works for ACP" | **false.** ACP tool calls are never bracketed (`stream.rs:465-482` `continue`s), so every `cru chat -a claude` write is `external` and therefore non-revertible |
| Open Q2 | `git add -A` mutates the real index | **already fixed.** `capture_tree` uses a temp `GIT_INDEX_FILE`; regression test at `workspace_snapshot.rs:479-494`. Delete the entry |
| §3 | `Bash` "may write unattributably" | `Bash` **is** bracketed; only writes landing *after* it returns escape |
| §1 | "fail closed" stated unconditionally | needs its documented exception: a gate query under **no** tracked root must return `false`, or the turn hangs unreleasably |

Open Q2 in particular propagated as live evidence through three hops of
planning before being caught. Fix the doc so it stops.

## Defect inventory

17 total. Severity is post-refutation.

| # | Cluster | Severity | Defect |
|---|---|---|---|
| 1 | identity | critical | `HunkId::derive` omits the root; same relative path in workspace + kiln collides |
| 2 | identity | major | ordinal disambiguator is positional; sibling removal transfers `accepted` |
| 3 | identity | minor | `attribute::side` duplicates `compose::load_side` |
| 4 | gate | major | symlinked workspace never matches; gate fails open for every file |
| 5 | gate | major | rejection recorded permanently; re-applied edit resolves `Rejected` and stops gating |
| 6 | gate | major | `resolve_root` accepts `..` verbatim — stored-path confusion, network-reachable once the bridge lands |
| 7 | gate | major | gate state is broadcast-only; reload while parked and the agent reads as hung |
| 8 | bracket | critical | bracket opens before the gate's unbounded wait; human edits during review attribute to the held call |
| 9 | bracket | major | `close` disarms `Drop` for all roots then awaits per-root; cancel mid-loop leaks the remainder |
| 10 | bracket | major | delegated work gets zero attribution; `ChildLedgerRef` never dereferenced |
| 11 | bracket | critical | ACP tool calls never bracketed at all |
| 12 | bracket | minor | watcher never instantiated |
| 13 | persistence | major | `session_base` never persisted; restart empties the composed diff |
| 14 | persistence | major | base/interval trees are unreachable git objects; `gc --prune=now` deletes them |
| 15 | bridge | critical | five REST endpoints don't exist; `ChangesPanel` ships visible and dead |
| 16 | bridge | minor | `review-api.ts` throws the raw error body instead of `.error.message` |
| 17 | bridge | minor | `review-api.ts` `send()` has no 401 branch; expired cookie never re-prompts |

**Withdrawn:** the "leaked bracket on turn cancellation" escalated from an
implementing lane's notes was **already fixed** — the `Weak<ReviewLedgers>`
`Drop` guard is at `review/mod.rs:52-75` with a passing test. Defect 9 is the
real, much narrower residual.

### One failing test is misspecified — do not make it pass

`review::tests::repro_human_edit_during_open_bracket` cannot be satisfied by
`ReviewLedgers`. Its only evidence is a pair of tree SHAs, and a human write
and a tool write inside one window are **byte-identical evidence**. If it
passed, every genuine tool write would be `external` too. The defect is real;
the test is at the wrong layer. Replace with:

- `review/tests.rs` — `an_edit_that_lands_before_the_bracket_opens_is_external`
- `agent_manager/tests/review_capture.rs` —
  `a_human_edit_during_the_review_gates_hold_is_not_attributed_to_the_held_call`

Note also that `just ci` fail-fast **hid 9 tests**; real counts need
`--no-fail-fast`.

## C1 — Identity

**Replace the ordinal with the hunk's `LineRange` in `session_base`
coordinates.** `session_base` is immutable for the session's lifetime, so base
coordinates do not move under *any* worktree edit — while still separating
identical siblings, which content alone cannot.

`HunkId::derive(root, path, before, after, base) -> Self`, hashing each
length-prefixed field in order. The root is the value already in
`RootBase.root` — **not** re-canonicalized inside `derive`, which would put a
syscall in a pure hash and make identity depend on filesystem state at derive
time rather than at ledger-open time.

Rejected, each with a counterexample:

- **surrounding context** — base `a\nx\na\nx\na\n` gives 1-line contexts `""`,
  `x`, `x`; the last two collide. Unbounded N degenerates to hashing the file.
- **byte offset in base tree** — equivalent strength, needs a second pass; the
  range is already on `RawHunk`.
- **hash of preceding unique hunk** — chains the same defect; the first hunk
  has no predecessor.
- **`base_tree` as root discriminator** — the two-root test builds
  byte-identical repos, so tree SHAs match and the test stays red.
- **shared fate (no disambiguator)** — the only provably transfer-proof
  scheme, and kept as fallback, but it silently accepts an unreviewed
  occurrence and forces surgery on `revert_hunk`.

**Do not add a generation/epoch counter to `HunkId`.** Not because it breaks a
specific test — it does not; the tests that look relevant perform no rejection,
so the counter stays 0 and they pass. The categorical reason: `compose.rs:119`
derives ids with no access to the states map, by design. A counter would thread
mutable review state into a pure derivation, two clients composing the same
worktree would compute different ids, and ids would not survive a resume unless
the counter were itself persisted and totally ordered. That is a different
property, not content-derived identity with an asterisk.

**Files:** `core/session/types/review.rs:82-115` (doc + signature),
`review/compose.rs:113-121` (delete the `seen` map), `:142-154` (delete
`load_side`), `review/git.rs` (+`blob_or_empty` after `:112`),
`review/attribute.rs:154-164` (delete the duplicate).

**Residuals.** A re-alignment in a genuinely ambiguous region (base `x\nx\n` →
`x\n`) can move a hunk's `base_range`, changing its id — which returns it to
the queue, fail-closed. A transfer would additionally require landing exactly
on a removed hunk's coordinates. **Bind mounts are an identity hazard**, not
only a matching one: one repo through two mounts yields two roots and two
identities, so accepting through one path leaves the other unreviewed. Inherent
to anchoring identity on an absolute path; unlike symlinks, which
`--show-toplevel` resolves to one physical path, bind mounts have no single
answer. Log it, do not block.

## C2 — Gate

**Normalization goes at query time only.** Verified against git 2.55:
`git rev-parse --show-toplevel` returns the physical path regardless of `PWD`,
so ingest is already correct and `link`/`real` already dedupe to one root. Only
the query side never normalizes.

| | Spelling | Source |
|---|---|---|
| identity (`derive`, `ComposedHunk.root`) | physical, exactly `RootBase.root` | `git::top_level`, unchanged |
| gate matching | any; translated before comparison | resolved per query |
| stored in `RootBase.root` | physical, written once at `open` | `git::top_level` |

Nothing derived from a resolved query is ever hashed or persisted, so C1's
invariant holds by construction. **Ingest normalization is rejected**: it would
rewrite `RootBase.root`, change the first hashed field of every `HunkId`, and
silently invalidate every decision across a resume — and on Windows,
`canonicalize` emits `\\?\` paths git never prints, *introducing* the mismatch.

New `review/paths.rs` canonicalizes the deepest existing ancestor and
re-attaches the remainder, since a file a tool is about to create does not yet
exist.

**A re-applied rejected edit returns as `Unreviewed` with a derived
`reapplied: bool`.** A recorded `Rejected` on a hunk *present in the composed
diff at all* can mean exactly one thing: the revert landed and the identical
change came back. This splits the two axes the current code conflates — `state`
is the user's decision, `reapplied` is a fact about history. Under every
candidate the hunk is `Unreviewed` and the gate holds, so re-applying costs a
block, not a bypass; the flag is what makes the grind *visible* rather than
letting the user accept out of fatigue.

**Pruning `states` is rejected.** `unreviewed_hunks` → `list_hunks` is on the
gate poller's 500ms `&self` read loop, so pruning there makes a hot read path
irreversibly delete user decisions. The transient-fewer-hunks window is real:
the temp index is seeded from the real one, and the user's own `git stash` /
`checkout` genuinely move the composed diff. The derived flag deletes nothing,
ever. (Dropping the entry on a *successful revert* — a write path unreachable
from the poller — is the labeled fallback.)

**`resolve_root` gets ONE rewrite covering both defects 4 and 6**, on
`review::paths::resolve`. Fail-closed is correct here and is not in tension
with the gate's exception: `resolve_root` refusing produces a loud
`NotAGitRepo` on one RPC; the gate refusing produces an unreleasable hang on a
turn.

**Gate state lives in `ReviewLedgers`, behind a `Drop` guard.** It is already
per-session, already `DashMap`-keyed, already drained by `clear_session`, and
already held by the gate — so no `AgentManager` field and no `session.status`
change. The `Drop` guard is not optional: `hold_for_review` awaits
`sleep(POLL_INTERVAL)`, a yield point, so a cancel or timeout drops the future
*while parked* and never reaches the release event. Unguarded, this converts a
transient invisible-block bug into a permanent visible lie — a chip claiming
"waiting on review" in a session with no turn running. Broadcast-only state is
*accidentally* immune because a dropped future stops emitting; making it
durable removes that accident.

`GateBlock` is **live daemon state, not a persisted event** — it must stay out
of `should_persist`, or a resumed session replays a stale `blocked: true`.

## C3 — Bracket

**Move the open point inward; leave the close point exactly where it is.**
Narrowing and leak-prevention only conflict if narrowing adds close edges. The
handle stays a local in the *caller's* frame, so every early return in the
callee unwinds to the one close site and every cancellation drops the frame,
firing the existing `Drop` guard. Close-edge count stays at one. This is why
the fix is an out-parameter, not a returned tuple — a tuple needs ~10 early
returns rewritten and still would not cover cancellation.

There is no single safe open point: a writer (plugin `pre_tool_call` handlers)
precedes a waiter (the permission gate). So **open after the review gate,
re-baseline after the permission gate but only when it actually prompted.**

Side win: the bracket then sees the *unwrapped* tool name, so
`invoke_tool`→`read_file` stops being bracketed and
`invoke_tool`→`delegate_session` is correctly excluded.

**Defect 9** is inside `close` itself: `mem::take` disarms `Drop` for all roots
before the per-root `capture_tree` await. Consume the handle root-by-root,
deregistering *before* the await — which is the correct interval-end instant
anyway.

**Delegation: keep `delegate_session` excluded from bracketing** (a parent
bracket would overlap every child bracket, marking both contested, and the
whole delegation would go `external`). The missing half is harvesting the
child's intervals into the parent at child cleanup — the child already gets its
own ledger, and `top_level` normalization means a shared workspace yields the
same root `PathBuf` in both.

**The harvest must go through `ReviewLedgers::absorb_child_intervals`, not
`Ledger::push_interval`.** `push_interval` is a method on `Ledger`, so a
harvest written against a `DashMap` guard compiles cleanly and never reaches
the parent's journal — the intervals die on the next restart, reintroducing
defect 13 for delegated work specifically. General invariant, worth a module
doc line: **any write to a `Ledger` not going through a `ReviewLedgers` method
is a persistence bug.**

**The watcher is "the queue is stale", not "the diff lies."** `is_external()`
is `tool_call_ids.is_empty()`, so any write no interval accounts for is
*already* external with no watcher at all. What is genuinely missing is push
notification and self-write suppression. Two daemon writers must take a
suppression window or they re-detect themselves: `revert_hunk` (else rejecting
a hunk reports an external change for the same file and refills the queue) and
`WorkspaceSnapshot::restore` on turn undo.

**Defect 11 (ACP) is a different shape** — bracketing that arm would not fix
it, because the notification arrives around execution the daemon does not
control. The honest fix is a turn-level bracket for owns-history agents. File
as follow-up; correct Open Q1 now.

## C4 — Persistence

**Not SQLite.** `storage/sqlite/` is the note/knowledge-graph store; a sessions
table would be the parallel store to avoid. The pattern is files under
`{kiln}/.crucible/sessions/{id}/`. New sibling `review.jsonl`, append-only,
one JSON object per line, tagged `{"t": …}`, read with the lenient
skip-and-warn parser already used for `session.jsonl`.

`states` and `comments` are **in scope** — persisting `Ledger` alone would
leave decisions evaporating on restart and the cluster would not close the gap
it exists to close.

Appends are eager, at each mutation. Whole-blob rewrite is O(n²) and a crash
mid-write truncates the base record — the one record whose loss is
unrecoverable. Turn-boundary-only loses exactly the turn you most wanted, since
the daemon dies mid-turn far more often than between turns.

**A journal that exists but cannot be read must NEVER fall through to capturing
a fresh base.** That is defect 13 wearing a different hat. Only two things ever
set `session_base`: a session with no journal at all, and an explicit rebase.

**Degradation is graded, because losing attribution fails OPEN**: a lost
`interval` makes hunks `external`, and `unreviewed_hunks` excludes external
hunks, so the gate silently stops blocking.

| Skipped record | Severity | Gate under `PreWrite` |
|---|---|---|
| `header` / `base` | blocking | block all writes |
| `interval` | blocking | block writes under that root |
| `child` / `state` / `comment` | informational | do not block |

Structural failure (root missing, base gc'd) fails closed; **transient** failure
(git invocation failed) keeps today's fail-open. This is only defensible
because a sixth RPC, `review.rebase`, bounds the block. Note that
`hold_for_review`'s doc comment currently argues the opposite explicitly —
*"failing closed here would mean hanging a turn forever on a git error"* — and
needs rewriting, not ignoring.

**Retention:** one tree-valued keep ref per (session, root) under
`refs/crucible/sessions/<id>`, built with `git mktree`. Verified: one such ref
protects every referenced tree transitively through `gc --prune=now
--aggressive`, `git fsck` stays clean, and a *tree*-valued ref is invisible to
`git log --all` where a commit-valued one is not.

**`ChildLedgerRef.node_id` must be `Option<u32>` with `#[serde(default)]`**, not
a defaulted `u32`. `0` is a real conversation-tree node — the root — so a
defaulted `0` renders a delegation confidently under turn 0 instead of turn 7.
That is confident wrong state, worse than the absence it replaces.

An `alg` fingerprint in the header covers both the identity-derivation change
and a `similar` version bump: mismatched `state` rows are excluded from the
active map (fail-closed) but **never deleted from disk**, so a rollback
restores them.

## C5 — Web bridge

**The frontend TS is correct. Keep it.** All nine `ComposedHunk` fields and all
nine `Comment` fields match — names, types, snake_case renames,
`#[serde(transparent)]` newtypes. Zero wire mismatches. `ReviewPolicy` is
already served by the existing `/api/session/{id}/modes` route, which already
applies `degraded_for`, so the frontend genuinely gets the *effective* policy.

**The hole is three layers deep, not one.** `DaemonClient` has no `review_*`
methods, `ReconnectingDaemon` has none, and `crucible-web/src/` contains no
review code at all.

**Writes must not retry.** `call_with_retry` retries on timeout — exactly the
case where the daemon may already have executed. A retried `review.revert_hunk`
reverts once and injects the rejection note into the conversation **twice**, or
answers `UnknownHunk` for a revert that succeeded. At-most-once is the only
correct semantics.

**Security is inherited by construction.** Registering inside
`session_routes_with()` picks up `bearer_auth`, `host_guard`, the CORS
allowlist, the 10MB body limit, and CSP/`nosniff`/`Referrer-Policy`. No new
middleware — per-route layers are how review would drift from the session
group.

CSRF defence is that every state-changing request carries
`Content-Type: application/json`, forcing a preflight the allowlist refuses.
**Do not "optimise away" the `{}` body on resolve** — dropping it drops the
header and turns `POST /review/revert` into a simple request a cross-origin
page could fire blind. Needs a comment at the call site, since it currently
looks redundant.

**`revert_hunk` writing to disk is contained.** `hunk_id` is an opaque
content-derived digest resolved by lookup against the ledger's own composed
diff; there is no caller-supplied path anywhere on the revert path. No
traversal via hunk id is possible.

## Cross-cluster arbitration

| Site | Claimants | Resolution |
|---|---|---|
| `review/mod.rs:336-346` `has_unreviewed_in_file` | C2 (matching logic), C4 (returns `Verdict`) | **land as one change** |
| `compose.rs:119-132` | C1 (derivation call), C2 (`reapplied: false`) | C1 first; C2 rebases one line. C1 also deletes `load_side` at `:142-154` — rebase, don't merge blind |
| `resolve_root` | C2 (symlink), C5 (`..`) | one rewrite, C2 owns it |
| `core/session/types/review.rs` | C1 `82-115`, C2 `134-145`/`271-291`, C3 after `183`/`194`, C4 after `267` | disjoint; sequence writes |
| `handle_review_list_hunks` signature | C4 adds `&Arc<SessionManager>`, C5 depends on it | coordinate before either lands |
| child harvest | C3 harvests, C4 owns journaling | C3 calls `absorb_child_intervals` |

**Landing order:** C3 defect-9 fix (3 lines, independent) → C1 → C3 bracket
narrowing → C2 → C4 → C3 delegation + watcher → C5.

## Open decisions

1. **Passthrough vs typed responses** for the bridge. Recommendation:
   `serde_json::Value` passthrough, matching the rationale already written for
   `list_modes` — zero cross-lane file contention, forward-compatible with a
   `gate` key, shape pinned by contract tests instead of the type system.
2. **Scope.** All 17, or the correctness core (C1 + C2 + C3's bracket fixes)
   with bridge, delegation harvest, and persistence deferred? If deferred,
   `ChangesPanel` must be **unregistered** rather than shipping visible and
   dead.
3. Fsync policy for interval appends (currently flush-only, matching
   `append_event`) — whether attribution must survive a machine crash.
4. Retention window default: 30 days is a guess; git's two weeks is the
   defensible alternative.
5. No out-of-process restart harness exists (`AgentFactoryOverride` is
   in-process only), so a full turn → kill → restart → same-queue test cannot
   be written today. Building that harness is its own work item; the substitute
   is a write-half test against real code plus a read-half test in a real
   process.

## Filed separately — pre-existing defect

**`workspace_snapshot.rs` has the GC bug in shipped code today.**
`snapshot_commit` wraps every captured tree in an orphan commit reachable from
no ref, so every turn-undo snapshot is an unreachable git object.
`git gc --prune=now` in the agent's repo — or any gc once the object passes the
two-week default expiry — deletes it, `WorkspaceSnapshot::restore` fails, and
the undo path logs and continues past. **The user asks to undo a turn, nothing
is undone, and no error is surfaced.**

Mitigating but not exculpating: `SnapshotMap` is in-memory, bounding the window
to a daemon lifetime, and undo usually replays within the grace period.

Fix is the same two calls as C4's — `update_keep` after `snapshot_commit`,
`drop_keep` in `SnapshotMap::clear_session`. It must **not** ride this branch:
separate defect, separate regression test.
