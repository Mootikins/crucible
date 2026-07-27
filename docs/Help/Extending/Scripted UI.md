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

## Three things, three names

"Theme" used to mean three unrelated things here. They are now named separately:

| Name | Covers |
|---|---|
| `crucible.colorscheme` | the colour palette, and what highlight groups resolve against |
| `crucible.ui` | surface geometry, prompt glyphs, layout |
| `crucible.syntax` | code highlighting inside fenced blocks |

## Colorscheme

A colorscheme is a palette. Define one inline, or drop a file in
`~/.config/crucible/themes/` and switch with `ui.set_theme`.

```lua
crucible.colorscheme.setup{
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

## Syntax highlighting

By default code blocks derive their colours from the colorscheme, so a fenced
block does not clash with the chat around it:

```lua
crucible.syntax.setup{
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

## Statusline

The screen is three ordered lists. Position in a list is the arrangement, and
the input is an element like any other:

```lua
local sl = crucible.statusline

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
than substituting a guess. A layout that places no input is refused whole and
the built-in kept, since a screen you cannot type into is worse than one that
ignored your config.

The TUI holds a complete compiled-in theme and renders correctly with no daemon
at all. `ui.config` is an upgrade, never a precondition for the first frame.

## See also

- [[Help/Lua/Configuration]] — where `init.lua` lives and how it loads
