---
title: Canvas
description: Infinite-canvas boards in the JSON Canvas format, and how they join the knowledge graph
status: implemented
tags:
  - concept
  - knowledge-graph
---

# Canvas

A **canvas** is a board you arrange notes on: cards, groups, images, embedded
web pages, and labelled arrows between them. Crucible reads and writes
[JSON Canvas 1.0](https://jsoncanvas.org), the `.canvas` format Obsidian uses,
so a board opens in either application without conversion.

Using that spec rather than inventing a format is deliberate. Interoperating
with a vault you already have is worth more than anything a bespoke format would
buy, and a canvas Crucible saves is byte-identical to one Obsidian saves — the
same key order, the same indentation — so the two never fight over a file in
version control.

Canvases live in a kiln alongside notes. Open one from the file tree in the web
UI, or read a published board on this site — [the Canvas Tour](/crucible/canvas/canvas-tour/)
is this documentation kiln's own board, rendered from the same `.canvas` file
the application edits.

## What a card can be

| Card | Holds |
|------|-------|
| **Note** | A live view of a real note. Editing the card edits the file. |
| **Text** | Markdown stored inline in the board, wikilinks included. |
| **Web page** | An embedded page. |
| **Image / media** | Any image, audio, video or PDF in the kiln. |
| **Group** | A labelled backdrop grouping the cards inside it. |

Arrows carry a direction, a colour, and an optional label. An arrow with neither
end specified is a one-way arrow — that asymmetry is in the spec, not a quirk of
this implementation.

## Canvases are part of the graph

A canvas contributes to the knowledge graph the same way a note does. Each note
card becomes a link to that note, and wikilinks written inside text cards count
too — so a note's [[Help/Concepts/The Knowledge Graph|backlinks]] include the
boards that reference it.

Obsidian does neither. Canvas references never appear in backlinks there, and
text-card wikilinks never appear at all. Renaming or moving a note rewrites the
canvases that point at it, so a board does not rot when the kiln is
reorganised.

## References stay inside their kiln

A canvas may only reference files inside the root that owns it — the specific
kiln, or, for a board that lives in a code repository rather than a vault, that
project. A board is portable knowledge: one reaching outside its kiln breaks the
moment the kiln is copied or synced, and one reaching into a project root turns
a knowledge document into a reader for source code.

This is enforced when the board is saved and again when it is read, so editing
the file by hand is not a way around it. A reference to a note that was merely
*deleted* stays legal — it renders as a broken card rather than invalidating the
whole document.

## Embedded web pages

A web card renders the live page. That means opening a board contacts every
third party it references, so the frame is sandboxed into an opaque origin: it
can run scripts and submit forms, but cannot reach Crucible's own session, and
cannot escape the sandbox. Only `http` and `https` addresses can be embedded.

A card ignores the pointer until you select it, which is what keeps an
unselected card draggable.

On this documentation site, web cards are links rather than live frames — a
documentation page that silently contacted every third party a board mentions
would be a worse bargain than the application's, not a better one.

## Related

- [[Help/Concepts/The Knowledge Graph]]
- [[Help/Wikilinks]]
- [[Help/Concepts/Kilns]]
