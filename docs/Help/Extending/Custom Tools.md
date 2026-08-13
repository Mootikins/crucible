---
title: Custom Tools
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

A tool is declared in a plugin's spec table. There is one declaration form and
one loader — see [[Help/Extending/Creating Plugins]].

```lua
-- ~/.config/crucible/plugins/search-web/init.lua

local function search_web(args)
    local response = cru.http.get("https://api.search.com?q=" .. args.query)
    return { results = response.body }
end

return {
    name = "search-web",
    tools = {
        search_web = {
            desc = "Search the web for information",
            params = { { name = "query", type = "string", desc = "Search query" } },
            fn = search_web,
        },
    },
}
```

A tool declared this way is reachable from an internal agent and over `cru mcp`
alike, because both serve the same plugin registry.

> [!NOTE] `-- @tool` doc comments no longer declare anything
> Earlier revisions showed an annotation form. Nothing parses it: a plugin that
> returns a spec table has its exports read from that table and nothing else,
> and the separate annotation loader that once fed `cru mcp` has been removed.
> A tool declared only by a comment never reaches an agent.

## MCP Tools

Expose tools via Model Context Protocol:

```toml
[[mcp.servers]]
name = "my-tools"
prefix = "my_"

[mcp.servers.transport]
type = "stdio"
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
