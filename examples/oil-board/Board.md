---
title: Board
description: An experimental note with a live Oil view embedded in it
tags:
  - experiment
  - oil
  - views
---

# Board

This note is the experiment. Everything below the next paragraph is ordinary
markdown that you can edit, link and comment on. One block is not: it is live,
drawn from data a plugin owns and publishes.

The board reads the `tickets/` folder beside this file. A ticket is a markdown
note with a `status:` in its frontmatter. Click a card to move it to the next
column — that rewrites one line in one file, and nothing else.

```plugin
kanban/board
{ "kiln": "oil-board", "folder": "tickets" }
```

## What the fence says, and what it does not

The fence names a plugin and a block. It carries no layout:

```text
kanban/board
{ "kiln": "oil-board", "folder": "tickets" }
```

That split is the design. A note that carried the layout would rot the day the
plugin changed. The note says *which* block; the plugin says what the data is;
the frontend says what it looks like *here*.

## Who owns what

The Lua plugin owns the tickets. It publishes `{columns, tickets}` and accepts
one command to move a card. It describes no layout, no colour and no widget —
about 230 lines, and not one of them mentions a pixel or a cell.

The web component owns the appearance. Drag a card between columns; the columns
wrap when the pane is narrow; the colours are the app's theme tokens. All three
of those are things the earlier version of this experiment could not do,
because it shipped a terminal's node tree to a browser and asked it to cope.

A plugin that publishes data and ships no component is not invisible: it
renders as a plain table of what it published. A custom component is an
upgrade, never a prerequisite.

## What happens when you drag a card

1. The component asks the daemon to run the plugin's `kanban_move` command.
2. The plugin rewrites one `status:` line in one markdown file.
3. The plugin republishes the board.
4. The daemon pushes `publication_changed`; every open client re-reads.

The component never applies the move itself. There is one description of the
board and the plugin owns it, so two browsers and a terminal cannot disagree.

## See also

- `runtime/plugins/kanban/` — the plugin
- `docs/Meta/Analysis/The Plugin Contract.md` — the design
- `docs/Meta/Analysis/Oil in Documents.md` — the spike this replaced
