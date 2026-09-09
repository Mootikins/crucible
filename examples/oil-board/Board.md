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
markdown that you can edit, link and comment on. One block is not: it is a live
view, declared by a plugin in Lua and drawn by the frontend you are reading
this in.

The board reads the `tickets/` folder beside this file. A ticket is a markdown
note with a `status:` in its frontmatter. Click a card to move it to the next
column — that rewrites one line in one file, and nothing else.

```oil
kanban/board
{ "kiln": "oil-board", "folder": "tickets" }
```

## What the fence says, and what it does not

The fence names a plugin and a view. It carries no layout:

```text
kanban/board
{ "kiln": "oil-board", "folder": "tickets" }
```

That split is the whole design. A note that carried the layout would be a note
that rots the day the plugin changes. The note says *which* view; the plugin
says what it looks like; the frontend says how that looks *here*.

## Why not HTML

The other way to do this is to let a plugin ship HTML and JavaScript, the way
Obsidian does. That gives a plugin author everything, and it is the reason to
take it seriously.

It also decides something. `crucible-lua/src/options/mod.rs` already wrote the
argument down, for settings:

> The alternative — an imperative builder like Obsidian's
> `new Setting(el).addToggle(...)` — works only when there is exactly one
> renderer, because the plugin draws the widget itself. Crucible has two, so the
> plugin describes and the frontend draws.

A view is the same claim over a bigger vocabulary. The board above renders in
the browser from a tree that a terminal can also draw, because the plugin never
touched a pixel.

## What it costs

Three things do not survive the crossing, and they are visible in the tickets:

- **Cells are not pixels.** Oil counts padding and gaps in terminal cells.
- **Colours are terminal colours.** The sixteen names map onto theme tokens.
- **An input cannot answer.** Only `oil.action` sends anything back.

None of these is a bug to fix. They are the price of one declaration drawing in
two places, and the question this experiment exists to answer is whether that
price is worth paying — or whether a plugin should get a script tag and a
stable stylesheet instead.

## See also

- `runtime/plugins/kanban/` — the plugin, about 200 lines of Luau
- `docs/Meta/Analysis/Oil in Documents.md` — the decision this feeds
