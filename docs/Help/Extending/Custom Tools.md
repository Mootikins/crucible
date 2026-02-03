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

Extend agent capabilities with custom tools written in Lua or exposed via MCP.

## Overview

Tools are functions that agents can call to interact with the world:
- Search notes
- Read/write files
- Execute commands
- Call external APIs

## Lua Tools

Create tools using the Lua scripting language:

```lua
-- tools/search_web.lua

--- Search the web for information
-- @tool name="search_web" description="Search the web for information"
-- @param query string "Search query"
function search_web(args)
    local response = cru.http.get("https://api.search.com?q=" .. args.query)
    return { results = response.body }
end
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

- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Extending/MCP Gateway]] - External tool integration
- [[Help/Lua/Language Basics]] - Lua syntax
