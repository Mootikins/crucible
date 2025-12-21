---
description: How to create custom tools for Crucible agents
tags:
  - help
  - extending
  - tools
aliases:
  - Creating Tools
  - Tool Development
---

# Custom Tools

Extend agent capabilities with custom tools written in Rune or exposed via MCP.

## Overview

Tools are functions that agents can call to interact with the world:
- Search notes
- Read/write files
- Execute commands
- Call external APIs

## Rune Tools

Create tools using the Rune scripting language:

```rune
// tools/search_web.rn
pub fn search_web(query) {
    // Tool implementation
    let results = http::get(f"https://api.search.com?q={query}");
    results
}
```

## MCP Tools

Expose tools via Model Context Protocol:

```toml
# Config.toml
[[mcp.servers]]
name = "my-tools"
command = "my-mcp-server"
```

## Tool Definition

```yaml
name: search_web
description: Search the web for information
parameters:
  query:
    type: string
    description: Search query
    required: true
```

## See Also

- [[Help/Rune/Tool Definition]] - Tool and param attributes
- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Extending/MCP Gateway]] - External tool integration
- [[Scripts/Auto Tagging]] - Example Rune script
