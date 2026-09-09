---
title: Oil in documents — a spike, and the decision it feeds
description: A working Lua-declared view embedded in a markdown note, and what it settles about Oil versus plugin-authored TSX
type: analysis
status: spike
updated: 2026-09-09
tags:
  - meta
  - plan
  - oil
  - views
  - plugins
---

# Oil in documents

A spike, not a proposal. It exists to answer one question with running code
rather than with argument:

> Can a plugin declare a whole **view** — not a settings form — and have the
> TUI and the web both draw it from that one declaration?

The answer is yes, it now does, and the interesting part is what it cost.

## What runs

An `oil` fence in a markdown note names a plugin and a view. The reading view
fetches the tree and draws it as a live component. Clicking a card dispatches an
action, which rewrites one line of one markdown file, and the view re-renders
from the plugin's own description of the new state.

Verified end to end against a running daemon: `examples/oil-board/Board.md`
renders a three-column board over five ticket files, and a click in the browser
changes `status:` on disk with the body untouched.

## What it took

| Piece | Where | Size |
|---|---|---|
| `Node::Action` — a child made addressable | `crucible-oil/src/node.rs` | one variant, four match arms |
| `oil.action(...)` | `crucible-lua/src/oil.rs` | one binding |
| `cru.plugin.views{...}` and the registry | `crucible-lua/src/views.rs` | ~215 lines |
| `plugin.view_render` / `plugin.view_action` | `crucible-daemon` | one handler |
| `POST /api/plugins/:name/view/:view` | `crucible-web` | one route |
| `OilNode.tsx` — the tag switch | `crucible-web/web` | ~250 lines |
| the `oil` fence and its island mount | `markdown.ts`, `oil/mount.ts` | ~90 lines |
| `kanban` — the plugin under test | `runtime/plugins/kanban/` | ~230 lines of Luau |

## The five findings

### 1. Oil nodes were not addressable, and that is the load-bearing change

Every Oil variant described appearance. Activation lived in `FocusContext`,
registered imperatively by the TUI beside the tree, never on it. A terminal can
do that because the TUI owns the key loop. A browser cannot: the click has to
arrive somewhere.

So a tree that travels has to declare its own activation targets.
`Node::Action` is a wrapper — one variant, not an `action` field on all ten —
so a renderer that knows nothing about actions still draws the child.

**This is the part to argue about.** It puts a behavioural concept into a type
that was purely presentational.

### 2. A view has no session, so it has no kiln

`plugin.view_render` carries a plugin, a view and params, and nothing that says
which kiln is in front of the user. Every kiln-reading plugin leans on
`cru.kiln.active`, which is empty here.

The spike's answer is that the fence names its own kiln. That is honest and it
is also a gap: a note embedded in kiln A should not have to spell out that it
means kiln A. Either the request grows a session, or views stay explicit
forever.

### 3. The web has two markdown surfaces, and this only reached one

The fence renders in the **reading view** (`MarkdownPreview`, an HTML string
with mounted islands). In the editor's **live preview** (CodeMirror,
`live-preview.ts`) it is still a plain code block.

That is the `CLAUDE.md` rule biting exactly where it says it will — "shipping
one is not shipping the other". Mermaid has the same split and answers it with
a settings toggle. A view would need the same, or a reason not to.

### 4. Three things do not survive the crossing

- **Cells are not pixels.** Padding and gap are `u16` terminal cells. The web
  maps one cell to one spacing step. A tree tuned for 80 columns cannot say so.
- **Colours are terminal colours.** The sixteen names map to theme tokens;
  `Indexed` has no browser equivalent and inherits.
- **An input cannot answer.** `oil.input` renders read-only, because only
  `oil.action` crosses back. A form needs a second affordance, or `cru.ui`.

None is a bug. They are what "one declaration, two renderers" means.

### 5. The wire shape is now a contract

A serialized tree crosses to another language. `crucible-oil/tests/wire_shape.rs`
pins it: externally tagged, snake_case, defaults omitted, `Empty` as a bare
string. Without that, an enum rename breaks a renderer no Rust test watches.

## The decision this feeds

The alternative is to let a plugin ship JS/TSX and a stable stylesheet, the way
Obsidian does. `crucible-lua/src/options/mod.rs` already wrote down the case
against, for settings:

> The alternative — an imperative builder like Obsidian's
> `new Setting(el).addToggle(...)` — works only when there is exactly one
> renderer, because the plugin draws the widget itself. Crucible has two, so the
> plugin describes and the frontend draws.

What the spike adds to that argument, in both directions:

**For Oil.** It works, and it was small. The plugin never touched a pixel and
its board draws in a browser. The projection pattern that `PluginSettings.tsx`
proved for one narrow domain generalises to the whole node vocabulary without a
new mechanism. And a declared tree stays inspectable: the daemon can see what a
plugin is about to draw, which a script tag forecloses.

**Against Oil.** The vocabulary is a terminal's. Finding 4 is not a list of
missing features, it is a ceiling — a plugin that wants a chart, a drag handle,
a text field that submits, or a layout that responds to width cannot express it
and cannot escape. Finding 1 shows the pressure: the first genuinely interactive
view already needed a new node kind. The next one will need another. Each is
cheap; the sequence is a second UI framework, grown one variant at a time and
only ever as good as the terminal half allows.

**The honest middle.** These are not exclusive. Oil is the right vocabulary for
what both frontends can draw — a board, a table, a report, a status panel — and
it is the only one the TUI can have at all. TSX would be the escape hatch for
what only a browser can do. The cost of allowing both is that plugin authors
face a choice on every view, and half the ecosystem stops working in the
terminal.

**What would settle it.** Two questions, neither answered by this spike:

1. Is a TUI rendering of a plugin view actually wanted? Nothing here draws the
   kanban board in the terminal — the renderer exists, but no TUI surface hosts
   a plugin view. If the answer is no, Oil's whole advantage is gone and the
   argument collapses to TSX.
2. What is the second view? One plugin proves a mechanism. The shape of the
   *next* three — whether they stay inside the vocabulary or immediately want a
   node it lacks — is the real evidence.

## Status and what is not done

- No TUI surface hosts a plugin view. The Oil renderer can draw one; nothing
  calls it. **This is the biggest gap, and it is the gap that decides.**
- The editor's live preview does not render the fence (finding 3).
- `oil.input` is read-only in the browser (finding 4).
- No `Web User Stories` entry, no W2/W3 tier. Unit coverage only
  (`components/oil/__tests__/OilNode.test.tsx`, 13 tests over a real captured
  tree).
- Branched from `master`, which predates the `plugin.yaml` deletion on
  `feat/runtime-path-unification`. `runtime/plugins/kanban/plugin.yaml` has to
  go when the two meet.

## Links

- `examples/oil-board/Board.md` — the document
- `runtime/plugins/kanban/` — the plugin
- [[Meta/Product Decision Log]] — the 2025-01-23 Oil DSL row this would close
