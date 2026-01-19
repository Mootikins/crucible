---
description: Analysis of scripting language choices for Crucible's extension system
status: decided
tags:
  - analysis
  - extensibility
  - scripting
date: 2025-12-27
updated: 2026-01-18
---

# Scripting Language Philosophy

This document captures the reasoning behind Crucible's Lua-based extension approach.

## Core Principles

Crucible's extensibility follows three guiding principles:

1. **Transparency** - Users understand how their data moves and what agents see
2. **Extensibility** - APIs enable customization; agents can help users extend the system
3. **Continual Growth** - The knowledge base grows sustainably, building ground truth

For extensibility specifically, the gold standards are **Neovim** and **Obsidian** - both achieve deep customization through event-driven architectures and accessible scripting.

## Design Decisions

### Declarative UI Model

Extensions return **data structures**, not UI code. The runtime interprets declarations and renders appropriate components for each frontend (CLI, web, desktop).

```
Extension Code (Lua/Fennel)
       ↓
Returns: { component = "search_results", data = {...} }
       ↓
Rust Runtime (interprets declaration)
       ↓
UI Layer (renders "search_results" appropriately)
```

This decouples the scripting language from UI concerns and enables multi-frontend support.

### Lua as Primary Language

Crucible uses **Lua** (via mlua) as its primary scripting language, with **Fennel** as an optional layer:

| Language | Audience | Strengths |
|----------|----------|-----------|
| **Lua** | General users, LLM-generated code | Simple syntax, massive training data, mature ecosystem |
| **Fennel** | Power users wanting macros | S-expressions, compile-time macros, compiles to Lua |

Both languages return the same data shapes to Rust, enabling a unified tool/handler interface.

### Why Lua?

**Lua optimizes for the criteria that matter most:**

| Criterion | Lua | Fennel |
|-----------|-----|--------|
| Non-technical accessibility | Excellent | Medium |
| LLM code generation | Excellent | Good |
| Macro system | None | Excellent |
| Threading (`Send+Sync`) | Yes (`send` feature) | Yes (via Lua) |
| Async support | Via mlua | Via Lua |
| Ecosystem | Neovim patterns, massive libraries | Compiles to Lua |

### Language Selection Rationale

#### Lua via mlua

- **LLM writability** - models write Lua exceptionally well
- **Non-technical users** - simple, forgiving syntax
- **Threading** - `send` feature enables `Send+Sync`
- **Ecosystem** - Neovim patterns, massive library ecosystem
- **Lightweight** - minimal runtime overhead

#### Fennel (Optional Layer)

- **S-expressions** - unambiguous structure, trivial for LLMs to parse
- **Macros** - users can define DSLs for tool definitions
- **Compiles to Lua** - zero runtime overhead, ~160KB compiler
- **Choice** - power users opt-in, others use plain Lua

## Considered Alternatives

### Static Typing Options

| Language | Status | Why Not Primary |
|----------|--------|-----------------|
| **Gluon** | Unmaintained | Dead project (last commit 2022) |
| **Starlark** | Mature | No async support |
| **Teal** | Active | Adds complexity (compile step) |
| **Rune** | Removed | `!Send/!Sync` limitation blocked multi-threaded usage |

### Runtime Contract Systems

**Steel (Scheme)** offers Racket-style contracts:

```scheme
(define/contract (divide x y)
  (-> number? (and/c number? (not/c zero?)) number?)
  (/ x y))
```

Contracts can express richer invariants than static types (e.g., "positive number", "sorted list") but errors are runtime-only. Steel's `!Send/!Sync` limitation and S-expression syntax barrier led to choosing Lua instead.

### WASM

Rejected for MVP - requires compilation rather than interpretation. May revisit for sandboxed third-party plugins.

## Implementation Architecture

```
┌─────────────────────────────────────────────┐
│  User Extensions                            │
│  ├── tool.lua      (simple, accessible)     │
│  └── tool.fnl      (power users)            │
├─────────────────────────────────────────────┤
│  Scripting Runtimes                         │
│  ├── mlua (Luau, async, send feature)       │
│  └── Fennel compiler (~160KB, optional)     │
├─────────────────────────────────────────────┤
│  Unified Tool Interface                     │
│  └── All languages return same ToolResult   │
├─────────────────────────────────────────────┤
│  Rust Core                                  │
└─────────────────────────────────────────────┘
```

## Validation Strategy

Type safety achieved at the Rust boundary, not in scripting languages:

```rust
#[derive(Deserialize, JsonSchema)]
struct SearchParams {
    query: String,
    #[serde(default = "default_limit")]
    limit: u32,
}

// Validate when loading tool, not when calling
fn load_tool(source: &str) -> Result<Tool> {
    let handler = runtime.compile(source)?;
    validate_params_schema::<SearchParams>(&handler)?;
    Ok(Tool { handler, schema: SearchParams::json_schema() })
}
```

## Future Considerations

- **WASM plugins** - for untrusted third-party code with true sandboxing
- **Runtime contracts** - could add Lua-side validation via library
- **Luau type annotations** - leverage gradual typing in Luau mode

## Related

- [[Extending Crucible]] - User-facing extension documentation
- [[Meta/Analysis/Event Architecture]] - Event system design
- [[Meta/Plugin API Sketches]] - API design details
