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
> Your `init.lua` is evaluated once, daemon-side, and the result is delivered to
> every attached client over the `ui.config` RPC as data. That is why styling is
> declarative, and why statusline values are *pushed* rather than computed per
> frame — see [[#Statusline]].
>
> An earlier version of this page documented `cru.popup`, `cru.ui` and
> `cru.panel` modules for building interactive popups from scripts. Those were
> never registered in a running daemon and have been removed. For asking the user
> something from a handler, use `cru.interaction`.

## Themes

A theme is a palette. Define one inline, or drop a file in
`~/.config/crucible/themes/`.

```lua
crucible.theme.setup{
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

Colour values are a named colour (`"cyan"`), hex (`"#282c34"`), or an adaptive
pair. Adaptive pairs cross the wire **unresolved** — the daemon cannot know which
terminal a client is attached to, so the client resolves them.

## Highlight groups

Groups are an open namespace: define your own, and link one to another.

```lua
crucible.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })
crucible.hl.link("PopupSelected", "Visual")
```

A colour is a literal, an adaptive pair, or a **palette reference** — the name of
a field in the active theme (`"mode_normal"` above). References resolve at use
time, so swapping the palette moves every group that references it.

`link` is a base to override, not a rename: attributes set on the linking group
beat the target, so you can say "like `Visual`, but red".

## Surfaces

Geometry is a closed set — the renderer has to know how to draw each surface.

```lua
crucible.ui.setup{
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

## Statusline

A bar is a list of items. Items are values, so a bar reads as a list:

```lua
local sl = crucible.statusline

sl.setup{
  main = {
    anchor = "footer.below_input",
    items  = { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
               sl.align,
               sl.any(sl.notification, sl.context) },
  },
  context = {
    anchor = "footer.above_input",
    items  = { sl.when("streaming", sl.cache) },
  },
}
```

Anchors: `top`, `bottom`, `footer.above_input`, `footer.below_input`.

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
local sl = crucible.statusline

sl.setup{
  main = { items = { sl.mode, sl.align, sl.expr("git"):hl("Git") } },
}

crucible.on("FileChanged", function(ctx)
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
than substituting a guess. A bar anchored somewhere unrecognised is skipped
entirely, since drawing it in the wrong place is worse than not drawing it.

The TUI holds a complete compiled-in theme and renders correctly with no daemon
at all. `ui.config` is an upgrade, never a precondition for the first frame.

## See also

- [[Help/Lua/Configuration]] — where `init.lua` lives and how it loads
