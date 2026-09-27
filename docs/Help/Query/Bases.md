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
Without `--format`, `query` writes a table to a terminal and TSV to a pipe.
JSON follows Obsidian's display-row format. `--format data` returns the full
typed daemon result (date-only values are `YYYY-MM-DD` calendar strings),
including groups, summaries and each note's current `ancestor_hash`; use that hash for `set`. `--this Host.md` sets the host. A stale hash refuses the whole edit.
`set --delete` removes a property. `set` needs a value or `--delete`. A value
parses as JSON when possible, otherwise it is text. The value `null` sets an
empty property. The `--group` value of `create` follows the same rule.

A write prints its outcome as JSON, with a `status` of `applied`, `unchanged`,
`proposed`, `stale` or `refused`. When the status is `stale` or `refused`, the
command prints the outcome to stderr and exits with a non-zero status. The note
does not change.

The terminal surface is `cru base`: the chat TUI does not yet have a note viewer
in which to mount a base. Cards and lists render as tables there, and grouped
views have one table per group.

## Web

Open a `.base` file from Files to select a named view. **Edit source** opens the
YAML editor. Table, cards, list and kanban have native presentations; a custom
view type falls back to a table and keeps its original type and options.

Embed a saved view with `![[Tasks.base#Board]]`, or put the YAML inside a `base`
code fence. `![[Tasks.base]]` shows the first view of the base. Reading view and
live preview render it. `this` refers to the host note for an embed, or the base
file when opened directly. With no host, `this` is null. A file change in a kiln
refreshes the open bases of that kiln once for each burst of changes.

A legacy `kanban/board` block shows the saved base that its `base` parameter
names, or `tickets.base`. The block uses its `view` parameter, or the first
view. It uses the kiln of its `kiln` parameter, or the kiln of the note that
shows it.

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
Formulas can reference each other; cycles are errors. Each formula runs once
per row. List `map`, `filter` and `reduce` use `value`, `index` and `acc`. View
filters combine with global filters; sort, grouping, limits and built-in/custom
summaries run in the daemon.

The daemon parses each expression once, when it loads the base. A filter or a
custom summary that does not parse, or that calls an unknown function or gives
the wrong number of arguments, stops the load. A formula with such an error
loads, and each cell that uses it shows `Error: …`, as Obsidian does. An
unknown summary name stops the load.

An error in one cell does not stop the query. The cell holds an `error` value
that shows as `Error: …`; sorting and grouping treat it as an empty value. A row
whose filter fails is left out. A file that cannot be read or parsed, such as
a note that is not UTF-8, is left out with a warning. An unreadable folder is
also left out.

Operators and functions follow JavaScript where the Obsidian captures do not
decide: `&&` and `||` return an operand (`note.owner || "nobody"`), `<` and `>`
compare two strings by UTF-16 code units and other pairs as numbers, string
positions and `length` count UTF-16 code units, `toFixed` rounds half away from
zero, a negative `split` limit keeps every part, `number()` reads JavaScript
number text, and a regular expression `\d` or `\w` matches only ASCII.
`replace` with a text pattern replaces every occurrence and expands `$&`, `$$`,
`` $` `` and `$'`. `hasTag` ignores case. `file.folder` is `/` at the kiln root.
`max()` and `min()` with no arguments give null. `date()` reads RFC 3339 text,
`YYYY-MM-DD`, and a local date and time with `T` or a space, with or without
seconds (`2024-01-15T14:30`), so it also reads the text of any date value.
Moment's `\` makes the whole next token literal (`\YYYY` gives `YYYY`).

File timestamps and sizes come from current filesystem metadata, not index
update times. A filesystem without creation times reports null for `ctime`.
Obsidian's `.obsidian/types.json` supplies property types when present:
`date` and `datetime` text becomes a date, and `multitext`, `tags` and
`aliases` make one value into a list. Text that does not read as a date stays
text. Without `types.json`, `tags` and `aliases` are list types, as in
Obsidian. Unknown document, property and view options round-trip as YAML values;
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

Queries still scan files. One query resolves each link once, builds the
backlinks once, and hashes an attachment only when the query returns it.
Index/prefilter/cache optimization of the file scan is out of scope for this
compatibility pass.

A query result tells a client how to move rows. Each row has `movable`, which
is true when the view groups by `file.folder`, or by a note property and the
row is a note. Each group has `write_value`, the exact `value` for
`base.set_property` that puts a row into that group; null means delete the
property. A date-only value stays `YYYY-MM-DD`, a date and time stays local
text, and a link stays a wikilink with its path and display text. `options` always holds every view
option with its default. A duration value carries its English `text`.

## Lua and ticket policy

`cru.kiln.query(kiln, options)` uses the same query contract. The writes are
`cru.kiln.set_property`, `create_entry`, `reorder_groups` and `ensure_base`.
Inside a plugin tool, command or session hook they act for the session of that
call, and they refuse a different `options.session`. Outside a session they
need an explicit `options.session`. The daemon checks the attached kiln,
current trust/isolation, containment and the session's card, mode and operator
permissions. A write inside a tool call that the permission gate allowed uses
that call's grant; any other write cannot answer a prompt. Apply mode records
changes in the session review ledger, inside the interval of the enclosing
tool call when there is one. Propose mode records a proposal without changing
disk. `ensure_base` creates a valid `.base` only when nothing holds the path.
Each write answers a `status`: `applied`, `unchanged`, `proposed`, `stale` or
`refused`. See [[Help/Extending/Creating Plugins]] for the fields.

Property writes change only the lines of the one key; comments, quoting, other
keys and the body keep their bytes. A write that changes nothing is not made.
`value = null` writes an empty property; deleting needs `delete = true`.
Saving a group order changes only the view's `groupOrder` key when the views
are a block list. Other layouts rewrite only the `views` key.

A synchronous `base:before_write` hook receives the kiln, final `path`,
`previous_path` and proposed content. Request fields are not forwarded. Note
writes include parsed `properties` and `old_properties`; `previous_path` equals
`path` except during folder moves. Policies see the final properties, including
values inferred from filters or templates. Return nil to permit
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
