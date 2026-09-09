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

### 4. A quarter of the vocabulary is a terminal artifact

This finding was originally written as "three things do not survive the
crossing". That understated it. Counting the surface:

**Meaningless in a browser** — these carry terminal *state as data*, not merely
terminal looks:

| Primitive | What makes it terminal-bound |
|---|---|
| `popup` | `viewport_offset`, `max_visible`, `selected`, `anchor_col: Option<u16>` — an anchor in **columns**. An nvim pmenu, serialized. A browser listbox owns its own scroll; there is nothing to hand it. |
| `Overlay` | `OverlayAnchor::FromBottom(usize)` is the only anchor: N **rows** up from the frame bottom. `composite_overlays` paints cells. |
| `Raw` | Escape-sequence passthrough sized in `display_width`/`display_height` cells, for the kitty/sixel image protocols. |
| `input` | Carries `cursor: usize` and `focused: bool` because the TUI's key loop drives it. A browser element owns its own cursor. |

`Overlay` and `Raw` are not even exposed to Lua, which is itself the tell.

**Drawable, but specified in the wrong unit:** `divider` (a character repeated
across a width), `progress` (a bar in cells), `spinner` (`frame: usize` indexing
`SPINNER_FRAMES`, clocked by the TUI redraw), and every `padding`, `gap` and
`Size::Fixed`.

**Travel honestly (16 of 22):** `text`, `col`, `row`, `spacer`, `fragment`,
`badge`, `kv`, `bullet_list`, `numbered_list`, `when`, `either`, `each`,
`match_state`, `component`, `markup`, `action`.

**And one absence that matters more than any of the above.** Oil has no
responsive vocabulary at all — no wrap, no breakpoints, no min or max width.
`Size` is `Flex` / `Fixed(cells)` / `Content` and that is the whole of it. A
tree cannot say "stack these when narrow". The TUI has the identical problem at
80 columns and solves it **in the renderer**, never in the tree; a browser
cannot, because the tree is all it receives.

Oil also has no gesture vocabulary and cannot grow one, because a terminal has
no gestures to project. A tap works — `Node::Action` is a button. Drag to
reorder, which is what a kanban board actually is, is unreachable.

None of this is a bug. It is what "one declaration, two renderers" means when
one of the renderers is a terminal.

### 5. The wire shape is now a contract

A serialized tree crosses to another language. `crucible-oil/tests/wire_shape.rs`
pins it: externally tagged, snake_case, defaults omitted, `Empty` as a bare
string. Without that, an enum rename breaks a renderer no Rust test watches.

## What the mobile shell adds

A second web shell for small screens is drafted in `docs/Meta/Architecture/
Mobile Shell.md` on the `worktree-mobile-ui` branch. It is a draft, unbuilt,
and its section 12 engages this spike directly. Four of its points change the
argument here, two in each direction.

**It kills an objection that was never raised but would have been.** A phone
does not need a Lua VM. `plugin.view_render` evaluates Luau on the *daemon* and
serializes a tree; `OilView` fetches and re-fetches and never computes state.
So a phone renders a plugin view with no VM, no WASM and no change at all.

**It does not add a third renderer.** The mobile shell is a separate *shell*,
not a separate *renderer* — it reuses the panel components through `<Dynamic>`.
One `OilNode.tsx` serves both web shells. So Oil buys the TUI, and only the
TUI. The "many renderers" argument inherited from `options/mod.rs` does not
scale the way it first appears to.

**It turns finding 4 from a nuisance into a defect.** On a desktop, "a tree
cannot say stack-when-narrow" is a wart. At 390 px it is a broken view: the
kanban board declares three columns and one fits. And the mobile draft names a
second dimension this spike missed — **a tap target has no size**.
`Node::Action` carries no dimension, the mobile design sets a 44 px touch
floor, and the wire format cannot express it. So the web renderer must impose a
size its author never chose.

**It puts a real cost on the TSX alternative that this analysis did not have.**
Two, both about the origin rather than the language:

1. **The CSP cannot tell app code from plugin code.** `crucible-web`'s
   `server.rs` sets `script-src 'self' 'wasm-unsafe-eval'`. A plugin bundle the
   daemon serves *is* `'self'`. The policy admits it and protects nothing.
2. **The service worker's root scope leans on that same control.**
   `pwa-options.ts` records the residual: root scope is unavoidable, and what
   bounds it is that raw file responses are inert and that `script-src` stays
   same-origin. Plugin JS on the app origin weakens the second bound — and the
   thing it bounds is a cache that survives a reload.

A mitigation exists and the repo already uses its pattern: serve plugin code
into a sandboxed iframe on an opaque origin with a `postMessage` bridge, the
way `/api/file/raw` and canvas web cards already work. That is feasible. It is
also a bridge, a message protocol, a second permission surface and a second
render path. Price it as that, not as "let a plugin ship a component".

