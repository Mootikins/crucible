---
description: Lua scripting reference for Crucible
status: stub
tags:
  - lua
  - luau
  - fennel
  - scripting
  - reference
---

# Lua Language Basics

Crucible uses Luau (Lua with gradual types) for plugin development, with optional Fennel support.

> **Note**: This page is a stub. Full documentation coming soon.

## Why Lua?

Lua is one of the most widely-used scripting languages, with simple syntax that's easy for both humans and LLMs to write. If you want AI to generate your plugins, Lua is an excellent choice.

## Key Features

- **Simple syntax**: Easy to learn if you know JavaScript or Python
- **Gradual types**: Optional type annotations for documentation
- **Fennel support**: Write in Lisp syntax, compile to Lua
- **LLM-friendly**: Models generate high-quality Lua code

## Fennel

Fennel is a Lisp that compiles to Lua. Use `.fnl` files if you prefer Lisp syntax with Lua's runtime.

## Resources

- [Lua Reference Manual](https://www.lua.org/manual/5.4/)
- [Luau Documentation](https://luau-lang.org/)
- [Fennel Language](https://fennel-lang.org/)
- [[Help/Concepts/Scripting Languages]] — Language comparison
- [[Help/Extending/Creating Plugins]] — Plugin development guide

## See Also

- [[Help/Rune/Language Basics]] — Rune reference
- [[Help/Steel/Language Basics]] — Steel reference
