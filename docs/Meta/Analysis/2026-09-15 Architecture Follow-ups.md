---
title: Architecture Follow-ups — September 2026
description: Six daemon-first follow-ups, their boundary tests and remaining limits
tags:
  - architecture
  - testing
  - review
---

# Architecture follow-ups

Implementation order follows [[Meta/Product]]: establish trustworthy boundary
tests, repair context delivery, expose daemon-owned card selection, prove the
learning loop, make consolidation read persisted history, then exercise offline
failure recovery. The design keeps business decisions in the daemon and reuses
existing stores and protocols.

## Changes and evidence

1. **One web router composition.** Startup and route contract tests use
   `server::build_router`. Tests inject auth state rather than read developer
   credentials. `router_security.rs` exercises authentication across route
   families, hostile Host headers, browser cookies, security headers and the
   terminal's additional policy. Unknown API paths return 404 instead of the SPA.
   Removing authentication makes the security test fail.
2. **Context accepted now, incorporated next turn.** The session slot serializes
   context acceptance with input assembly. A persisted acceptance record anchors
   it after the already-assembled turn, preserving ordering when the broadcast
   writer runs later. One parser projects that record for history and rebuild.
   `agent_manager/tests/context_injection.rs` crosses real Lua bindings into the
   daemon, verifies the in-flight turn is unchanged, later prompts contain one
   copy, and rebuild preserves position. The learning-loop test also checks the
   actual provider request. ACP refuses injection because it owns its history.
   The existing turn-undo test now injects context before the turn: its failing
   restore proved that snapshot keys must be chosen after pending context is
   incorporated, so undo still reaches the captured workspace state.
   See [[Help/Core/Sessions#Adding context from Lua]].
3. **Agent cards through the existing creation RPC.** `cru chat --card NAME`
   reaches interactive and one-shot chat; desktop and mobile web composers expose
   the same optional name. The daemon discovers and resolves it in the final
   session scope. CLI setup does not overwrite the resolved card afterward.
   Existing process and composer tests cover forwarding, daemon persistence and
   rendered initialization; US-909 in [[Meta/TUI User Stories]] covers the TUI.
   There is no second card-discovery service or client-side configuration merge.
4. **One integrated knowledge-reuse test.** `agent_manager/tests/learning_loop.rs`
   drives agent `create_note` calls, reads their disk output, runs the production
   indexing job, and asks fresh sessions questions. Assertions inspect the HTTP
   provider requests, including exclusion of a separately indexed sibling kiln.
   The watcher trigger remains covered separately; this test invokes its indexing
   job directly. `assets/fixtures/learning_loop_v1` reuses the existing
   `cru eval precognition` golden TOML format. Its CI hash embeddings prove data
   flow, not semantic ranking quality; real-provider quality measurements are
   deliberately not claimed.
5. **Consolidation can read across restarts.** Lua session listing, metadata and
   transcript reads now include persisted sessions without reviving them.
   `session_bridge/tests/persisted_history.rs` starts with an empty resident map.
   The shipped consolidation suite reloads the plugin against saved progress,
   retries an interrupted review and excludes plugin sessions. The existing
   progress store was sufficient: no new scheduler or job framework. Processing
   is at-least-once; a crash between note writes and saving progress can repeat
   a review. A real timer-triggered pass and durable cron history remain open.
6. **Offline failures at their real boundaries.** The live browser suite retains
   a successfully committed save whose reply is lost, reloads the real IndexedDB
   queue, introduces a competing disk write, restarts the daemon during drain,
   and retries through production HTTP until the queue is empty.
   It runs on desktop and mobile. The daemon RPC test also retries an original
   write after a fresh server starts, checking both clean merge and conflicting
   refusal without overwriting the other writer. The browser test exposed a
   desktop-only gap: automatic drain belonged to the mobile status badge. Both
   shells now mount that shared control, including the unsent/conflict counts.

## Testing cost and remaining scope

New coverage concentrates on missing boundaries. Card forwarding extends existing
process and UI stories. The small knowledge corpus runs several questions in one
daemon fixture. Restart merge outcomes share a fixture; only the browser failure
sequence adds an expensive live story, run at two viewports. No additional `just`
recipe or parallel test harness was needed.

This closes delivery and visibility gaps, not every adjacent roadmap item:
semantic retrieval quality still needs measured runs with real embeddings,
consolidation has no durable execution history, and offline creation of a new
note remains separate from recovery of edits to an existing note.
