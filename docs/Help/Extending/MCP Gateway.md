---
title: MCP Gateway
description: Connect external MCP servers to add tools from GitHub, databases, and more
status: implemented
tags:
  - extending
  - mcp
  - tools
  - integration
aliases:
  - Gateway
  - External Tools
---

# MCP Gateway

The MCP Gateway connects Crucible to external Model Context Protocol servers. This lets you add tools from GitHub, filesystems, databases, and any other MCP-compatible service.

## Why Use the Gateway

Without the gateway, Crucible has built-in tools for your kiln. With the gateway, you can add:

- **GitHub** - Search code, read files, list repos
- **Filesystem** - Read files outside your kiln
- **Databases** - Query external data
- **Custom services** - Any MCP server you build

All external tools integrate with [[Help/Extending/Event Hooks|event hooks]], so you can filter, transform, and audit them.

## Quick Start

Add to your `Config.toml`:

```toml
[[gateway.servers]]
name = "github"
prefix = "gh_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_TOKEN}"
```

Set your token:
```bash
export GITHUB_TOKEN="ghp_your_token"
```

Now you have tools like `gh_search_code`, `gh_get_file_contents`, etc.

## Configuration

### Basic Structure

```toml
[[gateway.servers]]
name = "server_name"           # Unique name
prefix = "prefix_"             # Added to tool names
allowed_tools = ["search_*"]   # Whitelist (optional)
blocked_tools = ["delete_*"]   # Blacklist (optional)
auto_reconnect = true          # Reconnect if disconnected

[gateway.servers.transport]
type = "stdio"                 # or "sse"
command = "npx"
args = ["-y", "@package/name"]

[gateway.servers.transport.env]
TOKEN = "secret"
```

### Transport Types

**stdio** - Runs a command and communicates via stdin/stdout:
```toml
[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

**sse** - Connects to an HTTP endpoint:
```toml
[gateway.servers.transport]
type = "sse"
url = "http://localhost:3000/sse"
auth_header = "Bearer secret"  # Optional
```

## Tool Filtering

Control which tools are exposed:

```toml
# Only expose read operations
allowed_tools = ["search_*", "get_*", "list_*"]

# Block dangerous operations
blocked_tools = ["delete_*", "create_pull_request"]
```

**Blacklist wins** - if a tool matches both allowed and blocked, it's blocked.

Patterns use glob syntax:
- `*` matches any characters
- `?` matches one character

## Prefixes

Prefixes prevent name conflicts:

```toml
[[gateway.servers]]
name = "github"
prefix = "gh_"
# Tools become: gh_search_code, gh_get_file_contents
```

**Always use prefixes** to avoid conflicts between servers.

## Common Configurations

### GitHub

```toml
[[gateway.servers]]
name = "github"
prefix = "gh_"
allowed_tools = ["search_*", "get_*", "list_*"]

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_TOKEN}"
```

### Filesystem (Read-Only)

```toml
[[gateway.servers]]
name = "filesystem"
prefix = "fs_"
blocked_tools = ["write_*", "delete_*"]

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/allowed/path"]
```

### Context7 (Library Docs)

```toml
[[gateway.servers]]
name = "context7"
prefix = "c7_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[gateway.servers.transport.env]
CONTEXT7_API_KEY = "${CONTEXT7_API_KEY}"
```

### Multiple Accounts

```toml
[[gateway.servers]]
name = "github_work"
prefix = "gh_work_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_WORK_TOKEN}"

[[gateway.servers]]
name = "github_personal"
prefix = "gh_personal_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_PERSONAL_TOKEN}"
```

## Using with Hooks

All gateway tools emit events. Use hooks to filter, transform, or audit:

```lua
-- Transform GitHub results. `tool_result` patches the outcome as the model
-- receives it; `pattern` globs the tool name.
crucible.on("tool_result", { pattern = "gh_*", priority = 50 }, function(ctx, event)
    return { result = summarise(event.result) }
end)

