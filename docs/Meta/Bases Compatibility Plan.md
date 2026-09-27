---
title: Bases Compatibility Plan
description: Compatibility work, evidence and the deferred indexing phase for native Bases.
tags: [meta, plan, bases]
---

# Bases Compatibility Plan

The initial native implementation is `6ae566dca`; the gap inventory is
`ad9b93023`. The follow-up scope covers compatibility, canonical links,
presentation and session-aware plugin operations. **Index optimization is
explicitly deferred.** See [[Help/Query/Bases]] for the user contract.

## Compatibility target and oracle

The target is Obsidian **1.14.2**, English locale, America/Chicago timezone.
The running desktop app, in an isolated disposable vault, produced the
versioned files under `assets/fixtures/bases/`. The capture script invokes its
registered CLI handlers and native table summary evaluation; no Obsidian
implementation source is vendored.

The corpus contains expressions for every declared function plus Unicode,
regex, duration, date, coercion and receiver cases; named queries in five CLI
formats; entry creation and native kanban moves with captured file bytes; and every built-in summary
over all rows, groups and empty sets. Offline gates compare those outputs.
The live regeneration test is ignored with the required desktop/CDP setup
named explicitly. The README records inputs and regeneration commands.

The CLI oracle has two context limitations: it enumerates Markdown, and it
supplies no host for `this`. Native views retain attachments and use the base
file or embedding note as `this`, following Obsidian's documented syntax.
Creation compares paths, frontmatter values and exact body bytes; YAML
indentation and null spelling are serializer choices. JSON display rows are
compatible; `cru base query --format data` retains Crucible's typed API result.

## Canonical links and source identity

SQLite and Bases now share the daemon link resolver's normalization,
exact/title/path-suffix precedence and ambiguity handling, including Unicode
case matching before and after rename. A partial path such as `[[sub/note]]`
resolves by path suffix, and an exact path in the written case wins over a
path that differs only in case. The `note_link_keys` table stores each note's
folded keys, so SQLite reads candidates by key instead of scanning every
non-ASCII note. Bases builds the same keys once per query from fresh file
candidates, without an index prefilter. A bare saved-base reference
can resolve into a subfolder; an ambiguous source is refused. Resolved source
identity is returned with query results and used for subsequent mutations.
Containment remains mandatory after resolution.

Link occurrences retain heading/view fragments. Existing v2 link tables gain
the fragment column and request a relink; fresh tables include it. Regression
coverage includes migration/idempotence and stored fragments. Socket and HTTP
fixtures exercise short saved-base references, named views and host context.

## Expression, creation and summary corrections

Observed differences are pinned by failing reference comparisons and fixes:
UTF-16 surrogate-pair escapes; falsey empty lists; note tag display; file names;
regular-expression lookaround, backreferences and JavaScript replacements;
calendar-month overflow, DST wall-time arithmetic and ambiguous/gap handling,
locale week-year boundaries and typed durations/relative dates; missing/empty
view defaults; property labels; multi-value creation inference; explicit
content precedence; and built-in summary/empty-set behavior.

JavaScript semantics that the Obsidian captures do not reach are pinned by
`js-semantics.json` and `js-reference.json`. Node evaluates the JavaScript
form of each case through `scripts/capture-bases-js-reference.mjs`, and an
offline gate compares Crucible with it. Cases that Obsidian documents
differently, such as `replace` with a text pattern, cite their source.

Regex execution has a backtracking budget and a compiled-pattern size limit.
Expression work, nesting and allocation limits remain. The parser measures
nesting as the evaluator does, so a long left-associative chain such as
`a + b + …` is one level; parse and evaluation accept the same expressions. The corpus is evidence
for the pinned cases, not proof that all ECMAScript regex or Moment locale
behavior is equivalent. Unknown plugin view types are preserved and displayed
with the documented table fallback.

## Native presentation

The query DTO projects card size, image/fit/aspect, kanban width/empty groups,
list markers/indentation/separator and table row/column sizes. A shared typed
cell renderer serves all layouts, including booleans, lists, links, images,
icons and sanitized HTML. Overall and group summaries are shown.

Saved and inline kanban column order can be edited. Inline writes use canonical
parser fence spans and the host note ancestor; surrounding bytes and line
endings survive. Duplicate matching fences and stale hosts are refused.
WS-253 covers component behavior and actual browser layout, including narrow
panels. Screenshots are inspected individually. The CLI is the terminal
surface until a TUI note viewer exists.

## Lua operations and kanban migration

`cru.kiln.query`, `set_property`, `create_entry` and `ensure_base` delegate to
the daemon. Writes require an explicit session id, avoiding reliance on the
shared VM's ambient session slot. Plugin tool callbacks receive their host
invocation context as their second argument.

The daemon checks attached kiln, current provider trust/isolation, filesystem
scope and existing card/mode/operator permissions. It uses the session write
disposition: apply writes enter the persisted review ledger; proposals leave
disk unchanged and use the existing proposal store. An absent file remains
absent on rejection, and an empty file remains empty. Repeated proposed edits
compose, and pending entry names are reserved. Nested plugin edits retain the
enclosing tool call’s review attribution.

`base:before_write` is a synchronous, bounded, fail-closed stage; nil permits
and cancel refuses. `base:changed` is a typed broadcast emitted only after an
applied write. The existing LuaSource registry owns registration and reload
cleanup. Tests cross Lua -> daemon -> filesystem/review and cover stale
ancestors, refusal, timeout, cleanup and proposals.

Kanban initializes `tickets.base` only if absent and uses native queries and
writes. Optional WIP/transition rules use the policy stage. Manual frontmatter
parsing, direct file writes and global board publication are removed; legacy
web embeds route to the native base. General `cru.fs` cleanup is separate.

## Deferred: index optimization

Do not add this work to the compatibility pass. Expressions already parse once
per base load, and a query hashes an attachment only when it returns it.

- Persist filesystem mtime, ctime and size with migration/backfill and watcher
  reconciliation. Index timestamps must never replace filesystem times.
- Add only SQL prefilters proven equivalent to full expression evaluation.
- Scope cache invalidation to affected kilns while retaining gap reconciliation.
- Compare indexed and scanning results over the same corpus, then benchmark
  cold rebuilds and large-kiln latency, work and memory.

The broad `base_hash` to `ancestor_hash` wire/storage rename is also a separate
optional migration. Existing proposals and offline payloads must not be changed
incidentally.

Reference: [Bases syntax](https://obsidian.md/help/bases/syntax),
[functions](https://obsidian.md/help/bases/functions),
[views](https://obsidian.md/help/bases/views), and
[CLI](https://obsidian.md/help/cli).
