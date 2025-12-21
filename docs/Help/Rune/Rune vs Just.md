---
description: When to use Rune scripts vs Just recipes for automation
status: implemented
tags:
  - rune
  - just
  - guide
  - comparison
aliases:
  - Rune vs Justfile
  - Automation Comparison
---

# Rune vs Just

Crucible provides two complementary automation systems: **Rune scripts** for plugin logic and **Just recipes** for shell-based tasks.

## Overview

Both automate repetitive tasks, but operate at different layers:

- **Rune** integrates with Crucible's internal API, processing notes and reacting to events
- **Just** orchestrates shell commands and external tools

Think of Rune as "inside Crucible" and Just as "outside Crucible" automation.

## Comparison Table

| Aspect | Rune | Just |
|--------|------|------|
| Use Case | Plugin logic, event hooks | Shell commands, build tasks |
| Language | Rust-like syntax | Make-like syntax |
| Access | Crucible API (notes, search) | Shell commands |
| Events | Hook into tool/note events | N/A |
| Discovery | Auto-discovered from paths | Justfile in project root |
| Sandboxing | Yes (safe by default) | No (full shell access) |
| Async | Native async support | Sequential execution |
| Error Handling | Type-safe Result types | Exit codes |

## When to Use Rune

- **Process notes with Crucible API**: Search, parse, modify notes programmatically
- **React to events**: Hook into tool calls, note changes, or agent actions
- **Create custom tools**: Extend agent capabilities with new MCP tools
- **Complex logic**: Type-safe operations on knowledge graph

Rune runs sandboxed with access to Crucible's domain model—ideal for operations that understand note structure, wikilinks, or semantic relationships.

## When to Use Just

- **Run shell commands**: Execute system utilities, package managers, build tools
- **Build/test automation**: Compile code, run test suites, generate artifacts
- **System administration**: Set up environments, manage processes
- **External tools**: Integrate with tools outside Crucible's ecosystem

Just has full shell access for complex workflows involving external programs.

## Example Use Cases

| Task | Tool | Why |
|------|------|-----|
| Auto-tag notes based on content | Rune | Requires parsing via Crucible API |
| Run tests and deploy | Just | Shell orchestration |
| Transform tool output | Rune hook | Event intercepts tool results |
| Install dependencies | Just | Package manager commands |
| Generate note summaries | Rune | LLM provider through Crucible traits |
| Build web UI and start server | Just | Coordinates npm, cargo |

## See Also

- [[Help/Rune/Language Basics]]
- [[Help/Extending/Creating Plugins]]
