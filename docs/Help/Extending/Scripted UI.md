---
title: Scripted UI
description: Theme the TUI, style its surfaces, and build statuslines from Lua
status: implemented
tags:
  - scripting
  - ui
  - theme
  - statusline
  - lua
---

# Scripted UI

Crucible's terminal UI is configured from Lua: colours, per-surface geometry, and
statusline layout.

> **Where this runs.** The Lua VM lives in the daemon, not in the `cru` process.
> Your `init.luau` is evaluated once, daemon-side, and the result is delivered
> to every attached client over the `ui.config` RPC as data. That is why styling
> is declarative, and why statusline values are *pushed* rather than computed
> per frame — see [[#Statusline]].
>
> **This page is about styling, not about asking.** An earlier version
> documented `cru.popup` and `cru.panel` modules for building interactive
> popups; those were never registered in a running daemon and are gone.
> `cru.ui` is not: it IS registered on the plugin VM, and it is how a handler
> asks the user something. Its variants, and its `opts.timeout` — SECONDS,
> default 300, where `0` means the default rather than "give up at once" — are
> documented in [[Help/Plugins/Lua Runtime API]]. Asking an LLM is a separate
> question and is unimplemented (see [[Help/Extending/Script Agent Queries]]).

## Three things, three names

"Theme" used to mean three unrelated things here. They are now named separately:

| Name | Covers |
|---|---|
| `cru.colorscheme` | the colour palette, and what highlight groups resolve against |
| `cru.geometry` | surface geometry, prompt glyphs, layout |
| `cru.syntax` | code highlighting inside fenced blocks |

A fourth namespace on this page is **not** theming, and shares a word with the
first three by accident: `cru.surface` declares a panel of rows for every client
to draw. Styling decides how the chrome looks; a plugin surface is content. See
[[#Plugin surfaces]].

## Colorscheme

A colorscheme is a palette. Define one inline, or drop a file in
`~/.config/crucible/themes/` and switch with `ui.set_theme`.

```lua
cru.colorscheme.setup{
  name = "my-theme",
  is_dark = true,
  colors = {
    primary    = "cyan",
    background = "#282c34",
    popup_bg   = "#282c34",
    -- adaptive: the client picks using its own terminal background
    text       = { dark = "#ffffff", light = "#1a1a1a" },
  },
}
```

### Colour values

| Form | Example | Meaning |
|---|---|---|
| slot | `term4`, `4` | terminal palette entry 4 — whatever the user put there |
| name | `"blue"`, `"bright_magenta"` | an alias for a slot (see below) |
| hex | `"#282c34"` | a literal colour |
| adaptive | `{ dark = …, light = … }` | resolved by the client, per terminal background |
| `"none"` | | the terminal's own default |

**The names are slot aliases, not colour promises.** `"blue"` is exactly
`term4`, and plenty of terminal themes put something other than blue in slot 4.
When a theme means "whatever the user calls blue", `term4` says so honestly;
when it means blue specifically, use hex.

Slots run 0-15. Indices 16-255 are the fixed xterm cube — no terminal palette to
defer to, so they render the same everywhere.

Adaptive pairs cross the wire **unresolved** — the daemon cannot know which
terminal a client is attached to, so the client resolves them.

## Highlight groups

Groups are an open namespace: define your own, and link one to another.

```lua
cru.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })
cru.hl.link("PopupSelected", "Visual")
```

A colour is a literal, an adaptive pair, or a **palette reference** — the name of
a field in the active theme (`"mode_normal"` above). References resolve at use
time, so swapping the palette moves every group that references it.

`link` is a base to override, not a rename: attributes set on the linking group
beat the target, so you can say "like `Visual`, but red".

## Surface geometry

> **Two meanings of "surface", and they are not interchangeable.** This section is
> about the *chrome* a renderer draws — a popup, a modal, a drawer. A **plugin
> surface** is a different thing: a panel of rows a plugin declares, described
> under [[#Plugin surfaces]] below. Geometry is closed because the renderer must
> know each one; a plugin surface is open because the plugin supplies only data.

Geometry is a closed set — the renderer has to know how to draw each surface.

```lua
cru.geometry.setup{
  popup  = { border = "rounded", padding = 1, max_visible = 10 },
  modal  = { border = "double", padding = 1 },
  drawer = { border = { "", "▀", "", "", "", "▄", "", "" } },
  prompt = {
    normal  = { glyph = "❯ " },
    command = { glyph = ": " },
    shell   = { glyph = "! " },
  },
}
```

Surfaces: `popup`, `modal`, `drawer`, `toast`, `statusline`, `prompt`. Every
field is optional, and omitting one keeps the built-in — a surface you do not
mention is untouched, not blanked.

### Borders

A preset name (`"none"`, `"single"`, `"double"`, `"rounded"`, `"heavy"`) or a
list of eight characters clockwise from the top-left, matching Neovim's
`nvim_open_win`:

```lua
border = { "─" }                              -- one char fills all eight
border = { "+", "-" }                         -- corners then edges
border = { "╔","═","╗","║","╝","═","╚","║" }  -- tl, top, tr, r, br, bottom, bl, l
border = { "", "▀", "", "", "", "▄", "", "" } -- rules above and below, no sides
```

A shorter list repeats. An empty string means that edge is **absent and occupies
no cell** — distinct from `" "`, a blank edge that still takes one.

## Syntax highlighting

By default code blocks derive their colours from the colorscheme, so a fenced
block does not clash with the chat around it:

```lua
cru.syntax.setup{
  theme  = "derived",        -- the default; or any syntect theme by name
  colors = {                 -- override individual scopes
    keyword = "#c678dd",
    string  = "success",     -- palette reference
    comment = "term8",       -- terminal slot
  },
}
```

`:set syntax_theme=monokai` switches at runtime; `:set syntax_theme=derived`
goes back to following the colorscheme.

The derivation maps keyword←`primary`, string←`success`, comment←`text_dim`,
number←`warning`, type←`info`, function←`secondary`. That is conventional, not
authoritative — override any slot that reads wrong.

Terminal slots survive into code blocks, even though syntect's own colour type
is RGB-only, so a colorscheme written against terminal colours applies to code
as well as to chrome.

## Plugin surfaces

A **surface** in this sense is a panel a plugin declares and every client draws:
a session list, a review queue, a file tree. The plugin states *what it has*;
neither client is told *how* to draw it.

```lua
cru.surface.declare{
  plugin = "session-board",
  name   = "sessions",
  title  = "Sessions",
  shape  = "list",
}

cru.surface.set_rows{
  plugin = "session-board",
  name   = "sessions",
  rows = {
    { id = "s1", text = "crucible", mark = "busy" },
    { id = "s2", text = "web-fix",  mark = "blocked", detail = "waiting" },
  },
}
```

A user opens it with `:surfaces` in the TUI, or the **Surfaces** panel in the
browser. `runtime/plugins/session-board/` is the reference implementation.

### A row is data, never presentation

A row carries `id`, `text`, an optional `detail`, and an optional `mark` from a
closed set: `busy`, `blocked`, `ok`, `failed`. **The client picks the glyph.** The
TUI draws `●`, `⏸`, `○`, `✗`; the browser draws a coloured dot. A plugin that
shipped the character would bind one client's medium into a contract both have to
honour, and the browser cannot afford a cell grid — it would forfeit screen-reader
roles, real text inputs, find-and-select and reflow.

A mark this build does not know renders blank rather than as a placeholder: a
client that cannot name a status should say nothing about it, not assert a fault.

`id` is the row's stable identity. A refresh keeps the reader's cursor on the same
`id`, so a row arriving above it does not move the selection.

### `declare` is idempotent; `set_rows` is the update

A reload re-runs your `init.luau`, so `declare` runs again on a live surface. It
keeps the existing rows and version and refreshes only the title, shape and
session — which is what lets a panel a user has open survive a reload. Surfaces
are keyed by `(plugin, name)`, never by a generated id, so the same declaration
lands on the same surface every time.

`declare` announces the surface, so a panel you declare and do not fill yet is
still something a client can list and open. A re-declare announces only when the
title, shape or session really moved: a reload re-declares every surface, and an
announcement per surface per reload costs every client a redraw to learn nothing.
The version does not move for a retitle — it counts row generations, so shifting
it would tell a client the rows changed when they did not.

A plugin that goes **inert** loses its surfaces instead: an uninstall, or a
reload that failed. The daemon withdraws each one and marks the announcement
`withdrawn`, so a client drops the panel straight off the event. It does not have
to ask for the surface and read the empty answer — that costs a round trip, and
an empty answer is also what a lost race looks like.

### Staying current

`declare` and `set_rows` both broadcast `surface_changed`, and both clients
refetch. The event carries the identity and the new version and **never the
rows**, because a surface is unbounded where an event is not.

A withdrawal is the one event a client acts on without refetching. It sets
`withdrawn`, which says the surface is gone and there is nothing left to ask for.
The field is absent when it is false, so an ordinary change looks exactly as it
always did.

Redraw from the daemon-wide session hooks, which fire for every session rather
than only the one a handler runs in:

```lua
cru.on("session:created", refresh)
cru.on("session:ended", refresh)
```

A `surface_changed` refreshes a panel a user has open. It will never *open* one:
a plugin pushes rows at a moment the user did not choose, and a full-screen panel
over someone's typing is not acceptable.

### Caps, and what is stripped

Titles and every row string are sanitised and length-capped by the daemon, not by
a renderer — a row reaches a terminal that parses ANSI out of plain strings, so an
escape in a row would be an injection. Row count is capped too. A plugin that
pushes more is truncated rather than refused: losing the tail of a list beats
losing the list.

### Limits worth knowing

- `shape` accepts `list` today. A shape arrives *with* its renderer, never ahead
  of one, so a declaration naming an undrawable shape is refused at the call.
- There is no action verb yet: a row cannot be clicked into a behaviour. That
  needs per-client addressing, because an action fires against a surface and two
  clients must not run each other's keypresses.
- `cru.session.list()` is **singular**, and it returns a value *and* an error.
  Read the error half; `ipairs` on a failed call raises, which turns a background
  refresh into a broken panel.

## Statusline

The screen is three ordered lists. Position in a list is the arrangement, and
the input is an element like any other:

```lua
local sl = cru.statusline

sl.setup{
  prompt = {
    sl.input,
    { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
      sl.align,
      sl.any(sl.notification, sl.context) },
    { sl.when("streaming", sl.cache) },
  },
}
```

Regions: `top`, `prompt`, `bottom`. A region holds as many rows as you write, so
a prompt area can be several rows deep. **There is no ordering field** — a row
renders where you put it, which is also why `sl.input` being an element is what
lets you place rows above or below the editor.

Only `prompt` may contain `sl.input`; one placed elsewhere is dropped, since two
inputs would mean two editors. A region you do not mention keeps its built-in,
so a `setup{}` naming only `top` will not blank the place you type. A key that
is not a region places nothing and says so in the log — worth knowing, because
bars used to be named (`main = {...}`) and that spelling still reads as valid.

Rows above the input push it down whenever they render, and that space is also
where completion popups open — so unless you want the editor to move, prefer
putting context rows below it. The popup works out how far to sit from the
bottom by measuring the prompt region, so extra rows and a wrapped multi-line
input both move it correctly.

The shipped default lives in `runtime/statusline/default.lua` and is written in
exactly this vocabulary — it is the real default the daemon evaluates, not an
illustration, so it is the best starting point for your own.

Built-in items — `mode`, `model`, `context`, `cache`, `status`, `notification` —
are evaluated by the TUI every frame and cost no RPC. Bare strings are literal
text. `sl.align` splits the bar; one gives left/right, two give left/centre/right.

### Conditionals

Lua's `or` will not work here — item objects are truthy, so `a or b` always takes
the first, and the branch has to survive being sent to the client. Use
combinators:

- `sl.any(a, b, …)` — the first that renders something
- `sl.when(cond, item)` — render only when `cond` holds

Conditions are facts only the TUI knows: `"streaming"`, `"has_notification"`,
`"mode:plan"`. Lua places them; the TUI answers them.

### Custom values

Anything the daemon has to compute — a git branch, a queue depth — is an
expression. Place it in a bar, then supply it from a handler:

```lua
local sl = cru.statusline

sl.setup{
  prompt = {
    sl.input,
    { sl.mode, sl.align, sl.expr("git"):hl("Git") },
  },
}

cru.on("FileChanged", function(ctx)
  local out = cru.shell.exec("git status -b --porcelain")
  cru.statusline.set(ctx.session_id, "git", parse_branch(out))
end)
```

`FileChanged` fires when the workspace changes, which is the trigger a value like
git status actually needs — turn boundaries are the wrong moment, since files
change while you are not in a turn.

An unset expression renders nothing, so a bar does not jump when a value first
arrives. Re-setting an unchanged value is reported as `unchanged` and costs no
repaint.

Values are **text**, not escape sequences. Styling goes on the item
(`:hl("Git")`), which is what lets Crucible strip control characters from a value
unconditionally — a branch name should not be able to move your cursor.

Limits: 256 characters per value, 16 expressions per session. The character limit
is a safety bound, not layout; display truncation is the TUI's job, because only
the TUI knows the terminal width.

## Failure behaviour

Styling is not a gate, so nothing here fails closed — but "fail open" means
falling back to a *coherent whole*, never a partial one. A malformed theme yields
the complete built-in rather than a half-applied palette, because `bg` without
`fg` is invisible text. An unknown palette name drops that one attribute rather
than substituting a guess. A layout that places no input is refused whole and
the built-in kept, since a screen you cannot type into is worse than one that
ignored your config.

The TUI holds a complete compiled-in theme and renders correctly with no daemon
at all. `ui.config` is an upgrade, never a precondition for the first frame.

## See also

- [[Help/Lua/Configuration]] — where `init.lua` lives and how it loads
