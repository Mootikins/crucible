---
title: Oil
description: Concept overview for the Oil rendering layer in Crucible
tags: [help, concepts, tui]
---

# Oil

**Oil** is Crucible's terminal UI layer. It provides a declarative, flex-based renderer for the TUI and exposes the same primitives to Lua plugins so they can build views without knowing about cursor positioning, ANSI escape codes, or terminal dimensions.

The Rust crate is `crucible-oil`. The Lua bindings live in `crucible-lua` and are exposed as `cru.oil.*`.

## Why a Dedicated Layer

Crucible's TUI runs across several surfaces — the chat viewport, popups, overlays, Lua-defined custom views — that all need consistent spacing, theming, and layout. Rather than each surface driving `crossterm` directly, everything builds a tree of `Node`s and hands it to the Oil renderer. The renderer handles:

- **Flex layout** (`col`, `row`, gaps, padding, alignment)
- **Theming** (named colors, bold/italic/underline, borders)
- **Wrapping** (terminal-aware text wrap, CJK-aware width)
- **Caching** (frame diffing so only changed cells get repainted)

## The Primitives

The Rust node tree and the Lua DSL mirror each other. The building blocks fall into four groups:

**Layout** — `col`, `row`, `fragment`, `spacer`  
**Content** — `text`, `divider`, `hr`, `spinner`, `progress`, `badge`  
**Lists** — `bullet_list`, `numbered_list`, `kv`  
**Input** — `input`, `popup` (dropdowns/menus)

Control flow helpers (`when`, `either`, `each`, `match_state`) let you build reactive views without imperative branching, and every node supports chainable style/padding/margin/border methods.

For the full Lua API with signatures and examples, see [[Help/Plugins/Oil-Lua-API]].

## Relation to the TUI

Every frame of the Crucible TUI is built by composing an Oil tree top-down: chat containers, the input area, the status bar, popups. The `Container` trait in the chat app builds sub-trees; the root tree is laid out in one pass and rendered to a `CellGrid`, which is diffed against the previous frame and flushed to the terminal.

Plugins can participate in two ways:

1. **Custom views** — `cru.view.register(name, fn)` registers a named view that returns an Oil tree. Users open it with `/view name` or bind it to a key.
2. **Status bar customization** — Lua can replace the default status bar renderer with its own Oil tree, accessing runtime state through the view's render callback.

## Relation to `crucible-oil` (the crate)

The crate is kept separate from `crucible-cli` because `crucible-lua` depends on it (the Lua bindings need the Rust types). Everything the renderer produces is a plain data structure — no I/O until the final `render_to_string()` call — which is what lets the same tree drive live TUI output, snapshot-tested golden strings, and GIF-replay fixtures.

## See Also

- [[Help/Plugins/Oil-Lua-API]] — Lua API reference
- [[Help/TUI/Component Architecture]] — how the chat TUI composes Oil trees
- [[Help/Extending/Scripted UI]] — writing custom views in Lua
