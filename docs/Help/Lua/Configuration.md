---
description: Documentation note for Configuration.
title: Lua Configuration
tags:
  - lua
  - config
  - reference
---

# Lua Configuration

Crucible loads Lua configuration from `~/.config/crucible/init.lua` at startup. This file can configure the TUI, define keybindings, and customize behavior.

## Quick Start

Create `~/.config/crucible/init.lua`:

```lua
-- Configure plugins
require("kiln-expert").setup({
  kilns = { docs = "~/crucible/docs" },
})

-- Colours
crucible.colorscheme.setup({ colors = { primary = "term4" } })

-- Statusline
local sl = crucible.statusline
sl.setup({
  main = {
    anchor = "footer.below_input",
    items  = { sl.mode, " ", sl.model{ max = 25 },
               sl.align,
               sl.any(sl.notification, sl.context) },
  },
})
```

## Config Locations

| Location | Purpose | Load Order |
|----------|---------|------------|
| Built-in defaults | Precognition format, session defaults, bundled plugins | First (embedded) |
| `~/.config/crucible/init.lua` | Your config — overrides defaults | Second |
| `<kiln>/.crucible/lua/init.lua` | Kiln-specific config | Third |

Your init.lua runs after the built-in defaults, so you can override anything. Kiln config runs last and can override both.

## Configuring Plugins

Plugins are configured via `require("name").setup({...})` — the same pattern as Neovim plugins.

```lua
-- Configure a bundled plugin with custom settings
require("kiln-expert").setup({
  kilns = {
    docs = "~/crucible/docs",
    research = "~/notes/research",
  },
  timeout = 60,
})
```

Bundled plugins (in `runtime/plugins/`) load with defaults automatically. Your `setup()` call overrides those defaults. To skip a bundled plugin entirely, don't call `require()` for it.

See [[Help/Extending/Creating Plugins]] for writing your own plugins.

## Built-in Modules

Runtime APIs live under `cru`; configuration lives under `crucible`. The two are
not aliases of each other, and one pair in particular is easy to mix up:

| Call | Namespace | When |
|---|---|---|
| `crucible.statusline.setup{}` | config | once, defining the bars |
| `cru.statusline.set(session, key, value)` | runtime | any time, supplying a value |

The UI-config namespaces — `crucible.colorscheme`, `crucible.hl`,
`crucible.ui`, `crucible.statusline`, `crucible.syntax` — exist only under
`crucible`, because they describe what the UI *is* rather than doing something.

```lua
-- Runtime namespace
cru.log(level, msg)  -- Logging (debug, info, warn, error)
cru.json.encode(tbl) -- Convert table to JSON string
cru.json.decode(str) -- Parse JSON string to table
cru.include          -- Load another config file

-- Also available via cru.*
cru.http             -- HTTP requests (GET, POST, PUT, etc.)
cru.fs               -- Filesystem operations
cru.shell            -- Shell command execution
cru.oq               -- Data query/transform (parse, json, etc.)
cru.paths            -- Path utilities
cru.ws               -- WebSocket client
cru.kiln             -- Kiln access
cru.graph            -- Knowledge graph queries

-- Utility modules
cru.timer            -- sleep(secs), timeout(secs, fn), clock()
cru.ratelimit        -- Rate limiter: new({capacity, interval})
cru.retry(fn, opts)  -- Exponential backoff retry
cru.emitter.new()    -- Event emitter (:on, :once, :off, :emit)
cru.check            -- Argument validation (.string, .number, .boolean, .table, .func, .one_of)
cru.spawn(fn)        -- Spawn async task (daemon context only, requires send feature)

-- Daemon-side modules (available when running as a plugin in the daemon)
cru.sessions         -- Session management: create, get, list, send_message, subscribe, etc.

-- UI configuration (crucible only — no cru equivalent)
crucible.colorscheme -- colour palette
crucible.hl          -- highlight groups (set, link)
crucible.ui          -- surface geometry, prompt glyphs, layout
crucible.statusline  -- statusline bars and item vocabulary
crucible.syntax      -- code highlighting

-- Runtime statusline values (cru only — the counterpart to the config above)
cru.statusline.set   -- push a value for `sl.expr("key")`
cru.statusline.clear -- drop one

-- Legacy aliases (still work)
crucible.log         -- same as cru.log
crucible.json_encode -- same as cru.json.encode
crucible.json_decode -- same as cru.json.decode
crucible.include     -- same as cru.include
-- Standalone globals: http, fs, shell, oq, paths (backwards-compat)
```

