---
title: Scripting Languages
description: Luau scripting for Crucible plugins
status: implemented
tags:
  - concept
  - scripting
  - plugins
  - lua
---

# Scripting Languages

Crucible uses Luau for plugins, tools, and hooks.

## Overview

Luau plugins can:
- Define tools in a plugin's spec table, served to agents and `cru mcp` alike
- Register event hooks
- Access the Crucible API (search, notes, graph)
- Execute shell commands (with policy controls)

## Luau

Luau is a Lua-derived scripting language. Crucible embeds it via the `mlua` crate.

**Strengths:**
- Familiar syntax (if you know JavaScript/Python)
- LLM-friendly (models generate excellent Lua)
- Simple and easy to debug
- Optional strict mode and type annotations

See [[Help/Lua/Language Basics]] for syntax and examples.

## Plugin Discovery

Place files in:

```
~/.config/crucible/plugins/     # Global personal
<runtimepath entry>/plugins/    # Opt-in extra trees, named in init.lua
```

Plugins are user-scoped: no kiln, project or workspace directory is searched on
its own. See [[Help/Extending/Creating Plugins]] for why, and for how
`runtimepath` opts a tree in.

Plugin sources use the `.lua` extension.

## Configuration

Crucible also loads configuration from `~/.config/crucible/init.lua`. This allows customizing the TUI, defining keybindings, and more.

See [[Help/Lua/Configuration]] for details.

## Oil UI DSL

Luau can build TUI components using the **Oil** (Obvious Interface Language) API. Oil provides a functional, React-like model where components are functions that return node trees.

```lua
-- Lua
local view = cru.oil.col({ gap = 1 },
    cru.oil.text("Hello", { bold = true }),
    cru.oil.when(loading, cru.oil.spinner())
)
```

See [[Help/Plugins/Oil Lua API]] for the full Oil component reference;
[[Help/Extending/Scripted UI]] covers theming and statuslines.

## See Also

- [[Help/Lua/Language Basics]] — Lua reference
- [[Help/Lua/Configuration]] — Lua configuration
- [[Help/Plugins/Oil Lua API]] — Oil UI DSL reference
- [[Help/Extending/Creating Plugins]] — Plugin development guide
- [[Help/Extending/Custom Tools]] — Adding MCP tools
- [[Help/Extending/Event Hooks]] — Reacting to events
