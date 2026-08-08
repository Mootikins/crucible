---
title: "MCP Configuration"
description: Configure upstream MCP servers for tool aggregation
tags:
  - reference
  - config
  - mcp
status: implemented
---

# MCP Configuration

Configure upstream MCP (Model Context Protocol) servers to aggregate external tools into Crucible.

## Overview

The MCP Gateway allows Crucible to connect to multiple upstream MCP servers, aggregating their tools under prefixed namespaces. This enables:

- Connecting to official MCP servers (GitHub, filesystem, etc.)
- Running multiple servers simultaneously
- Tool filtering with glob patterns
- Automatic tool prefixing to avoid name collisions

## Configuration File

Add to `~/.config/crucible/config.toml`:

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.servers.transport.env]
GITHUB_TOKEN = "{env:GITHUB_TOKEN}"
```

## Server Configuration

### Basic Structure

Each upstream server requires:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique identifier for this upstream |
| `prefix` | Yes | Prefix for all tools (e.g., `gh_` → `gh_search_code`) |
| `transport` | Yes | Connection configuration (stdio; SSE parses but is not implemented) |
| `allowed_tools` | No | Whitelist of tool patterns (glob) |
| `blocked_tools` | No | Blacklist of tool patterns (glob) |
| `auto_reconnect` | No | Reconnect on disconnect (default: true) |
| `timeout_secs` | No | Tool call timeout (default: 30) |

### Prefix Rules

Prefixes must:
- Be non-empty
- Contain only alphanumeric characters and underscores
- End with an underscore (`_`)
- Be unique across all configured upstreams

Valid: `gh_`, `fs_`, `docker_v2_`.

Invalid: `""` (empty), `gh` (no trailing underscore), `my-server_` (contains a hyphen).

## Transport Types

### Stdio (Subprocess)

Spawn an MCP server as a subprocess:

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.servers.transport.env]
GITHUB_TOKEN = "{env:GITHUB_TOKEN}"
```

**Fields:**
- `command` - Executable to run
- `args` - Command arguments (optional)
- `env` - Environment variables (optional)

### SSE (Server-Sent Events)

> **Not yet implemented.** The config shape below parses, but connecting fails with
> "SSE transport not yet implemented"
> (`crates/crucible-daemon/src/tools/mcp_gateway.rs`). Use stdio.

Connect to an HTTP-based MCP server:

```toml
[[mcp.servers]]
name = "remote"
prefix = "remote_"

[mcp.servers.transport]
type = "sse"
url = "http://localhost:3000/sse"
auth_header = "Bearer your-secret-token"
```

**Fields:**
- `url` - SSE endpoint URL
- `auth_header` - Authorization header value (optional)

## Tool Filtering

Control which tools are exposed using glob patterns:

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"
allowed_tools = ["search_*", "get_*", "list_*"]
blocked_tools = ["delete_*", "*_dangerous"]

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

**Filter behavior** (`mcp_gateway.rs`, `is_tool_allowed`):
1. `blocked_tools` is checked first and wins — a tool matching both lists is blocked
2. If `allowed_tools` is set, a tool must match it to be included
3. With neither set, every tool from the upstream is exposed

**Glob patterns:**
- `*` matches any characters
- `search_*` matches `search_code`, `search_issues`, etc.
- `*_repo` matches `get_repo`, `create_repo`, etc.

## Examples

### GitHub MCP Server

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"
timeout_secs = 60

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.servers.transport.env]
GITHUB_TOKEN = "{env:GITHUB_TOKEN}"
```

### Filesystem MCP Server

```toml
[[mcp.servers]]
name = "filesystem"
prefix = "fs_"
allowed_tools = ["read_*", "list_*"]  # Read-only access

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/dir"]
```

### Multiple Servers

```toml
# GitHub
[[mcp.servers]]
name = "github"
prefix = "gh_"

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.servers.transport.env]
GITHUB_TOKEN = "{env:GITHUB_TOKEN}"

# Filesystem
[[mcp.servers]]
name = "filesystem"
prefix = "fs_"

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~"]

# Custom local server
[[mcp.servers]]
name = "custom"
prefix = "my_"
auto_reconnect = false

[mcp.servers.transport]
type = "stdio"
command = "/usr/local/bin/my-mcp-server"
```

### Separate Configuration File

To keep servers out of the main config, point the `mcp` key at a file with a value
reference. The referenced `.toml` is parsed and substituted in place, so its top level is
the body of `[mcp]` — `[[servers]]`, not `[[mcp.servers]]`. See
[[Help/Configuration#Value References]] for `{file:}` and `{dir:}`.

## How Tools Appear

When connected, upstream tools are prefixed and available to agents:

| Upstream Tool | Prefixed Name |
|---------------|---------------|
| `search_code` | `gh_search_code` |
| `get_repo` | `gh_get_repo` |
| `read_file` | `fs_read_file` |
| `list_directory` | `fs_list_directory` |

Agents see prefixed names, ensuring no collisions between upstreams.

## Troubleshooting

### "Connection failed"

Check the MCP server command works standalone:

```bash
npx -y @modelcontextprotocol/server-github
```

### "Invalid prefix"

Ensure prefix:
- Ends with `_`
- Contains only alphanumeric characters and underscores
- Is unique across all servers

### "Tool timed out"

Increase the timeout on that server:

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"
timeout_secs = 120

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

### "Prefix collision"

Each upstream must have a unique prefix. Check for duplicates in your config.

## See Also

- [[Help/Config/workspaces]] - Workspace configuration
- [[Help/Extending/Creating Plugins]] - Plugin development
- [MCP Specification](https://modelcontextprotocol.io/)
