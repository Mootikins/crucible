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
require("reflection").setup({
  enabled = true,
})

-- Colours
crucible.colorscheme.setup({ colors = { primary = "term4" } })

-- Statusline: a row below the input
local sl = crucible.statusline
sl.setup({
  prompt = {
    sl.input,
    { sl.mode, " ", sl.model{ max = 25 },
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
| `<workspace>/.crucible/lua/init.lua` | Per-project config | Third |

Your init.lua runs after the built-in defaults, so you can override anything. The per-project file runs last and can override both.

That third path is the session's **workspace** — where work happens — not its kiln. The two are often the same directory, which is why this is easy to get wrong; the daemon reads `session.workspace`.

## Configuring Plugins

Plugins are configured via `require("name").setup({...})` — the same pattern as Neovim plugins.

```lua
-- Configure a bundled plugin with custom settings
require("reflection").setup({
  enabled = true,
  model = "llama3.2",
  timeout = 60,
})
```

Bundled plugins (in `runtime/plugins/`) load with defaults automatically. Your `setup()` call overrides those defaults. To skip a bundled plugin entirely, don't call `require()` for it.

See [[Help/Extending/Creating Plugins]] for writing your own plugins.

## Session Defaults and Modes

`cru.defaults` sets the value every new session starts with — the Neovim
`vim.o` tier. `session.x` inside a handler changes one session, the `vim.bo`
tier.

```lua
cru.defaults.system_prompt = "Answer in British English."
cru.defaults.temperature = 0.3
```

Modes are declared, not built in. `cru.modes.<name>` takes a tool set and a
permission stance; the three shipped modes are declared this same way in
`runtime/defaults/init.lua`, so yours are not second-class.

```lua
cru.modes.review = {
  tools = { "read_*", "grep", "glob", "bash" },
  permissions = {
    default = "deny",
    allow = { "bash:rg *", "bash:git log *" },
  },
}
```

Declared modes appear in the TUI's `Shift+Tab` cycle and the web mode picker,
and each gets its own slash command (`/review`). Use a declaration for a static
rule and a hook for one that depends on the arguments — see
[[Help/TUI/Modes]] and [[Help/Concepts/Permission Precedence]].

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
crucible.include     -- Load another config file (crucible only — no cru.include)

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
-- Standalone globals: http, fs, shell, oq, paths, graph (backwards-compat)
```

> [!warning] `cru.config` and `crucible.config` are not the same function
> `cru.config.get(key)` reads one **top-level** value of the merged app config
> (`config.toml` seeded, `cru.config.set{}` overlaid) and takes no dotted
> paths. On the daemon's plugin VM, `crucible.config.get("plugin.key")` walks
> dotted keys into `[plugins.*]` config. This is a known trap — check which
> one you mean before reaching for either.

## Statusline Configuration

The screen is three ordered lists — regions `top`, `prompt`, and `bottom` — and
a region entry is either a **row** (a table of items) or, in `prompt`, the
`sl.input` marker for the editor itself. Position in the list is the
arrangement; there is no anchor and no ordering field:

```lua
local sl = crucible.statusline

sl.setup({
  prompt = {
    sl.input,
    { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
      sl.align,
      sl.any(sl.notification, sl.context) },
  },
})
```

Move `sl.input` below a row and that row renders above the editor. A region you
do not mention keeps the built-in default; a key that is not `top`, `prompt`,
or `bottom` (the old `main = {...}` spelling, say) places nothing and logs a
warning.

### Items

| Item | Renders |
|---|---|
| `sl.mode` | the chat mode badge — whatever mode the session is in |
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
and `"mode:<name>"` for any declared mode.

### Values the daemon computes

```lua
sl.setup({ prompt = { sl.input, { sl.mode, sl.align, sl.expr("git") } } })

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

-- Statusline: input first, one row below it
local sl = crucible.statusline
sl.setup({
  prompt = {
    sl.input,
    { sl.mode:hl("StatusMode"), " ", sl.model{ max = 25 },
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
