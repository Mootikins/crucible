---
description: Map of all extension points for customizing and extending Crucible
status: implemented
tags:
  - moc
  - extending
  - plugins
---

# Extending Crucible

Crucible is designed to be extended. This map connects all the ways you can customize behavior, add capabilities, and integrate with external tools.

## Scripting Languages

Crucible supports multiple scripting languages to serve different needs:

| Language | Best For | Syntax |
|----------|----------|--------|
| **Lua** | General users, LLM-generated code | Simple, familiar |
| **Fennel** | Power users wanting macros | Lisp (compiles to Lua) |
| **Rune** | Rust developers, system integration | Rust-like |

See [[Meta/Analysis/scripting-language-philosophy]] for the reasoning behind this design.

### Lua (Recommended for Most Users)

Simple, accessible syntax that LLMs write exceptionally well:

- [[Help/Lua/Getting Started]] - First steps with Lua extensions
- [[Help/Lua/Tool Definitions]] - Creating custom tools
- [[Help/Lua/Event Handlers]] - Reacting to system events

### Fennel (Optional)

S-expression syntax with compile-time macros, for power users:

- [[Help/Fennel/Overview]] - When and why to use Fennel
- [[Help/Fennel/Macros]] - Defining custom DSLs

### Rune

Rust-like syntax with native async and the `?` operator:

- [[Help/Rune/Language Basics]] - Rune syntax fundamentals
- [[Help/Rune/Crucible API]] - Built-in functions for reading, searching, creating notes
- [[Help/Rune/Best Practices]] - Error handling, mode checking, debugging

## Extension Points

Create reusable extensions:

- [[Help/Extending/Creating Plugins]] - Build and run Rune plugins
- [[Help/Extending/Event Hooks]] - React to file changes and system events
- [[Help/Extending/Custom Tools]] - Add tools via MCP or Rune
- [[Help/Extending/Agent Cards]] - Configure AI agent behavior

## Workflows

Automate multi-step processes:

- [[Help/Extending/Workflow Authoring]] - Define workflow steps

## Examples

See working examples in this kiln:

- [[Agents/Researcher]] - Research-focused agent card
- [[Agents/Coder]] - Code analysis agent
- [[Agents/Reviewer]] - Quality review agent
- [[Scripts/Auto Tagging]] - Automatic tag suggestions
- [[Scripts/Daily Summary]] - Daily note generation

## Related

- [[AI Features]] - Agent and chat capabilities
- [[Configuration]] - Setting up providers and backends
- [[Help/Concepts/Agents & Protocols]] - Understanding MCP and ACP

## See Also

- [[Index]] - Return to main index
- `:h extending` - Help system entry point
