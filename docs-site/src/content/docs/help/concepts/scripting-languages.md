---
title: "Scripting Languages"
description: "Lua scripting for Crucible plugins"
---

Crucible uses Lua for plugins, tools, and hooks. Fennel (a Lisp that compiles to Lua) is also supported.

## Overview

| Language | Best For | Key Feature |
|----------|----------|-------------|
| **Lua** | General use, LLM-generated code | Familiar syntax, simple |
| **Fennel** | Power users wanting macros | Lisp syntax, compiles to Lua |

Both languages can:
- Define MCP tools via annotations
- Register event hooks
- Access the Crucible API (search, notes, graph)
- Execute shell commands (with policy controls)

## Lua

Lua is a simple, embeddable scripting language with massive adoption. Crucible uses **Luau** (Lua with gradual types) for enhanced tooling.

**Strengths:**
- Familiar syntax (if you know JavaScript/Python)
- LLM-friendly (models generate excellent Lua)
- Gradual types for documentation
- Simple and easy to debug

See [Language Basics](../lua/language-basics/) for syntax and examples.

## Fennel

Fennel is a Lisp that compiles to Lua. It provides:
- S-expression syntax
- Compile-time macros
- Full Lua interoperability
- Pattern matching

Fennel files (`.fnl`) are compiled to Lua at load time, so they have the same runtime characteristics.

## Choosing a Language

### Use Lua when...

- You want simple, readable code
- LLMs will generate your plugins
- You're prototyping quickly
- You prefer familiar syntax

### Use Fennel when...

- You love Lisp
- You want compile-time macros
- You're building DSLs
- You prefer s-expressions

## Plugin Discovery

Place files in:

```
~/.config/crucible/plugins/     # Global personal
KILN/.crucible/plugins/         # Kiln personal (gitignored)
KILN/plugins/                   # Kiln shared (version-controlled)
```

File extensions determine the runtime:
- `.lua` — Lua
- `.fnl` — Fennel (compiles to Lua)

## Configuration

Crucible also loads configuration from `~/.config/crucible/init.lua`. This allows customizing the TUI, defining keybindings, and more.

See [Configuration](../lua/configuration/) for details.

## Oil UI DSL

Both Lua and Fennel can build TUI components using the **Oil** (Obvious Interface Language) API. Oil provides a functional, React-like model where components are functions that return node trees.

```lua
-- Lua
local view = cru.oil.col({ gap = 1 },
    cru.oil.text("Hello", { bold = true }),
    cru.oil.when(loading, cru.oil.spinner())
)
```

```fennel
;; Fennel
(local oil (require :oil))
(oil.col {:gap 1}
  (oil.text "Hello" {:bold true})
  (oil.when loading (oil.spinner)))
```

See [Scripted UI](../extending/scripted-ui/) for the full Oil component reference.

## See Also

- [Language Basics](../lua/language-basics/) — Lua reference
- [Configuration](../lua/configuration/) — Lua configuration
- [Scripted UI](../extending/scripted-ui/) — Oil UI DSL reference
- [Creating Plugins](../extending/creating-plugins/) — Plugin development guide
- [Custom Tools](../extending/custom-tools/) — Adding MCP tools
- [Event Hooks](../extending/event-hooks/) — Reacting to events
