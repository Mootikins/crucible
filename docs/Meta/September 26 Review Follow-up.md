---
title: September 26 Review Follow-up
description: Fixes and regression evidence for proposal decisions and notification attachment
tags: [meta, review, testing]
status: implemented
---

# September 26 Review Follow-up

The September 19–25 review found three defects in the behavior described by
[[Help/Concepts/Review Ledger]] and [[Meta/Web User Stories]]. The original six
regressions failed at behavioral assertions before these fixes.

## Proposal acceptance races

The daemon now reserves each proposal under its short writer mutex before
reading the settlement snapshot. Every competing mutation checks that same
reservation. Accept/resolve keep the reservation through checked file writes
and persistence; cancellation and errors release it. Partial acceptance reserves
both halves before leaving the mutex. Record preflights every proposal it would
supersede before persisting any replacement. Busy returns RPC code `-32009` with
a retryable message; decision clients send once and surface the refusal.

This avoids holding a blocking mutex across an await or waiting while a note
tool holds the file lock acceptance needs. Watcher stale checks skip reserved
proposals. Acceptance announces its final checked result directly instead of
persisting an intermediate Stale state.

Regression coverage pauses at real participating file locks with explicit
future polling: concurrent record/reject, cancellation, partial accept,
supersede preflight, resolve versus reject, and release after write refusal.

## Proposal file identity across kilns

`crucible-core::proposal::ProposalFile` carries the stored root and relative
path. Accept/reject accept `files` selectors through RPC, DaemonClient, HTTP,
generated TypeScript and the web diff pane. Resolve carries the optional root
beside its path. Legacy paths must match exactly one file; missing/ambiguous
selectors and mixed selection formats fail before splitting or writing. Roots
remain subject to daemon admission.

The CLI prints conflict roots and supports `--root` for showing and resolving
one conflict. The TUI is a read-only proposal view using the same projection;
it directs decisions to the CLI. See [[Help/CLI/proposal]].

Real socket tests cover ambiguous accept/reject/resolve and selection of the
second kiln. A real HTTP-to-daemon test exercises all three qualified decisions.
Web component tests assert root/path payloads for duplicate-path rows.

## Notification snapshots versus live dismissal

The shared session stream owns notification hydration and live application.
It opens before reading the snapshot. A temporary per-id map overlays additions
and dismissals received during the read. Generations discard old responses;
reconnects and stream gaps read again, removing missed dismissals. Detach drops
that session's origins and invalidates pending work. Multiple panes share this
owner, while shared notices retain their other session origins.

The real ChatProvider regression delays a snapshot until after a streamed
dismissal. Additional stream tests cover newer additions, shared panes,
reconnection, missed dismissals, old responses, detach, gaps, two sessions and
failed reads. The real daemon/web bridge test verifies that two independent
connections receive the dismissal while an older snapshot still contains the
notice.

## Validation

The original regressions were observed red before implementation. Mutation
checks disable the reservation/root matching and notification reconciliation
to verify the added gates fail, then restore the fixes before validation.
The complete local CI result is reported with the change.
