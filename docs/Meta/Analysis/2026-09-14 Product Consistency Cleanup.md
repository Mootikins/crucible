---
title: Product Consistency Cleanup — 2026-09-14
description: Follow-up to the code reduction review, using the product kiln as the behavior baseline
tags: [meta, architecture, review]
status: implemented
---

# Product Consistency Cleanup — 2026-09-14

Baseline: `5302407f2`, after [[2026-09-14 Code Reduction Review]]. Implemented in
the same isolated `refactor/code-reduction-review` worktree. [[Meta/Product]],
[[Meta/Product Decision Log]], [[Help/Concepts/Note Sync]] and
[[Help/Concepts/Reflection Pass]] supplied the expected behavior; their old
claims were checked against the implementation rather than treated as facts.

## Changes

| Finding | Implementation |
|---|---|
| Browser and agent note writes used independent locks | `fs.write` owns text mutation, root containment, project write policy, base comparison and merge in the daemon. Browser PUT/PATCH, named-note saves and canvas saves forward to it. Agent note create/update/delete take the same per-path lock |
| Outbox read/check/write sequences raced across tabs | Atomic IndexedDB update callback, used for folding, conditional deletion and conflict marking; a revision identifies each replacement |
| A merged drain advanced a dirty buffer onto a base it never incorporated | Landed results identify a merge; a dirty buffer keeps its original base pair, preserving the next save's merge requirement |
| Online reads/saves bypassed queued writing | Reads prefer the outbox; subsequent saves fold into the pending entry. Reopened queued text retains its original base text separately |
| Refused anchored entries skipped their merge fallback | Reconstruct from the saved base and merge before reporting a conflict; the no-base legacy refusal remains explicit |
| Same-path data from different daemons collided | Namespace mirror, outbox, index and attachments by daemon identity; reject cross-daemon folding; import legacy data once without deleting recovery originals |
| Consolidation skipped a session that ended after a newer session advanced the cursor | Persist consumed session ids and event counts. Long-running and resumed sessions remain eligible; tied start times sort by id |
| Retired tool cap left emitter state and branches | Remove the permanently-unlimited cap and counter; keep call-id deduplication and use its set length in tests |
| Availability flag described policy it never controlled | Remove unread `ToolRef.always_available`; the live disclosure logic is unchanged |
| Parser extracted collections nobody consumed | Remove separate callout and footnote collections, extension passes and their diagnostics. Preserve callout block classification, workflow gates, source bytes and LaTeX extraction/counting |
| Product/help claims described older implementations | Reconcile query-help status, summarization, parser behavior, note writes, consolidation progress and reflection's immediate visibility |

The source diff is **850 fewer lines**, including the new regression tests
(`.rs`, `.ts`, `.tsx` and `.luau` under `crates/` and `runtime/`). The web crate
also drops its now-unused `dashmap` dependency.

The write guarantee is intentionally precise: the daemon serializes participating
writers. A base-less legacy write is still a replacement, and a shell command or
external editor does not take this lock. This does not introduce an OS-wide lock
or a CRDT. The web read paths and canvas interpretation are not moved as part of
this write-path consolidation.

Offline identity still uses origin plus config root, as the product decision
specifies; it is not a cryptographic daemon identity. Legacy unlabelled mirrors
are imported only for the previously remembered daemon. Old outbox rows carry
their own identity. Recovery originals remain in IndexedDB and are not imported
again after a queued entry clears; clearing site data removes those copies.

Consolidation's old timestamp cursor cannot identify which older sessions were
still active. Migration re-samples resident ended sessions once. The existing
limitation remains: the plugin lists resident sessions, not all persisted logs.

## Validation

Regression tests first reproduced:

- A refused anchored edit with usable base text being discarded.
- Concurrent queued edits losing one writer's change.
- Cross-daemon writes folding into a mixed entry.
- Reopening queued writing online returning the stale server text.
- A newer online save leaving an older queued write to replay afterwards.
- A merged drain advancing the base of a dirty editor buffer.
- Consolidation skipping an older session that finished later.
- A stale in-flight conflict overwriting a newer queued save.
- The missing daemon write RPC, through two real socket clients.

Additional red-proofs deliberately disabled namespace isolation, split the
IndexedDB update transaction, and removed the agent note tool's shared lock.
Their tests failed; the implementations were restored. The split transaction
retained only one of thirty concurrent updates through two database handles.

The first full run caught two integration mismatches: retrieval-evaluation
phrases still quoted the retired footnote documentation, and the progress map
needed JSON encoding for `cru.storage`'s string-valued API. The fixture now
names the current documented behavior. Persistence tests explicitly require a
string, failed against the unencoded map, and cover timestamp migration.

The offline/editor suite passed 184 tests. Focused parser and shipped-plugin
tests passed. The real-socket test proves two clients' disjoint writes merge on
disk and an unregistered target is refused. HTTP contract tests cover unchanged
bases, stale refusals, clean merges, conflict regions and concurrent PUT/PATCH.
Full `just ci` passed, with the inherited `NO_COLOR` unset for the ANSI-color
assertions: 9,089 workspace tests, 433 feature-gated tests, 2,503 browser unit
tests, 137 browser end-to-end tests, 17 live/served-app checks, and 77
prerequisite-gated tests. Formatting, clippy, types, dead-code checks, licenses,
the documentation kiln and doctests passed too. No snapshots changed.

## Compatibility

No user notes, transcripts or snapshots are deleted. Removed Rust parser types,
fields and diagnostics are source-level API changes; stored markdown is not
rewritten. Footnotes remain an unimplemented rendering feature, now documented
as such. Web callout rendering and block-level retrieval stay live.

The UI work is browser-specific because only the browser has this IndexedDB
outbox. TUI agent note tools benefit from the daemon lock without a new command
or display feature. No TUI review panel or new permission mode is introduced.
