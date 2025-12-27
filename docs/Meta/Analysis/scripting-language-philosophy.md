---
description: Analysis of scripting language choices for Crucible's extension system
status: decided
tags:
  - analysis
  - extensibility
  - scripting
date: 2025-12-27
---

# Scripting Language Philosophy

This document captures the reasoning behind Crucible's multi-language extension approach.

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
Extension Code (any language)
       ↓
Returns: { component: "search_results", data: [...] }
       ↓
Rust Runtime (interprets declaration)
       ↓
UI Layer (renders "search_results" appropriately)
```

This decouples the scripting language from UI concerns and enables multi-frontend support.

### Multi-Language Support

Crucible supports multiple scripting languages to serve different audiences:

| Language | Audience | Strengths |
|----------|----------|-----------|
| **Rune** | Rust developers, system integration | Rust-like syntax, native async, `?` operator |
| **Lua** | General users, LLM-generated code | Simple syntax, massive training data, mature ecosystem |
| **Fennel** | Power users wanting macros | S-expressions, compile-time macros, compiles to Lua |

All languages return the same data shapes to Rust, enabling a unified tool/handler interface.

### Why Multiple Languages?

**No single language optimizes for all criteria:**

| Criterion | Rune | Lua | Fennel |
|-----------|------|-----|--------|
| Rust-familiar syntax | Excellent | Poor | Poor |
| Non-technical accessibility | Medium | Excellent | Poor |
| LLM code generation | Good | Excellent | Good |
| Macro system | Limited | None | Excellent |
| Threading (`Send+Sync`) | No* | Yes (`send` feature) | Yes (via Lua) |
| Async support | Native | Via mlua | Via Lua |

*Rune has `SyncFunction` workaround for limited cases.

### Language Selection Rationale

#### Rune (Existing)

- **Rust-like syntax** familiar to contributors
- **Native async** fits the async Rust ecosystem
- **`?` try operator** is intuitive for error handling
- **Investment exists** - already integrated

#### Lua via mlua (New)

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
│  ├── tool.fnl      (power users)            │
│  └── tool.rune     (Rust developers)        │
├─────────────────────────────────────────────┤
│  Scripting Runtimes                         │
│  ├── mlua (Lua 5.4, async, send feature)    │
│  ├── Fennel compiler (~160KB, optional)     │
│  └── Rune (existing)                        │
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

- **Gradual typing for Rune** - if upstream adds it, would improve the Rune story
- **WASM plugins** - for untrusted third-party code with true sandboxing
- **Runtime contracts** - could add Lua-side validation via library

## Related

- [[Extending Crucible]] - User-facing extension documentation
- [[Meta/Analysis/event-architecture]] - Event system design
- [[Meta/Plugin API Sketches]] - API design details
- [[Meta/Analysis/tools-mcp-rune]] - Tool integration patterns
