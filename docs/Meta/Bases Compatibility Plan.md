---
title: Bases Compatibility Plan
description: Remaining compatibility, ownership, performance and plugin work after native Bases support.
tags: [meta, plan, bases]
---

# Bases Compatibility Plan

The initial implementation is `6ae566dca`. It delivers daemon-owned queries,
CLI output, web embeds and native views, entry creation, property edits and
kanban moves. See [[Help/Query/Bases]] for supported behavior. This is a
working implementation, not a completed Obsidian conformance claim.

This plan follows the local scope and research notes dated 2026-09-25 in
`thoughts/`. Those notes are gitignored. The tracked plan below records what
remains without making them a prerequisite for a contributor.

## Evidence and priority

The implementation passed `just ci`: 9,714 Rust tests, 68 gated tests, feature
and documentation checks, 3,464 frontend tests with coverage, 158 UI/story
browser tests, and 120 live/served browser tests (67 skipped). These establish
Crucible's behavior; they do not establish equivalence with Obsidian.

| Priority | Gap | Consequence |
| --- | --- | --- |
| P0 | No live Obsidian golden outputs | Current fixtures cannot detect a consistently wrong interpretation of the spec. |
| P0 | Bases has its own path/stem link resolver | Links and backlinks can disagree with the canonical SQLite link index. |
| P0 | Saved source paths are resolved directly against the kiln | A bare embed naming a base in a subfolder can fail even when that base exists. |
| P1 | Expression semantics are only partly verified | Regex, dates, Unicode, coercion and method edge cases can differ from Obsidian. |
| P1 | Many view options are preserved but not rendered | Loading and saving an option does not mean the view honors it. |
| P1 | Lua query/write bindings and policy lifecycle are absent | The old kanban plugin still runs its separate board implementation. |
| P2 | Every query scans and hashes files | Large kilns and attachments make repeated queries expensive. |

P0/P1 here rank follow-up work; they are not claims of security severity.

## 1. Establish a versioned conformance corpus

Owner: daemon tests and `assets/fixtures/bases/`.

Use a disposable Obsidian vault containing the existing input fixtures. Capture
`base:query` output for named views and every CLI format, recording the exact
Obsidian version, commands, timezone and property-type configuration. The first
attempt to invoke the installed app did not yield usable CLI query output;
no current fixture is a captured Obsidian result.

Add cases for global/view filter composition, `this`, formulas, every supported
function and summary, null and missing values, links, attachments, sort ties,
list-valued groups, limits and custom views. Capture new-entry and drag outcomes
as file bytes as well as query output. Normalize only unavoidable timestamps
and temporary paths; retain value types and ordering.

Done when deterministic offline tests compare Crucible output to captured
results, and an explicitly ignored live regeneration test names Obsidian as
its prerequisite. Deliberately alter one expected value and observe failure.
Pin the compatibility target before extending behavior from a newer app.

## 2. Restore canonical link and embed ownership

Owner: daemon SQLite link index and note pipeline; core only for stored types.

`bases/eval.rs::resolve` currently searches the query's scanned entries by exact
path or unique stem. Replace that separate policy with the canonical link
resolver. Cover title/case matching, duplicate stems, fragments, unresolved
links, file/link equality and backlink direction. Do not teach clients another
resolution rule.

`bases/mod.rs::source_text` currently opens the supplied path directly. Resolve
saved-base references through the same daemon owner, with explicit host context
where required. Return the resolved source identity so query, create and
column-order writes address the same file. Preserve containment after resolution.

Persist an embed's heading/view fragment through link storage; the current web
renderer extracts the view from source text, which does not prove the indexed
representation retains it. Add any required schema migration and rebuild path.

Done when a note can embed a named view in a subfolder through the real HTTP and
socket paths, and links/backlinks agree with the canonical index before and
after rename. Include ambiguous names and out-of-kiln targets. Keep mutations
behind the existing ancestor-hash check and write door.

## 3. Close expression and creation mismatches

Owner: core expression parser/types; daemon evaluator and entry writer.

Confirmed implementation differences:

- Regex uses Rust `regex`, not ECMAScript semantics. Lookaround/backreferences
  and supported flags need a compatibility decision backed by corpus cases.
- Date formatting translates a finite subset of Moment tokens. Calendar month
  offsets exist, but standalone month/year durations use 30/365-day values.
- String escapes use scalar Unicode decoding; UTF-16 surrogate-pair escapes
  need explicit coverage.