**Offline is the one place the split genuinely bites.** `OilView`'s whole state
model is "the daemon answers with the new tree". Offline there is no answer, so
there is no view. The mobile draft's recommendation is to cache the last tree
per `(plugin, view, params)`, draw it read-only, and disable every action with
a visible marker — cheap, and it keeps the render-only contract. It rejects a
WASM Luau runner, and correctly not on CSP grounds (`wasm-unsafe-eval` is
already there for shiki and Whisper) but because `crucible-lua/src/modules.rs`
owns module lookup, lookup is import authority, and a browser VM would be a
second one.

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
new mechanism. A declared tree stays inspectable: the daemon can see what a
plugin is about to draw, which a script tag forecloses. And — the point this
analysis did not have until the mobile draft supplied it — a tree carries no
executable code, so it never touches the CSP or the service worker's root
scope. That is not a small advantage on an origin that caches itself.

**Against Oil.** The vocabulary is a terminal's, and finding 4 is not a list of
missing features but a ceiling: a chart, a drag handle, a text field that
submits, a layout that responds to width — none can be expressed, and there is
no way out. The mobile shell turns that ceiling from a nuisance into a defect,
because a phone meets it on the first view rather than the tenth. And the
"many renderers" premise inherited from `options/mod.rs` is weaker than it
looks: the mobile shell shares the web renderer, so the count is two, not
three, and one of the two is a terminal nobody has yet shown wants this.

## The recommendation

**Oil is the floor. TSX is a declared escape hatch. Neither is the default for
everything.**

Concretely:

- A plugin declares an Oil view. It draws everywhere, terminal included.
- If it *also* ships a TSX component for that view, the web prefers it and the
  TUI keeps the Oil one.
- A view that ships **only** TSX is visibly terminal-less. That is a stated
  capability in the manifest, not a silent hole a user discovers in the TUI.

This is progressive enhancement rather than a fork, and it keeps the cheap case
cheap. "Three proposals pending, tap to review" is squarely inside the sixteen
portable primitives and should never need two implementations. A chart, a drag
handle, a form that submits, or a layout that responds to width is outside them
and always will be.

What this deliberately refuses is the shape where an author picks between two
systems per view with no guidance. That ends with half the plugin ecosystem
invisible in the terminal and no rule saying which half.

### The question underneath, which nobody has answered

The mobile draft states it better than this analysis had:

> Is a plugin view **content inside a note**, or is it **an app surface**?

If it is content, Oil is correct, and an offline or narrow view degrades to a
cached, read-only tree. If it is a surface a user opens on a train to get
something done, a dead view is a hole and only client-side execution closes it.

That is a product bet. Neither this spike nor the mobile draft can settle it,
and every technical argument above is downstream of it.

### What would still settle the technical half

1. **Is a TUI rendering of a plugin view actually wanted?** Nothing here draws
   the kanban board in a terminal. The renderer exists; no TUI surface hosts a
   plugin view. If the answer is no, Oil's entire advantage evaporates — it is
   not a cross-frontend vocabulary, it is the TUI's own renderer, and plugin
   views should be TSX with `cru.plugin.views` withdrawn rather than shipped
   half-used.
2. **What is the second view?** One plugin proves a mechanism. The shape of the
   *next three* — whether they stay inside the sixteen primitives or
   immediately reach for a node that does not exist — is the real evidence.
   Finding 1 is the warning: the first genuinely interactive view already
   needed a new node kind. Each is cheap; the sequence is a second UI
   framework, grown one variant at a time, permanently capped by the terminal
   half.

## Status and what is not done

- No TUI surface hosts a plugin view. The Oil renderer can draw one; nothing
  calls it. **This is the biggest gap, and it is the gap that decides.**
- The editor's live preview does not render the fence (finding 3).
- `oil.input` is read-only in the browser (finding 4).
- No `Web User Stories` entry, no W2/W3 tier. Unit coverage only
  (`components/oil/__tests__/OilNode.test.tsx`, 13 tests over a real captured
  tree).
- `Node::Action` carries no size, so a touch target's 44 px floor has to be
  imposed by the renderer (see the mobile section).
- Offline is unhandled: `OilView` has no cached-tree fallback and no disabled
  state. The mobile draft specifies both.
- Branched from `master`, which predates the `plugin.yaml` deletion on
  `feat/runtime-path-unification`. `runtime/plugins/kanban/plugin.yaml` has to
  go when the two meet.

## Links

- `examples/oil-board/Board.md` — the document
- `runtime/plugins/kanban/` — the plugin
- [[Meta/Product Decision Log]] — the 2025-01-23 Oil DSL row this would close
- `docs/Meta/Architecture/Mobile Shell.md` (draft, on `worktree-mobile-ui`) —
  section 12 answers this analysis and supplies the CSP and offline costs
