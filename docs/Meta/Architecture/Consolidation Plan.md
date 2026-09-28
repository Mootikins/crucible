---
title: Consolidation Outcomes and Extension Seams
description: Results of the August consolidation, boundaries to preserve, and current extension entry points
status: historical
tags: [meta, architecture, maintenance]
---

# Consolidation Outcomes and Extension Seams

The August 2026 consolidation is complete. This is not a queue of symbols to
delete. [[Meta/Product]] is the current product baseline;
[[Index]] routes to the maintained architecture notes.

The original per-symbol inventories, conflicting recommendations and completed
checklists remain in git history. They used baseline commit `7053bcfe7`;
treating their line numbers or "no callers" claims as current is unsafe.

## Recorded outcome

- Tiers 1–2: `b31aa0b00` through `5fb48681d`, 266 removals/merges and about
  13,200 net lines removed. `1f7a555ae` resolved final lint findings.
- Tier 3: `2a07d01e4` through `7fcd3b9f4`, 57 implemented entries.
  [[Gaps#6. Status after consolidation, 2026-08-22]]
  retains the historical gap-to-commit mapping.
- Tier 5: `c9e6ddda1` through `443e1c20c`; the later [[ACP and MCP|ACP client]]
  work closed the remaining partial item. Tier-6 observations were review
  leads, not confirmed defects or a maintained backlog.
- September removed the unused block-extraction subsystem and redundant typed
  block lists. This did not retire the live note/block retrieval stores.
  A review note dated 2026-09-14 records the later sweep.
- Late September retired the review gate. `487f95de6` removed the review
  gate, the undo stack and reject-revert. `4edaab5bf` removed the
  `review.list_hunks` RPC and every hunk state. `aaeed9487` moved the
  remaining review RPCs to diffsets and proposals. It kept only
  `cru.session.review_list_hunks`, which backs the Lua reflection pass.
  [[Review]] and [[Daemon Server]] describe the current `diff.*` and
  `proposal.*` RPCs.
- `ad9fe9931` replaced `ReviewPolicy` with `WriteMode`, and `b268fc2c9` with
  `157b4bc13` added the propose write mode. [[Tools and Admission]] and
  [[Session Services]] describe the current write-mode gate.
- `8dc0e9e6b` unified five trust gates and three classification resolvers
  into one `refuse_untrusted` check. `bfa944017` gave session stop one owner.
  `d166c48ca` gave event publication one owner in `EventBus`. `0f92b93a6`
  gave the session journal one writer. `8516360b9` gave the kiln index one
  owner. [[Agent Manager]] and [[Daemon Server]] describe the current
  gates. [[Knowledge Storage and Retrieval]] describes the kiln index.

## Decisions to preserve

- A test-only caller does not make an implementation a product feature.
  Transfer useful behavior assertions onto the surviving entry point before
  deleting an obsolete implementation and its self-tests.
- [[Parser|Parser byte spans]], link resolution, kiln identity and
  [[Knowledge Storage and Retrieval|retrieval]] are separate responsibilities.
  Similar records across these seams are not automatically duplicates.
  `BlockHash` and `ContextMessage` have canonical owners. See
  [[Core Domain Types]] for their current definitions.
- Wire compatibility is not determined by Rust call counts. Serialized fields,
  migration readers, feature-specific integrations and drop guards need their
  own evidence before deletion.
- Bash permission layers have distinct scopes and overrides; the order is
  documented in a working note outside this kiln. Do not merge them from
  similar names. [[Tools and Admission]] covers the current containment and
  classification code.
- Required trait methods expose incomplete adapters at compilation. Test
  doubles and dependency firewalls are legitimate traits; zero-call wrappers
  and test-only alternative implementations are not.
- Byte, character and terminal-width truncation are different contracts.
  See [[Oil Renderer]] and [[TUI Components]] for the current truncation
  helpers. Layout nodes and renderer inputs may likewise retain intentional
  differences.
- Preserve behaviorally distinct failure policies when sharing helpers.
  A missing optional embedding provider is not the same as a failed provider.
  See [[Providers and LLM]] and [[Knowledge Storage and Retrieval]] for the
  current embedding-provider seam.
- Prefer an exhaustive enumerated table and a runtime-derived gate to a grep
  that can satisfy itself from stale test literals.

## Extension seams

These are entry points, not exhaustive edit lists. Follow the types and running
tests; do not copy the old plan's removed modules or static tool catalogs.

| Change | Start here | Required proof |
| --- | --- | --- |
| Builtin tool | `crates/crucible-daemon/src/tools/surface.rs`, then its executor ([[Tools and Admission]]) | Exhaustive surface classification, containment/permission behavior, actual advertised tool set |
| Provider | `crates/crucible-core/src/config/components/backend.rs` ([[Core Config]]), daemon factory ([[Providers and LLM]]), embeddings if supported | Config resolution and a request through the provider boundary |
| Client | `SessionEventMessage`, required `AgentHandle`/`SessionKnobs` including the plugin-approval and turn-limit knobs, client projection ([[Session Services]], [[RPC Client]]) | A real session round-trip, the `turn_finished` (`TurnStatus`) ending contract, and rendered unfamiliar data |
| Hook | `crates/crucible-lua/src/handlers/hook_name.rs` ([[Luau Host]]), the lifecycle/turn call site ([[Agent Manager]]) | Correct synchronous stage versus broadcast contract, owner attribution, interception ordering |
| Storage backend | Core note/block/property storage traits, daemon storage implementation ([[Knowledge Storage and Retrieval]]) | Link queries and scoped retrieval, not only CRUD self-tests |
| RPC | `crates/crucible-daemon/src/rpc/dispatch.rs`, handler and canonical request type ([[Daemon Server]], [[RPC Client]]) | Actual request/response, error and session-lifecycle behavior |

A session setting must survive get/set and resume and reach both clients
([[Session Services]]). A new Lua callable must exist in the running VM and
in its checked declaration ([[Luau Host]]). A new wire shape needs producer
and renderer tests. For modes and plan behavior, the shipped
`runtime/defaults/init.luau` is authoritative, not a Rust fallback.

## How to use an old review lead

Reproduce it on the current tree before promoting it to work. The old follow-up
lists included already-deleted watcher backends, already-wired notifications,
and test gaps closed by later changes. Current work belongs in
[[Meta/Product]] or a focused design note with evidence, not another tier of
an indefinitely growing cleanup plan.

Use the repository's current `just` recipes. Scope iteration tests, run
`just ci` before committing, red-proof behavior gates, and review every changed
snapshot. Historic test counts and warm-cache timings are not current budgets.