## Statusline Configuration

A bar is a **list of items**, so it reads the way it renders:

```lua
local sl = crucible.statusline

sl.setup({
  main = {
    anchor = "footer.below_input",
    items  = { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
               sl.align,
               sl.any(sl.notification, sl.context) },
  },
})
```

Anchors: `top`, `bottom`, `footer.above_input`, `footer.below_input`. Defining
more than one key gives you more than one bar.

### Items

| Item | Renders |
|---|---|
| `sl.mode` | the chat mode badge (Normal/Plan/Auto) |
| `sl.model{ max = 25, fallback = "…" }` | the active model, truncated |
| `sl.context` | context-window usage |
| `sl.cache` | prompt-cache hit rate, once one is known |
| `sl.status` | the daemon's status text |
| `sl.notification` | the active toast, or pending counts |
| `sl.align` | an alignment split |
| `sl.expr("key")` | a value pushed from a handler |
| `"any string"` | literal text |

Built-in items are evaluated by the TUI on every frame and cost no RPC.

`:hl("GroupName")` styles any item with a highlight group.

### Conditionals

Lua's `or` does not work here — item objects are truthy, so `a or b` always
takes the first, and the branch has to survive being sent to the client:

```lua
sl.any(sl.notification, sl.context)   -- first one that renders something
sl.when("streaming", sl.cache)        -- only while a turn is streaming
```

Conditions are facts only the TUI knows: `"streaming"`, `"has_notification"`,
`"mode:plan"`.

### Values the daemon computes

```lua
sl.setup({ main = { items = { sl.mode, sl.align, sl.expr("git") } } })

crucible.on("FileChanged", function(ctx)
  local out = cru.shell.exec("git status -b --porcelain")
  cru.statusline.set(ctx.session_id, "git", parse_branch(out))
end)
```

An unset expression renders nothing, so the bar does not jump when the first
value arrives. Re-setting an unchanged value costs no repaint.

See [[Extending/Scripted UI]] for colours, surfaces and borders.

## Example: Full Configuration

```lua
-- ~/.config/crucible/init.lua

-- Colours. `term4` is the terminal's slot 4 — whatever the user put there —
-- rather than a claim that it looks blue.
crucible.colorscheme.setup({
  name   = "mine",
  colors = { primary = "term4", success = "term2", text_dim = "bright_black" },
})

crucible.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })

-- Surfaces
crucible.ui.setup({
  popup  = { border = "rounded", padding = 1, max_visible = 10 },
  prompt = { normal = { glyph = "❯ " } },
  layout = { status_bar = "bottom", message_spacing = 1 },
})

-- Code blocks follow the colours above
crucible.syntax.setup({ theme = "derived" })

-- Statusline
local sl = crucible.statusline
sl.setup({
  main = {
    anchor = "footer.below_input",
    items  = { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
               sl.align,
               sl.any(sl.notification, sl.context) },
  },
})

cru.log("info", "Config loaded!")
```

## Troubleshooting

**Config not loading?**
- Check file exists: `ls ~/.config/crucible/init.lua`
- Check for syntax errors: run `lua ~/.config/crucible/init.lua`
- Check logs: `cru chat` with `RUST_LOG=crucible_lua=debug`

**Statusline or colours not changing?**
- Your `init.lua` is evaluated by the **daemon**, not by `cru`. A stale daemon
  serves stale config — `plugin.reload` re-evaluates it and pushes the result to
  every attached client, so a restart is not required
- Check for syntax errors first; a config that fails to load leaves the built-in
  default in place
- Crucible ships a built-in default that runs before your `init.lua`, so you only
  need to configure what you want to change
- If the daemon is unreachable entirely, the TUI renders from a complete
  compiled-in theme rather than failing
- Check logs with `RUST_LOG=crucible_lua=debug` to verify the config was loaded

## See Also

- [[Scripting Languages]] - Overview of Lua in Crucible
- [[Creating Plugins]] - Writing Lua plugins
