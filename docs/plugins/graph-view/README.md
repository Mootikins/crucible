---
title: graph view
description: Example Fennel plugin that inspects a kiln's link graph through cru.kiln
tags:
  - plugins
---

# Graph View Plugin

An example plugin, written in Fennel, that reads a kiln's link graph through
`cru.kiln`. Its purpose is to show how the three graph functions behave and
compose; it is small on purpose.

> **Status.** The tools and the `/graph` command work. The `graph` **view** is
> declared but nothing renders it — spec-table views are parsed and counted and
> that is all, as [[Help/Extending/Creating Plugins]] notes under "Providing
> Views". The view function here is kept pure (it formats a graph it is handed,
> and makes no `cru.kiln` call) so it is ready for a renderer without pretending
> one exists. There is no interactive view, and therefore no keybindings.

## Installation

```bash
cp -r graph-view ~/.config/crucible/plugins/
# or, to scope it to one kiln
cp -r graph-view ~/your-kiln/plugins/
```

## What it demonstrates

```fennel
(cru.kiln.outlinks path)          ; one hop, forward
(cru.kiln.backlinks path)         ; one hop, backward
(cru.kiln.neighbors path depth)   ; `depth` hops, undirected
```

Four properties of those functions drive the whole plugin:

1. **They are async.** Calling one is ordinary Fennel, but only from a context
   the daemon invokes asynchronously — a tool `fn`, a command `fn`, an event
   handler. Not from module top level.
2. **They speak resolved note paths**, in and out: `"Meta/Product.md"`, never
   the raw wikilink target `"Product"`. Unresolved (dangling) links are dropped,
   so every path that comes back can be fed straight into the next call. The
   `kiln.graph` RPC is the surface that reports dangling edges.
3. **Results are sorted and deduplicated**, so plugin output is reproducible.
4. **Results are scope-filtered** to the kiln the plugin is bound to. A path
   outside it is invisible: you get an empty table, which is indistinguishable
   from a note that genuinely has no links. This plugin therefore calls
   `cru.kiln.get` first and reports "no note at ..." separately from "no links".

Because `neighbors` walks the graph itself — undirected, deduped and cycle-safe
— the plugin does no traversal of its own. It only calls `neighbors` once per
depth and diffs consecutive results to get the per-hop rings; if you don't need
that split, a single call is the entire job.

## Tools (for agents)

### `graph_links { note }`

The resolved one-hop neighbourhood of a note.

```json
{
  "note": "Meta/Product.md",
  "outlinks": ["Help/Concepts/Kilns.md"],
  "backlinks": ["Meta/Analysis/Systems.md"],
  "outlink_count": 1,
  "backlink_count": 1
}
```

### `graph_stats { note, depth? }`

How connected a note is, and how much new ground each extra hop covers.
`depth` defaults to the configured `max_depth`.

```json
{
  "note": "Meta/Product.md",
  "depth": 3,
  "outlink_count": 1,
  "backlink_count": 1,
  "reachable": 3,
  "new_notes_by_depth": [
    { "depth": 1, "new_notes": 2 },
    { "depth": 2, "new_notes": 1 },
    { "depth": 3, "new_notes": 0 }
  ]
}
```

Both tools return `{ error = "..." }` for a missing `note` argument or a path
this kiln does not contain.

## Command (for users)

```
/graph Meta/Product.md
```

Prints the note's neighbourhood as a system message:

```
graph: Meta/Product.md
outlinks (1)
  Help/Concepts/Kilns.md
backlinks (1)
  Meta/Analysis/Systems.md
depth 1 (+2)
  Help/Concepts/Kilns.md
  Meta/Analysis/Systems.md
depth 2 (+1)
  Help/Concepts/Delegation.md
```

The argument is a **note path**, not a wikilink and not a title. A command `fn`
receives only its argument table (`{ input = "..." }`, or nil when the user
typed no argument) — there is no context object, so there is no "current note"
to default to, and `/graph` with no argument prints usage.

## Configuration

```toml
[plugins.graph-view]
max_depth = 3
```

The daemon hands that section to the plugin's `setup(cfg)` at load time, always
passing a table even when the section is absent. That is the only configuration
channel: there is no `cru.config.<plugin-name>` table to read, and a `config:`
block in `plugin.yaml` is not part of the manifest schema and is ignored.

## API reference

Implementation and precise semantics: `crates/crucible-lua/src/vault/mod.rs`.

### `cru.kiln.outlinks(path)` — async

Array of resolved note paths that `path` links to. Sorted, deduplicated,
scope-filtered; dangling links omitted. Empty for a path this kiln cannot see.

### `cru.kiln.backlinks(path)` — async

Array of resolved note paths whose links resolve to `path`. Same filtering, so
`outlinks` and `backlinks` are inverses over the visible subgraph.

### `cru.kiln.neighbors(path, depth)` — async

Array of note paths within `depth` hops of `path`. The walk is **undirected** —
a note that merely links *to* `path` is a neighbour. `depth` counts hops
inclusively, so `1` is direct links only and `0` returns nothing; `path` itself
is never in the result, not via a cycle and not via a self-link. Sorted,
deduplicated, scope-filtered.

### `cru.kiln.get(path)` — async

The note record at `path`, or nil. Used here to tell "no such note" apart from
"no links".

### `cru.oil.text(content, style)`

Styled text node. Style keys: `fg`, `bg`, `bold`, `dim`, `italic`, `underline`.
Colors are names (`red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`,
`black`, `gray`) or hex (`#ff0000`).

### `cru.oil.col(opts, ...children)`

Vertical container; `opts` takes `gap`, `padding`, `border`, `justify`, `align`.
Children are nodes, passed as separate arguments — spread a list with
`(table.unpack rows)`. Lua 5.4 has no global `unpack`.

## Fennel notes

The plugin returns a spec table from `init.fnl`; Fennel is compiled to Lua at
load time. Features it leans on: `let`/`local` bindings, `each`/`for`
iteration, `tset` for field assignment, `..` for concatenation, and the `cru`
global for Crucible APIs. See [[Help/Lua/Language Basics]] for the wider Lua
surface and [[Help/Extending/Creating Plugins]] for the spec-table contract.
