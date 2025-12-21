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

## Scripting with Rune

Write scripts that interact with your kiln:

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