- Function names have an exhaustive implementation gate, but that gate does
  not prove all receiver types, overloads or edge cases match Obsidian.
- Expression evaluation depends on selected rows: invalid calls can remain
  undetected in an empty dataset. Separate document validation from evaluation
  if the pinned reference rejects those calls at load time.

Audit against the corpus before labeling the following as defects: missing or
empty `views`, default-view selection, locale/timezone/DST behavior, empty
aggregates, list comparison/grouping, fractional durations, and coercion.
For creation, cover all filter-inference rules, multi-value `containsAll` versus
`containsAny`, property types, template precedence and empty-group overrides.

Done when each confirmed mismatch has a red regression followed by a fix, with
existing depth, work and allocation limits retained. Choose a regex implementation
only after evaluating both compatibility and bounded execution. No silent
translation into a second expression language.

## 4. Honor native view options and typed values

Owner: web presentation; daemon supplies configuration and query results.

`BaseView` currently implements basic layouts. Unknown options round-trip, but
card image/size controls, list formatting and table sizing are not fully
interpreted. Cards/lists mostly stringify values; the table has richer HTML,
image and link cells. Group summaries are returned but not presented alongside
each group. Inline column order still requires editing source text.

Inventory the pinned reference's built-in view options. Add typed configuration
to the query contract for those that affect rendering, then implement them in
the existing native components. Keep plugin view types preserved with an honest
fallback; do not add a second plugin rendering registry.

Done when each implemented option has a user story, component coverage and a
browser assertion on the actual layout. Cover dates, booleans, lists, links,
images, icons and sanitized HTML in each applicable view, narrow layouts,
empty states and surfaced write refusals. Inspect screenshots individually.
The CLI remains the terminal surface until a TUI note viewer exists.

## 5. Add session-aware Lua operations, then migrate kanban

Owner: daemon review/plugin lifecycle, Lua host API, shipped runtime plugin.

Add `cru.kiln.query` and daemon-backed property/entry writes. Define how each
call obtains kiln and session context. Agent/plugin writes must resolve the
session's existing review disposition; do not expose the human direct-write
RPC as a general plugin write bypass.

A before-write policy needs a synchronous stage hook with a correlated result,
timeout and cleanup. After-write notification is a broadcast event. Register
both with their `LuaSource` and prove source cleanup on reload. Publish a
successful change only after the write lands.

Then migrate the kanban plugin: create `tickets.base` only when absent, remove
manual frontmatter parsing and direct file writes, replace the global board
publication with native Bases, and express WIP/transition rules as policy.
Update host declarations, signature projection, Lua API docs and plugin README.

Done when tests cross Lua -> daemon -> filesystem/review, proving apply,
proposal, rejection, stale hashes, policy refusal, timeout and reload cleanup.
Plugin migration depends on this API; it is not required to use native human
Bases views today. General `cru.fs.write/edit` cleanup remains a separate issue.

## 6. Index and optimize without changing answers

Owner: daemon storage, pipeline and query planner.

Persist actual filesystem `mtime`, `ctime` and `size` with migration/backfill
and watcher reconciliation. Index the embed fragment from step 2. Introduce a
conservative SQL prefilter only for predicates proven equivalent to the full
evaluator; unsupported expressions must retain all candidate rows. Preserve
attachments in an unfiltered base.

Parse expressions once per query. Avoid hashing every attachment on every read;
retain a reliable ancestor identity and recheck it on mutation. Scope cache
invalidation to affected kilns while preserving stream-gap reconciliation.

Done when the indexed and scan implementations produce the same corpus results
and representative large-kiln benchmarks measure scan work, latency and memory.
Test create/update/delete/rename, property-type changes and cold rebuilds.
Index time must never substitute for filesystem modification time.

## Delivery order and boundaries

Suggested follow-up commits: conformance fixtures; canonical links and embed
identity; expression/creation fixes in independently testable slices; native
view options; Lua operations and policy lifecycle; kanban migration; indexed
query optimization. Steps 3 and 4 depend on step 1. Plugin migration depends on
step 5's API. Optimization comes after correctness and retains a comparison
path during development.

The broad wire/storage rename from `base_hash` to `ancestor_hash` remains a
separate optional migration. The glossary and new Bases APIs already use
ancestor terminology. Do not change existing stored proposals or offline
outbox payloads incidentally.

Reference: [Obsidian Bases syntax](https://obsidian.md/help/bases/syntax),
[functions](https://obsidian.md/help/bases/functions), and
[views](https://obsidian.md/help/bases/views).
