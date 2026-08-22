---
title: Canvas
description: Why Crucible reads and writes the JSON Canvas format the way it does
tags: [architecture, canvas, knowledge-graph]
---

# Canvas

As-built detail: [[Actual]]

[[Actual]] lists the canvas types, the file-kind predicate and the index adapter. This
note keeps the rationale: why the model keeps unknown keys, why containment has three
layers, and why embedded web pages get no `allow-same-origin`.

Crucible reads and writes [JSON Canvas 1.0](https://jsoncanvas.org), the `.canvas`
format of Obsidian. We use the spec of Obsidian instead of our own format because
interoperation with an existing vault is worth more than a bespoke format. The spec
is small, so full support is cheap.

Related: [[Meta/Analysis/Systems]], [[Help/Wikilinks]]

## The format

`{ nodes: [], edges: [] }`, both optional. **Node order is z-order.** The first node
is the lowest. Code that filters or reorders nodes must keep that order.

| Node type | Carries |
|-----------|---------|
| `text` | `text`, markdown stored inline in the canvas |
| `file` | `file` (kiln-relative path), optional `subpath` anchor |
| `link` | `url` |
| `group` | `label`, `background`, `backgroundStyle` |

Edges carry `fromNode`/`toNode`, optional `fromSide`/`toSide`, `fromEnd`/`toEnd`,
`color` and `label`. **The end defaults are asymmetric.** `fromEnd` defaults to `none`
and `toEnd` to `arrow`. An edge with neither key is a one-way arrow, not a plain line.

A colour is `#RRGGBB` or one of the preset *strings* `"1"` to `"6"` (red, orange,
yellow, green, cyan, purple). If a writer emits a preset as a number, Obsidian no
longer recognises it.

## Forward compatibility is a correctness property

Obsidian and its plugins write keys the spec does not define. Advanced Canvas stores
`styleAttributes`. Per-node zoom breakpoints appear too. A model that round-trips only
spec fields **destroys those keys on save**. The user opens a canvas in Crucible, moves
one card, and loses the styles from another tool.

Every type in `crucible_core::canvas` therefore keeps unknown keys in an `extra` map.
The round-trip tests are a contract, not a nicety.

That property found its own bug. A node as a flattened `kind` enum plus a flattened
`extra` map does not work. The flattened map of serde also captures the keys the
flattened enum reads, so the writer emitted `type` and each variant field twice. The
document still parsed, because JSON keeps the last duplicate, and a value-equality
test passed. Only a re-parse of our own output failed. Nodes now go through an explicit
`RawNode` wire form.

## Containment

**A canvas may reference only files inside the one root that owns it.** Not "any open
kiln". The specific root the canvas lives in.

That root is normally a kiln. A canvas outside every kiln resolves against its
**project** root, because an architecture board that references source files belongs
with the code. A project canvas also obeys the `project_files` policy of that project.
A repository set to `read-only` serves its canvases but refuses to save them. One set
to `off` does not serve them.

Known and deliberate: a canvas at a project root is contained to that project. That
project may contain a kiln, so the canvas can reference the notes of that kiln. The
rule is "the one root that owns it".

This is stricter than the `project_files` policy of the web file browser, which
permits project reads by default. A file browser is a browser. A canvas is portable
knowledge. A canvas that reaches outside its kiln breaks when the user copies the kiln.
A canvas that reaches into a project root turns a knowledge document into a reader for
source code.

Three layers, because the UI layer alone is worth nothing:

1. **UI (advisory).** Drop targets filter to the root that owns the canvas. They
   explain a rejection. Zero security value.
2. **Write path (authoritative).** `PUT /api/canvas` validates every reference before
   it touches disk. It refuses the document and names the bad nodes.
3. **Read path (fail-safe).** `GET /api/canvas` runs the same check and *redacts* a
   failing reference from the payload. A client that never receives the path cannot
   request it. Without this layer, `vim` bypasses layers 1 and 2.

A group `background` image is a file reference too. The check covers it with the
`file` nodes. Rejections cover `..` traversal, absolute paths, interior NULs, and
symlinks that escape the kiln. A reference to a *deleted* note stays legal. It renders
as a broken card. It does not invalidate the document.

## Graph citizenship

The daemon indexes a canvas like a note. A canvas contributes:

- each `file` node as a link to that note;
- each wikilink inside a `text` card.

So a note that a canvas references shows that canvas in its backlinks. **Obsidian does
not do this.** A canvas reference never appears as a backlink there.

A canvas link has no byte spans. `NoteRecord::links` exists so that a rename can splice
a new target into source text. That is right for markdown and wrong for a reference
inside a JSON string. A canvas rename goes through the typed model. `write_links`
accepts a span-less caller with negative sentinel spans. Those links resolve and
backlink but never splice.

`KilnFileKind::of(path)` is the single predicate that decides what the kiln cares
about. It replaced many hardcoded `extension == "md"` checks that disagreed with each
other. Some accepted `.markdown`. None was case-insensitive. Copies lived in the daemon,
the CLI, `crucible-core` and the web frontend, so `Reading List.markdown` was indexed
while `cru stats` reported that it did not exist. Test **A2f** in
`crates/crucible-cli/tests/architecture_tests.rs` keeps the predicate single. It scans
every `crates/*/src/**/*.rs` for a bare-extension comparison. It scans every
`crucible-web/web/src/**/*.{ts,tsx}` for a second copy of the frontend predicate. No
compiler crosses the Rust to TypeScript boundary, so only a test can.

### Not yet done

A canvas **edge between two file nodes** is a labelled, directional, hand-authored
relation between two notes. The wikilink graph cannot express that.
`Canvas::note_relations()` exposes these edges, but nothing stores them. `note_links`
is keyed by one source path. An A to B relation that a third document authors needs a
dedicated table or a schema change. `GraphLink` has no label field. This waits for a
follow-up.

## Rendering

Nodes are **DOM** inside a transformed layer. Edges are **one SVG overlay**. This
matches Obsidian (`.canvas-node` elements, SVG edges). It is the only shape that can
host a live editor inside a note card. A painted 2D canvas draws faster but cannot do
that.

Two things that Obsidian gets partly wrong are built in:

- **Overscan.** Cards stay mounted half a screen beyond the viewport. Obsidian
  remounts at the edge.
- **Level of detail on every node type.** Below about a third of natural zoom, a card
  becomes a placeholder. Obsidian exempts media. That is why a zoomed-out canvas full
  of images is slow there.

A note card is a live view of the real file. Edits write back to the source note. The
canvas document stores only the *path*. A note edit never touches the `.canvas` file,
so it cannot collide with the canvas undo stack. They are separate documents with
separate histories.

## Web pages

A `link` node is part of the spec and always round-trips. For a while it rendered as a
static card, and Crucible could not create one.

Both gaps are closed. The page is **embedded live**, as in Obsidian. A user creates a
card from the toolbar, by a paste of a URL onto the canvas, or by a drag of a link from
the browser.

An embedded page means that a canvas contacts every third party it references. The
sandbox makes that defensible. The frame gets scripts, forms and popups but **not**
`allow-same-origin`. That absence is load-bearing. A link URL is arbitrary document
text, so it can name the origin of this app. The web UI authenticates with a session
cookie. With `allow-same-origin` the frame would be same-origin with the app. It could
read those pages and call the API as the user. It could also drop its own sandbox.
Without it, the frame gets an opaque origin.

Only `http:` and `https:` embed, and only those two can be authored. `mailto:` renders
as a plain card. The UI rejects a dangerous scheme at creation. It never reaches the
document. A scheme that is only rendered inert depends on every future reader to keep
it inert, which is the weaker guarantee.

The frame stays pointer-inert until the user opens the card with a double-click, like
every other card type. An iframe swallows the pointer, so without that gate the user
could not drag, select or marquee the card. Below the LOD threshold no frame mounts.

## Where the code lives

| Concern | Location |
|---------|----------|
| Document model, round-trip | `crucible-core/src/canvas/mod.rs` |
| Containment rules | `crucible-core/src/canvas/containment.rs` |
| File classification | `crucible-core/src/kiln.rs` |
| Index and link extraction | `crucible-daemon/src/pipeline/canvas_index.rs` |
| Rename integrity | `crucible-daemon/src/server/note_refactor.rs` |
| HTTP endpoints | `crucible-web/src/routes/canvas.rs` |
| Viewport maths | `crucible-web/web/src/lib/canvas-viewport.ts` |
| Document mutation and undo | `crucible-web/web/src/lib/canvas-doc.ts` |
| Panel | `crucible-web/web/src/components/canvas/` |
