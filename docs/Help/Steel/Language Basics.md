---
description: Steel (Scheme) scripting reference for Crucible
status: stub
tags:
  - steel
  - scheme
  - scripting
  - reference
---

# Steel Language Basics

Steel is a Scheme implementation with Racket-style semantics, integrated into Crucible for plugin development.

> **Note**: This page is a stub. Full documentation coming soon.

## Why Steel?

Steel's killer feature is **contracts with blame tracking**. When something goes wrong, Steel tells you exactly:
- What constraint was violated
- Which argument failed
- Who called the function incorrectly

This makes Steel ideal for building validated pipelines where correctness matters.

## Key Features

- **Contracts**: Runtime validation with detailed error messages
- **Hygienic macros**: Safe metaprogramming
- **Racket compatibility**: Familiar to Scheme/Racket users
- **Hash tables**: Native support for JSON-like data structures

## Resources

- [Steel GitHub](https://github.com/mattwparas/steel) — Language repository
- [[Help/Concepts/Scripting Languages]] — Language comparison
- [[Help/Extending/Creating Plugins]] — Plugin development guide

## See Also

- [[Help/Lua/Language Basics]] — Lua reference
