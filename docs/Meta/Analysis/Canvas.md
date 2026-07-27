---
tags: [architecture, canvas, knowledge-graph]
---

# Canvas

Crucible reads and writes [JSON Canvas 1.0](https://jsoncanvas.org) — the
`.canvas` format Obsidian uses for its infinite-canvas view. Using Obsidian's
spec rather than inventing one is deliberate: interoperating with an existing
vault is worth more than anything a bespoke format would buy, and the spec is
small enough that full support is cheap.

Related: [[Meta/Analysis/Systems]], [[Help/Wikilinks]]

## The format

`{ nodes: [], edges: [] }`, both optional. **Node order is z-order** — first is
lowest — so anything that filters or reorders nodes must preserve it.

| Node type | Carries |
|-----------|---------|
| `text` | `text` — markdown, stored inline in the canvas |
| `file` | `file` (kiln-relative path), optional `subpath` anchor |
| `link` | `url` |
| `group` | `label`, `background`, `backgroundStyle` |

Edges carry `fromNode`/`toNode`, optional `fromSide`/`toSide`, `fromEnd`/`toEnd`,
`color`, and `label`. **The end defaults are asymmetric**: `fromEnd` defaults to
`none` and `toEnd` to `arrow`, so an edge with neither key set is a one-way
arrow, not a plain line.

Colours are either `#RRGGBB` or the preset *strings* `"1"`–`"6"` (red, orange,
yellow, green, cyan, purple). Writing a preset back as a number makes Obsidian
stop recognising it.

## Forward compatibility is a correctness property

Obsidian and its plugins write keys the spec does not define — Advanced Canvas
stores `styleAttributes`, and per-node zoom breakpoints appear too. A model that
round-trips only spec fields **destroys those on save**: the user opens a canvas
in Crucible, moves one card, and loses styling authored elsewhere.

Every type in `crucible_core::canvas` therefore preserves unknown keys verbatim,
and the round-trip tests are a contract rather than a nicety.

That property caught its own bug during implementation. Modelling a node as a
flattened `kind` enum plus a flattened `extra` map does not work: serde's
flattened map also captures the keys the flattened enum reads, so `type` and
each variant's fields were emitted twice. The document still *parsed*, because
JSON keeps the last duplicate, and a value-equality round-trip test passed.
Only re-parsing our own output failed. Nodes now go through an explicit
`RawNode` wire form.

## Containment

**A canvas must live in a kiln, and may only reference files inside that same
kiln.** Not "any open kiln", and never a project root.

This is stricter than the `project_files` policy governing the web file browser,
which permits reading project files by default. The reasons differ: a file
browser is a browser, but a canvas is portable knowledge. One that reaches
outside its kiln breaks the moment the kiln is copied or synced, and one that
reaches into a project root turns a knowledge document into a reader for source
code.

Three layers, because the UI layer is worth nothing on its own:

1. **UI (advisory)** — drop targets filter to the owning kiln and explain
   rejections. Good UX, zero security value.
2. **Write path (authoritative)** — `PUT /api/canvas` validates every reference
   before anything touches disk, refusing the document and naming the offending
   nodes.
3. **Read path (fail-safe)** — `GET /api/canvas` runs the same check and
   *redacts* failing references from the payload. A client that never receives
   the path cannot request it. Without this, `vim` is a bypass for layers 1
   and 2.

Group `background` images are file references too, and are checked alongside
`file` nodes. Rejections cover `..` traversal, absolute paths, interior NULs,
and symlinks escaping the kiln. A reference to a *deleted* note stays legal — it
renders as a broken card rather than invalidating the document.

## Graph citizenship

Canvases are indexed like notes. A canvas contributes:

- each `file` node as a link to that note
- wikilinks found inside `text` cards

So a note referenced by a canvas shows that canvas in its backlinks. **Obsidian
does not do this** — canvas references never surface as backlinks there, and
text-card wikilinks never do at all.

Canvas links carry no byte spans. `NoteRecord::links` exists so a rename can
splice a new target into source text, which is right for markdown and wrong for
a reference inside a JSON string. Canvas renames go through the typed model
instead; `write_links` already supports span-less callers with negative sentinel
spans that stay resolvable and backlinkable but are never spliced.

`KilnFileKind::of(path)` is the single predicate deciding what the kiln cares
about. It replaced twelve hardcoded `extension == "md"` checks that were already
subtly inconsistent — some accepted `.markdown`, none were case-insensitive.

### Not yet done

Canvas **edges between two file nodes** are a labelled, directional,
hand-authored relation between two notes — signal the wikilink graph structurally
cannot express, and the strongest long-term argument for canvas indexing.
`Canvas::note_relations()` exposes them, but they are not yet stored: `note_links`
is keyed by a single source path, so representing an A→B relation authored by a
third document needs either a dedicated table or a schema change, and `GraphLink`
has no label field. Left for a follow-up rather than forced into the existing
shape.

## Rendering

Nodes are **DOM** inside a transformed layer; edges are **one SVG overlay**. This
matches Obsidian (`.canvas-node` elements, SVG edges) and is the only shape that
can host a live editor inside a note card. A painted 2D canvas draws faster and
is structurally unable to do it.

Two things Obsidian gets partly wrong are built in from the start:

- **Overscan.** Cards stay mounted half a screen beyond the viewport. Obsidian
  remounts at the edge, and the community fix is a CSS snippet that enlarges the
  wrapper so churn happens off-screen.
- **Level of detail on every node type.** Below a third of natural zoom, cards
  become placeholders. Obsidian exempts media, which is exactly why a zoomed-out
  canvas full of images crawls.

Note cards are live views of the real file: edits write back to the source note.
The canvas document stores only the *path*, so note edits never touch the
`.canvas` file and cannot collide with canvas-level undo — they are separate
documents with separate histories, and conflating them would make Ctrl+Z
ambiguous.

Link nodes render as cards, not iframes. An iframe silently contacts a third
party the moment a canvas is opened, which is not a decision the user made by
drawing a box.

## Where the code lives

| Concern | Location |
|---------|----------|
| Document model, round-trip | `crucible-core/src/canvas/mod.rs` |
| Containment rules | `crucible-core/src/canvas/containment.rs` |
| File classification | `crucible-core/src/kiln.rs` |
| Indexing / link extraction | `crucible-daemon/src/pipeline/canvas_index.rs` |
| Rename integrity | `crucible-daemon/src/server/note_refactor.rs` |
| HTTP endpoints | `crucible-web/src/routes/canvas.rs` |
| Viewport maths | `crucible-web/web/src/lib/canvas-viewport.ts` |
| Document mutation + undo | `crucible-web/web/src/lib/canvas-doc.ts` |
| Panel | `crucible-web/web/src/components/canvas/` |
