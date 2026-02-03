---
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
-- Configure the statusline (cru.* is canonical, crucible.* still works)
cru.statusline.setup({
    left = {
        cru.statusline.mode(),
        cru.statusline.model({ max_length = 25 }),
    },
    right = {
        cru.statusline.notification({
            fallback = cru.statusline.context(),
        }),
    },
})
```

## Config Locations

| Location | Purpose |
|----------|---------|
| `~/.config/crucible/init.lua` | Global config (always loaded) |
| `<kiln>/.crucible/init.lua` | Kiln-specific config (loaded after global) |

Kiln config runs after global config, so it can override settings.

## Built-in Modules

All built-in modules are under the `cru` namespace (canonical). The `crucible` namespace is a backwards-compatible alias.

```lua
-- Canonical namespace (preferred)
cru.statusline       -- Statusline configuration
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

-- Daemon-side modules (available when running as a plugin in cru-server)
cru.sessions         -- Session management: create, get, list, send_message, subscribe, etc.

-- Legacy aliases (still work)
crucible.statusline  -- same as cru.statusline
crucible.log         -- same as cru.log
crucible.json_encode -- same as cru.json.encode
crucible.json_decode -- same as cru.json.decode
crucible.include     -- same as cru.include
-- Standalone globals: http, fs, shell, oq, paths (backwards-compat)
```

## Statusline Configuration

The statusline appears at the bottom of the TUI. Configure it with `cru.statusline.setup()`:

```lua
cru.statusline.setup({
    left = { ... },      -- Left-aligned components
    center = { ... },    -- Center-aligned components  
    right = { ... },     -- Right-aligned components
    separator = " ",     -- Between components (default: space)
})
```

### Components

#### mode()

Shows the current chat mode (Normal/Plan/Auto):

```lua
-- With defaults
cru.statusline.mode()

-- With custom styling
cru.statusline.mode({
    normal = { text = " NORMAL ", bg = "green", fg = "black" },
    plan = { text = " PLAN ", bg = "blue", fg = "black" },
    auto = { text = " AUTO ", bg = "yellow", fg = "black" },
})
```

#### model()

Shows the current model name:

```lua
-- With defaults
cru.statusline.model()

-- With options
cru.statusline.model({
    max_length = 20,     -- Truncate long names
    fallback = "...",    -- Show when no model
    fg = "cyan",         -- Text color
})
```

#### context()

Shows context window usage:

```lua
-- With defaults (shows "42% ctx")
cru.statusline.context()

-- With custom format
cru.statusline.context({
    format = "{percent}%",
    fg = "gray",
})
```

#### text()

Static text with optional styling:

```lua
cru.statusline.text(" | ", { fg = "gray" })
cru.statusline.text("Crucible", { fg = "cyan", bold = true })
```

#### spacer()

Flexible space that pushes components apart:

```lua
cru.statusline.spacer()
```

#### notification()

Shows transient notifications (toasts and warning/error counts). Supports an optional `fallback` component that renders when no notifications are active:

```lua
-- Simple notification area
cru.statusline.notification({ fg = "yellow" })

-- With fallback to context usage when idle
cru.statusline.notification({
    fg = "yellow",
    fallback = cru.statusline.context({ fg = "gray" }),
})
```

When a toast or warning counts are active, the notification component renders them. When idle, it renders the `fallback` component (if set) or nothing.

### Colors

Named colors: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `gray`, `darkgray`

Hex colors: `#ff5500`, `#1a1a1a`

## Including Other Files

Split your config into multiple files:

```lua
-- ~/.config/crucible/init.lua
cru.include("statusline.lua")  -- loads ~/.config/crucible/statusline.lua
cru.include("keymaps.lua")     -- loads ~/.config/crucible/keymaps.lua
```

## Example: Full Configuration

```lua
-- ~/.config/crucible/init.lua

-- Statusline with all features
cru.statusline.setup({
    left = {
        cru.statusline.mode({
            normal = { text = " N ", bg = "#98c379", fg = "black" },
            plan = { text = " P ", bg = "#61afef", fg = "black" },
            auto = { text = " A ", bg = "#e5c07b", fg = "black" },
        }),
    },
    center = {
        cru.statusline.model({ max_length = 25, fg = "cyan" }),
    },
    right = {
        cru.statusline.notification({
            fg = "yellow",
            fallback = cru.statusline.context({ fg = "gray" }),
        }),
    },
})

cru.log("info", "Config loaded!")
```

## Troubleshooting

**Config not loading?**
- Check file exists: `ls ~/.config/crucible/init.lua`
- Check for syntax errors: run `lua ~/.config/crucible/init.lua`
- Check logs: `cru chat` with `RUST_LOG=crucible_lua=debug`

**Statusline not changing?**
- Ensure your `init.lua` runs before the TUI starts (check for syntax errors first)
- `cru.statusline.setup()` is loaded at TUI startup — changes require restarting the chat session
- Crucible ships an embedded Lua default that runs before your `init.lua`. Your config overrides it — you don't need to configure everything from scratch
- If the Lua runtime fails entirely, a minimal emergency statusline (mode + model) renders in pure Rust
- Check logs with `RUST_LOG=crucible_lua=debug` to verify config was loaded

## See Also

- [[Scripting Languages]] - Overview of Lua in Crucible
- [[Creating Plugins]] - Writing Lua plugins