-- Audit external access. Handlers may call async APIs directly; there is no
-- custom-event emit on this path.
crucible.on("tool_result", { pattern = "fs_*", priority = 200 }, function(ctx, event)
    cru.log("info", string.format(
        "external access: session=%s tool=%s", tostring(ctx.session_id), event.tool))
end)
```

## Security

### Token Management

Never commit tokens to your config:

```toml
# Use environment variable reference
[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_TOKEN}"
```

Then set in your shell:
```bash
export GITHUB_TOKEN="ghp_actual_token"
```

### Principle of Least Privilege

Only expose tools you need:

```toml
allowed_tools = ["search_*", "get_*", "list_*"]
blocked_tools = ["delete_*", "create_*", "update_*"]
```

### Validation Hooks

`allowed_tools` and `blocked_tools` filter by tool name. To gate on the
*arguments* — which the config cannot see — cancel from a hook:

```lua
crucible.on("pre_tool_call", { pattern = "db_*", priority = 5 }, function(ctx, event)
    local query = event.args and event.args.query or ""
    if query:upper():find("DROP ") then
        return { cancel = true, reason = "DROP statements are blocked" }
    end
end)
```

Cancel rather than edit: `pre_tool_call` ignores a returned argument table, so a
handler that "sanitises" `event.args` and returns it looks like it worked while
the original query still runs. To run a checked version instead, return
`{ handled = true, result = ... }` and issue the call yourself. See
[[Help/Extending/Event Hooks|Event Hooks]] for the full set of return conventions.

## Runtime Behavior

### Startup

When you start a chat session (`cru chat`), Crucible connects to all configured MCP servers. The TUI displays real connection status — you'll see which servers are connected, pending, or failed.

Use `:mcp` in the TUI to view live server status at any time.

### Auto-Reconnect

If a server disconnects (network issues, server restart, etc.), Crucible automatically attempts to reconnect when `auto_reconnect = true` (the default). The reconnect loop runs in the background — no user action needed.

### Tool Injection

Gateway tools are dynamically injected into the agent at session creation via `McpProxyTool`. Tools appear with their configured prefix (e.g., `gh_search_code`) and are available alongside built-in tools.

The daemon manages gateway connections through a shared `McpGatewayManager`, so all sessions share the same server connections.

### SSE Keepalive

For SSE transport connections, a 30-second keepalive ping prevents idle timeouts. This is automatic and requires no configuration.

## Progressive Tool Disclosure

Every MCP tool you attach costs context: its name, description, and JSON
schema are sent on every turn. As you connect more servers, that overhead
grows and crowds out the conversation.

Crucible handles this automatically. When the internal agent's tool schemas
would exceed **15% of the effective context budget**, the gateway (user MCP)
tools are *deferred*: they are dropped from the request and replaced by a
small discovery bridge, while the kiln and workspace tools stay attached. The
agent is told how many tools were deferred and reaches them through three
built-in tools:

- **`discover_tools`** — search available tools by name, description, or source.
- **`get_tool_schema`** — fetch a specific tool's full input schema.
- **`invoke_tool`** — call a tool by name with an `args` object.

`invoke_tool` is unwrapped to the real tool *before* hooks and permission
checks run, so `pre_tool_call` handlers, permission prompts, and the TUI all
see the actual tool name — not `invoke_tool`.

**Plan mode disables upstream MCP tools entirely.** Because Crucible can't
tell which upstream tools mutate state, plan mode fails closed: gateway tools
are never attached (on either the direct or the deferred path), and
`invoke_tool` refuses to call anything outside the read-only plan tool set.
Plan mode stays limited to kiln and read-only workspace tools.

This is automatic and needs no configuration. Sessions with a modest tool set
behave exactly as before (all schemas attached, no bridge).

## Troubleshooting

**Server won't start:**
- Check the command exists (`npx`, `node`, etc.)
- Verify environment variables are set
- Look for errors in Crucible logs

**No tools appear:**
- Check `allowed_tools` patterns match
- Verify server started successfully
- Check for `tool:discovered` events

**Tools not working:**
- Verify prefixed names (use `gh_search_code` not `search_code`)
- Check token permissions
- Review hook patterns

## See Also

- [[Help/Extending/Event Hooks]] - Processing gateway events
- [[Help/Extending/Custom Tools]] - Creating your own tools
- [[Help/Concepts/Agents & Protocols]] - MCP explained
- [[Configuration]] - Full config reference
