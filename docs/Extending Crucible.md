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

Crucible supports multiple scripting languages:

| Language | Best For | Syntax |
|----------|----------|--------|
| **Lua** | General users, LLM-generated code | Simple, familiar |
| **Fennel** | Power users wanting macros | Lisp (compiles to Lua) |

### Lua (Recommended)

Simple, accessible syntax that LLMs write exceptionally well:

- [[Help/Lua/Language Basics]] - Lua syntax fundamentals
- [[Help/Lua/Configuration]] - Configuring Crucible via init.lua

### Fennel (Optional)

S-expression syntax with compile-time macros, for power users:

- Compiles to Lua at load time
- Full access to Lua ecosystem
- Macro support for custom DSLs

## Extension Points

Create reusable extensions:

- [[Help/Extending/Creating Plugins]] - Build and run Lua plugins
- [[Help/Extending/Event Hooks]] - React to file changes and system events
- [[Help/Extending/Custom Tools]] - Add tools via MCP or Lua
- [[Help/Extending/Agent Cards]] - Configure AI agent behavior

## Workflows

Automate multi-step processes:

- [[Help/Extending/Workflow Authoring]] - Define workflow steps

## Examples

See working examples in this kiln:

- [[Agents/Researcher]] - Research-focused agent card
- [[Agents/Coder]] - Code analysis agent
- [[Agents/Reviewer]] - Quality review agent

## Related

- [[AI Features]] - Agent and chat capabilities
- [[Configuration]] - Setting up providers and backends
- [[Help/Concepts/Agents & Protocols]] - Understanding MCP and ACP

## See Also

- [[Index]] - Return to main index
- `:h extending` - Help system entry point
