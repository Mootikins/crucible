---
title: Code Reduction Review — 2026-09-14
description: Completed reduction outcomes and the boundaries deliberately preserved
tags: [meta, architecture, review]
status: complete
---

# Code Reduction Review — 2026-09-14

Reviewed at `53534ceea`; all twelve findings were implemented in the September
review branch. This note records the outcome, not an active deletion queue.
The earlier symbol-by-symbol evidence remains in git history.

The first batch removed 3,587 net Rust/TypeScript source lines, including new
tests, comments and formatting; documentation and snapshots were excluded.
No stored notes or transcripts were deleted and no migration was required.

## What changed

| Area | Outcome |
| --- | --- |
| ACP | Deleted the obsolete tool executor/catalog, formatting facade and unused error vocabulary; retained the live MCP host and streaming translation |
| Watcher | Removed unreachable polling/editor alternatives, selection machinery and unread metrics; retained native grouping, both debounce stages, overflow semantics and capture suppression |
| Session history | Deleted the retired session-to-note indexer; kept retirement guidance, transcript replay/export and storage |
| Storage | Removed the error-only raw-query trait and test-only SQL DTO adapter; retained scoped typed repository operations |
| Retrieval | Shared SQLite vector helpers and the existing embedding-provider factory; retained note/block identity and distinct provider failure policies |
| Web RPC | Converted repeated forwarders to explicit replay-policy rows; mutations are single-attempt after an ambiguous connection failure |
| Browser reachability | Mounted the missing voice provider, tested actual composer/provider wiring, cleaned exports and enabled dead-code lint |
| Inert state | Removed the unwritten processing outcome, unused shell-history store and unconsumed Lua message-panel actions; kept real input recall and notification delivery |

## Why these were safe cuts

Self-tests showed several old implementations worked in isolation, but no
production entry point used them. Useful containment, protocol and behavior
assertions moved to the live paths. Serialized fields, migration readers and
drop guards were not treated as dead because a lexical index found no reads.

The retry change required a lost-reply transport test: an applied mutation must
not be submitted twice, while a replay-safe read may reconnect. Sticky
subscriptions and event delivery remain covered. A generic mocked error cannot
prove that distinction.

The microphone finding was a missing dependency at a visible control, not an
unused feature. Tests cross the composer/provider boundary and preserve the
draft on failure. CSS-aware analysis keeps Tailwind dependencies instead of
deleting them as false positives.

## Validation and follow-up

Focused tests and full `just ci` passed for the batch. Mutation replay,
queue behavior and voice wiring were observed failing without their fixes.
No snapshot change was needed. Historical counts are intentionally omitted;
the current recipes and test reports are the source for suite scope.

[[Meta/Analysis/2026-09-15 Architecture Follow-ups]] records the subsequent
daemon write-path, offline recovery, test-cost and architecture work.
[[Meta/Product]] remains the behavior baseline.

Keep four boundaries explicit in future reductions: parser versus resolver,
the four independent wire bindings, agent permission versus plugin trust, and
live block retrieval versus the retired extraction machinery. Fewer lines are
useful only while ownership and enforcement remain clear.
