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
-- Configure the statusline
crucible.statusline.setup({
    left = { crucible.statusline.mode() },
    center = { crucible.statusline.model({ max_length = 20 }) },
    right = { 
        crucible.statusline.context(),
        crucible.statusline.notification() 
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

All built-in modules are under the `crucible` namespace:

```lua
crucible.statusline  -- Statusline configuration
crucible.log         -- Logging (debug, info, warn, error)
crucible.json_encode -- Convert table to JSON string
crucible.json_decode -- Parse JSON string to table
crucible.include     -- Load another config file
```

## Statusline Configuration

The statusline appears at the bottom of the TUI. Configure it with `crucible.statusline.setup()`:

```lua
crucible.statusline.setup({
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
crucible.statusline.mode()

-- With custom styling
crucible.statusline.mode({
    normal = { text = " NORMAL ", bg = "green", fg = "black" },
    plan = { text = " PLAN ", bg = "blue", fg = "black" },
    auto = { text = " AUTO ", bg = "yellow", fg = "black" },
})
```

#### model()

Shows the current model name:

```lua
-- With defaults
crucible.statusline.model()

-- With options
crucible.statusline.model({
    max_length = 20,     -- Truncate long names
    fallback = "...",    -- Show when no model
    fg = "cyan",         -- Text color
})
```

#### context()

Shows context window usage:

```lua
-- With defaults (shows "42% ctx")
crucible.statusline.context()

-- With custom format
crucible.statusline.context({
    format = "{percent}%",
    fg = "gray",
})
```

#### text()

Static text with optional styling:

```lua
crucible.statusline.text(" | ", { fg = "gray" })
crucible.statusline.text("Crucible", { fg = "cyan", bold = true })
```

#### spacer()

Flexible space that pushes components apart:

```lua
crucible.statusline.spacer()
```

#### notification()

Shows transient notifications:

```lua
crucible.statusline.notification({ fg = "yellow" })
```

### Colors

Named colors: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `gray`, `darkgray`

Hex colors: `#ff5500`, `#1a1a1a`

## Including Other Files

Split your config into multiple files:

```lua
-- ~/.config/crucible/init.lua
crucible.include("statusline.lua")  -- loads ~/.config/crucible/statusline.lua
crucible.include("keymaps.lua")     -- loads ~/.config/crucible/keymaps.lua
```

## Example: Full Configuration

```lua
-- ~/.config/crucible/init.lua

-- Statusline with all features
crucible.statusline.setup({
    left = {
        crucible.statusline.mode({
            normal = { text = " N ", bg = "#98c379", fg = "black" },
            plan = { text = " P ", bg = "#61afef", fg = "black" },
            auto = { text = " A ", bg = "#e5c07b", fg = "black" },
        }),
    },
    center = {
        crucible.statusline.model({ max_length = 25, fg = "cyan" }),
    },
    right = {
        crucible.statusline.context({ fg = "gray" }),
        crucible.statusline.text(" ", {}),
        crucible.statusline.notification({ fg = "yellow" }),
    },
})

crucible.log("info", "Config loaded!")
```

## Troubleshooting

**Config not loading?**
- Check file exists: `ls ~/.config/crucible/init.lua`
- Check for syntax errors: run `lua ~/.config/crucible/init.lua`
- Check logs: `cru chat` with `RUST_LOG=crucible_lua=debug`

**Statusline not changing?**
- The TUI currently uses hardcoded defaults
- Lua config is scaffolding for future customization
- `crucible.statusline.setup()` stores config but rendering uses it in future versions

## See Also

- [[Scripting Languages]] - Overview of Lua in Crucible
- [[Creating Plugins]] - Writing Lua plugins
