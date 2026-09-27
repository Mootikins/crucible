---
title: Bases
description: Saved Obsidian Bases queries over kiln files, with native terminal and web views.
tags: [help, query, bases]
---

# Bases

A `.base` file is Obsidian's YAML query and view configuration. Data stays in
notes and their frontmatter. Queries run in the daemon; an unfiltered base
includes attachments as well as notes. Hidden directories and Crucible's
excluded directories are outside the dataset.

```yaml
filters: 'file.inFolder("tickets")'
formulas:
  label: 'if(status.isEmpty(), "Unassigned", status)'
views:
  - type: table
    name: Tasks
    order: [file.name, note.status, formula.label]
  - type: kanban
    name: Board
    groupBy: { property: note.status, direction: ASC }
    groupOrder: [todo, doing, done]
    order: [file.name, note.status]
```

Save this as `Tasks.base` in a registered kiln. The `tickets` folder must exist
before creating an entry there.

## Terminal

```sh
cru base list --kiln Work
cru base views Tasks.base --kiln Work
cru base query Tasks.base --kiln Work --view Board
cru base query Tasks.base --kiln Work --format json
cru base create Tasks.base --kiln Work --name "First task"
cru base set "tickets/First task.md" status '"done"' --kiln Work --ancestor-hash HASH
```

`query` supports `table`, `json`, `csv`, `tsv`, `md`, `paths` and `data`.
JSON follows Obsidian's display-row format. `--format data` returns the full
typed daemon result, including groups, summaries and each note's current
`ancestor_hash`; use that hash for `set`. `--this Host.md` sets the host. A stale hash refuses the whole edit.
`set --delete` removes a property. A value parses as JSON when possible,
otherwise it is text.

The terminal surface is `cru base`: the chat TUI does not yet have a note viewer
in which to mount a base. Cards and lists render as tables there, and grouped
views have one table per group.

## Web

Open a `.base` file from Files to select a named view. **Edit source** opens the
YAML editor. Table, cards, list and kanban have native presentations; a custom
view type falls back to a table and keeps its original type and options.

Embed a saved view with `![[Tasks.base#Board]]`, or put the YAML inside a `base`
code fence. Reading view and live preview render it. `this` refers to the host
note for an embed, or the base file when opened directly. With no host, `this`
is null. Filesystem events invalidate the query cache.

**New item** creates a note from the base and view filters. New notes inherit
literal property comparisons, tag and folder rules and displayed note
properties; `newItemFolder` overrides the inferred folder. `newItemTemplate`
provides initial note text when explicit content is absent. Existing names receive a numeric suffix.

Drag a kanban card between note-property groups to update frontmatter. The row's
ancestor hash protects the edit. Moving to the empty group deletes the property;
a list property receives a replacement list, including a one-item list for a
scalar group. For `file.folder` groups, dragging moves the file through the daemon refactor path. Column headings can be dragged to write `groupOrder`, or reset with **Reset columns**. Inline column edits replace the matching fence using the host note's ancestor hash; ambiguous or stale fences are refused. Body bytes are preserved, including line endings. Writes use the
same daemon lock and writer as the editor. Errors remain visible in the view.

## Expressions and compatibility

The parser accepts Obsidian's property namespaces, arithmetic and boolean
operators, filter trees, formulas, lists, objects and regular expressions.
Formulas can reference each other; cycles are errors. List `map`, `filter` and
`reduce` use `value`, `index` and `acc`. View filters combine with global filters;
sort, grouping, limits and built-in/custom summaries run in the daemon.

File timestamps and sizes come from current filesystem metadata, not index
update times. A filesystem without creation times reports null for `ctime`.
Obsidian's `.obsidian/types.json` supplies date and list property types when
present. Unknown document, property and view options round-trip as YAML values;
comments and original YAML whitespace are not preserved by serialization.

Card size, image fit/aspect ratio, kanban width/empty columns, list markers,
indentation/separators and table row/column sizing follow saved view options.
All layouts share typed value rendering, and group summaries appear with each
group. Bare saved-base references use the daemon's canonical link resolution;
queries return the resolved source identity for subsequent edits. Indexed
embeds retain their heading/view fragments, including after a schema upgrade.

The compatibility target is Obsidian 1.14.2. Versioned reference outputs and a
live regeneration script are in `assets/fixtures/bases/`. Offline tests compare
expressions, query/CLI formats, creation, and native summaries. Date arithmetic
uses calendar month overflow; durations preserve months separately from elapsed
time. Regex supports lookaround, backreferences and JavaScript replacement
strings with a bounded backtracking budget. Formula work, depth and output
allocations remain limited. This is a tested corpus, not a claim that every
possible ECMAScript or Moment expression is equivalent. HTML values are
sanitized before rendering.

Queries still scan files. Index/prefilter/cache optimization is deliberately
out of scope for this compatibility pass.

## Lua and ticket policy

`cru.kiln.query(kiln, options)` uses the same query contract. For writes,
`cru.kiln.set_property`, `create_entry` and `ensure_base` require an explicit
`options.session` id. Plugin tools receive that id in their second argument,
`ctx.session_id`. The daemon checks the attached kiln, current trust/isolation,
containment and the session's card, mode and operator permissions. An
unattended call cannot answer a permission prompt. Apply mode records changes
in the session review ledger; propose mode records a proposal without changing
disk. `ensure_base` creates a valid `.base` only when absent.

A synchronous `base:before_write` hook receives the mutation, kiln, path and
proposed content; property edits also include `old_value`. Return nil to permit
or `{cancel=true, reason="..."}` to refuse. Errors and timeouts refuse the
write. `base:changed` broadcasts only after a change reaches disk. Both hooks
are owned by their registering Lua source and cleared on reload.

The shipped kanban plugin creates `tickets.base` through that API, queries
native Bases, and provides optional WIP/transition policy. It no longer parses
frontmatter, writes files directly or publishes a global board. Human web/CLI
edits use the same Bases policy stage.

The follow-up work and acceptance criteria are in [[Meta/Bases Compatibility Plan]].

Specification: [Bases syntax](https://obsidian.md/help/bases/syntax) and
[Functions](https://obsidian.md/help/bases/functions).
