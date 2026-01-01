---
description: Choose your scripting language for Crucible plugins
status: implemented
tags:
  - concept
  - scripting
  - plugins
  - rune
  - steel
  - lua
---

# Scripting Languages

Crucible supports multiple scripting languages for plugins, tools, and hooks. Choose the language that fits your needs.

## Overview

| Language | Best For | Key Feature |
|----------|----------|-------------|
| **Rune** | Performance, Rust integration | Native async, sandboxed |
| **Steel** | Correctness, formal validation | Contracts with blame tracking |
| **Lua** | Simplicity, LLM-generated code | Familiar syntax, Fennel support |

All three languages can:
- Define MCP tools via annotations
- Register event hooks
- Access the Crucible API (search, notes, graph)
- Execute shell commands (with policy controls)

## Rune

Rune is a Rust-native scripting language designed for embedding. It's the default choice for Crucible plugins.

**Strengths:**
- Fastest execution (compiles to bytecode)
- Native async/await support
- Rust-like syntax and error handling
- Full sandboxing

See [[Help/Rune/Language Basics]] for syntax and examples.

## Steel (Scheme)

Steel is a Scheme implementation with Racket-style semantics. Its killer feature is **contracts with blame tracking** — runtime validation that tells you exactly what went wrong and who's responsible.

**Strengths:**
- Contracts catch bugs at boundaries
- Hygienic macros
- Functional programming idioms
- Great for formal validation

When a contract is violated, Steel reports exactly which argument failed and who called the function incorrectly.

See [[Help/Steel/Language Basics]] for syntax and examples.

## Lua

Lua is a simple, embeddable scripting language with massive adoption. Crucible uses **Luau** (Lua with gradual types) and optionally supports **Fennel** (a Lisp that compiles to Lua).

**Strengths:**
- Familiar syntax (if you know JavaScript/Python)
- LLM-friendly (models generate good Lua)
- Fennel for Lisp lovers
- Gradual types for documentation

See [[Help/Lua/Language Basics]] for syntax and examples.

## Choosing a Language

### Use Rune when...

- Performance matters
- You're comfortable with Rust-like syntax
- You need async operations
- You want maximum sandboxing

### Use Steel when...

- Correctness is critical
- You want runtime validation with good error messages
- You prefer functional programming
- You're building validated pipelines

### Use Lua when...

- You want simple, readable code
- LLMs will generate your plugins
- You prefer Lisp (use Fennel)
- You're prototyping quickly

## Plugin Discovery

All languages use the same discovery mechanism. Place files in:

```
~/.config/crucible/plugins/     # Global personal
KILN/.crucible/plugins/         # Kiln personal (gitignored)
KILN/plugins/                   # Kiln shared (version-controlled)
```

File extensions determine the runtime:
- `.rn` — Rune
- `.scm` — Steel
- `.lua` — Lua
- `.fnl` — Fennel (compiles to Lua)

## See Also

- [[Help/Rune/Language Basics]] — Rune reference
- [[Help/Steel/Language Basics]] — Steel reference
- [[Help/Lua/Language Basics]] — Lua reference
- [[Help/Extending/Creating Plugins]] — Plugin development guide
- [[Help/Extending/Custom Tools]] — Adding MCP tools
- [[Help/Extending/Event Hooks]] — Reacting to events
