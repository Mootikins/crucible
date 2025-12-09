# MCP Gateway

**Status**: Implemented (client infrastructure, transport pending)
**System**: agents
**Related**: [../plugins/event-system.md](../plugins/event-system.md), [../plugins/hooks.md](../plugins/hooks.md)

## Overview

The MCP Gateway enables Crucible to connect to upstream Model Context Protocol (MCP) servers and expose their tools through the unified event system. This allows Crucible to integrate with external services (GitHub, filesystems, databases, etc.) while maintaining consistent hook processing and event-driven architecture.

**Key Features**:
- **Multiple transports**: stdio (subprocess) and SSE (HTTP)
- **Tool filtering**: Whitelist/blacklist patterns for tool selection
- **Namespace prefixing**: Add prefixes to avoid name conflicts
- **Event integration**: All tool calls flow through the event system
- **Auto-reconnect**: Automatic reconnection on disconnection
- **Hot-reload**: Dynamic tool discovery from upstream servers

## Architecture

```text
┌──────────────────────────────────────────────────────────┐
│                    Crucible MCP Server                    │
│  (Serves: kiln_*, just_*, rune_*, upstream_gh_*, etc.)   │
└───────────────────────┬──────────────────────────────────┘
                        │
                        ▼
            ┌───────────────────────┐
            │   ExtendedMcpServer   │
            │  - Kiln tools         │
            │  - Just tools         │
            │  - Rune tools         │
            │  - Upstream tools  ◄──┼─────┐
            └───────────────────────┘     │
                        │                 │
                        ▼                 │
            ┌───────────────────────┐     │
            │      EventBus         │     │
            │  - tool:before        │     │
            │  - tool:after         │     │
            │  - tool:discovered    │     │
            └───────────────────────┘     │
                                          │
                ┌─────────────────────────┘
                │
                ▼
    ┌───────────────────────────┐
    │   McpGatewayManager       │
    │  - Manages upstream       │
    │    connections            │
    │  - Routes tool calls      │
    └──────────┬────────────────┘
               │
       ┌───────┴────────┬───────────────┐
       ▼                ▼               ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│   GitHub    │  │ Filesystem  │  │   Custom    │
│ MCP Client  │  │ MCP Client  │  │ MCP Client  │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       ▼                ▼                ▼
 (stdio/SSE)      (stdio/SSE)      (stdio/SSE)
       │                │                │
       ▼                ▼                ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│  @mcp/      │  │  @mcp/      │  │   Custom    │
│  server-    │  │  server-    │  │   MCP       │
│  github     │  │  filesystem │  │   Server    │
└─────────────┘  └─────────────┘  └─────────────┘
```

## Configuration

MCP Gateway configuration is defined in `~/.config/crucible/config.toml`:

### Basic Configuration

```toml
[[gateway.servers]]
name = "github"
prefix = "gh_"
auto_reconnect = true

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_your_token_here"
```

### Complete Configuration Example

```toml
# GitHub MCP Server (stdio)
[[gateway.servers]]
name = "github"
prefix = "gh_"
allowed_tools = ["search_*", "get_*", "list_*"]
blocked_tools = ["delete_*", "create_pull_request"]
auto_reconnect = true

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_your_token_here"

# Filesystem MCP Server (stdio)
[[gateway.servers]]
name = "filesystem"
prefix = "fs_"
auto_reconnect = true

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/allowed/path"]

# Custom MCP Server (SSE)
[[gateway.servers]]
name = "custom"
prefix = "custom_"

[gateway.servers.transport]
type = "sse"
url = "http://localhost:3000/sse"
auth_header = "Bearer secret_token"
```

## Configuration Reference

### Server Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | String | ✅ Yes | - | Unique identifier for this upstream |
| `prefix` | String | ❌ No | None | Prefix for tool names (e.g., "gh_") |
| `allowed_tools` | Array[String] | ❌ No | All | Whitelist of tool patterns |
| `blocked_tools` | Array[String] | ❌ No | None | Blacklist of tool patterns |
| `auto_reconnect` | Boolean | ❌ No | `true` | Reconnect on disconnection |
| `transport` | Object | ✅ Yes | - | Transport configuration |

### Transport: stdio

Spawns a subprocess and communicates via stdin/stdout.

```toml
[gateway.servers.transport]
type = "stdio"
command = "npx"                                    # Command to execute
args = ["-y", "@modelcontextprotocol/server-foo"] # Arguments
env = { TOKEN = "secret" }                        # Environment variables
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | String | ✅ Yes | Must be `"stdio"` |
| `command` | String | ✅ Yes | Command to execute |
| `args` | Array[String] | ❌ No | Command arguments |
| `env` | Object | ❌ No | Environment variables (key-value pairs) |

### Transport: SSE

Connects to an HTTP endpoint using Server-Sent Events.

```toml
[gateway.servers.transport]
type = "sse"
url = "http://localhost:3000/sse"
auth_header = "Bearer token123"  # Optional
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | String | ✅ Yes | Must be `"sse"` |
| `url` | String | ✅ Yes | HTTP endpoint URL |
| `auth_header` | String | ❌ No | Authorization header value |

## Tool Filtering

### Allowed Tools (Whitelist)

Only tools matching at least one pattern are exposed:

```toml
allowed_tools = [
    "search_*",           # All search tools
    "get_file_contents",  # Specific tool
    "list_*"              # All list tools
]
```

**If `allowed_tools` is not specified**, all tools are allowed (unless blocked).

### Blocked Tools (Blacklist)

Tools matching any pattern are filtered out:

```toml
blocked_tools = [
    "delete_*",      # All delete operations
    "dangerous",     # Specific tool
    "*_admin"        # All admin tools
]
```

**Blacklist takes precedence**: If a tool matches both allowed and blocked patterns, it is blocked.

### Pattern Matching

Patterns support glob syntax:
- `*` - Match any sequence of characters
- `?` - Match exactly one character
- Literal strings for exact matches

Examples:
```toml
allowed_tools = [
    "search_*",              # search_code, search_issues, etc.
    "get_file_contents",     # Exact match
    "list_*_files"           # list_directory_files, list_repo_files
]

blocked_tools = [
    "delete_*",              # All delete operations
    "*_private",             # Tools ending in _private
    "admin_*"                # All admin tools
]
```

## Namespace Prefixing

Prefixes prevent name conflicts between multiple upstream servers:

```toml
# Without prefix: tool name = "search_code"
[[gateway.servers]]
name = "github"
# Tools: search_code, get_file_contents, etc.

# With prefix: tool name = "gh_search_code"
[[gateway.servers]]
name = "github"
prefix = "gh_"
# Tools: gh_search_code, gh_get_file_contents, etc.
```

**Recommendation**: Always use prefixes to avoid conflicts between:
- Different upstream servers with similar tools
- Upstream tools and local tools (just_*, rune_*, kiln_*)

## Event Integration

All upstream tool calls flow through the event system:

### Event Flow

```text
1. Tool call request
   ↓
2. Find upstream client
   ↓
3. Emit tool:before
   - Pattern: "gh_search_code"
   - Source: "upstream:github"
   - Hooks can modify args or cancel
   ↓
4. Check if cancelled
   ├─ Yes → Return error
   └─ No → Continue
   ↓
5. Call upstream tool via transport
   ↓
6. Emit tool:after (or tool:error)
   - Pattern: "gh_search_code"
   - Source: "upstream:github"
   - Hooks can transform result
   ↓
7. Return result to caller
```

### Server Connection Events

When an upstream server connects:

```rust
Event {
    event_type: "mcp:attached",
    identifier: "github",  // server name
    payload: {
        "name": "github",
        "server": {
            "name": "@modelcontextprotocol/server-github",
            "version": "0.2.0",
            "protocol_version": "2024-11-05"
        },
        "transport": {
            "type": "stdio",
            "command": "npx"
        }
    },
    source: "upstream:github"
}
```

### Tool Discovery Events

Each discovered tool emits `tool:discovered`:

```rust
Event {
    event_type: "tool:discovered",
    identifier: "gh_search_code",  // prefixed name
    payload: {
        "name": "gh_search_code",
        "original_name": "search_code",
        "description": "Search code on GitHub",
        "input_schema": { /* JSON Schema */ },
        "upstream": "github"
    },
    source: "upstream:github"
}
```

Hooks can process these events to:
- Filter unwanted tools (cancel event)
- Add metadata (tags, categories)
- Log tool registration
- Emit follow-up events

## Usage Examples

### Example 1: GitHub Integration

**Configuration**:
```toml
[[gateway.servers]]
name = "github"
prefix = "gh_"
allowed_tools = ["search_*", "get_file_contents", "list_*"]
auto_reconnect = true

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_your_token_here"
```

**Hook to transform results**:
```rune
/// Transform GitHub search results
#[hook(event = "tool:after", pattern = "gh_search_*", priority = 50)]
pub fn transform_github_results(ctx, event) {
    let result = event.payload.result;

    // Extract relevant fields
    if let Some(content) = result.content {
        let items = content[0].text;
        let summary = summarize_github_results(items);

        content[0].text = summary;
    }

    event
}
```

### Example 2: Filesystem Access

**Configuration**:
```toml
[[gateway.servers]]
name = "filesystem"
prefix = "fs_"
blocked_tools = ["delete_*", "write_*"]  # Read-only

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/documents"]
```

**Hook to log file access**:
```rune
/// Audit filesystem access
#[hook(event = "tool:after", pattern = "fs_*", priority = 200)]
pub fn audit_file_access(ctx, event) {
    ctx.emit_custom("audit:filesystem_access", #{
        tool: event.identifier,
        timestamp: event.timestamp_ms,
        source: event.source,
    });

    event
}
```

### Example 3: Multiple GitHub Accounts

**Configuration**:
```toml
# Work account
[[gateway.servers]]
name = "github_work"
prefix = "gh_work_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_work_token"

# Personal account
[[gateway.servers]]
name = "github_personal"
prefix = "gh_personal_"

[gateway.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_personal_token"
```

Now you have:
- `gh_work_search_code`
- `gh_personal_search_code`

### Example 4: Custom MCP Server

**Configuration**:
```toml
[[gateway.servers]]
name = "database"
prefix = "db_"

[gateway.servers.transport]
type = "sse"
url = "http://localhost:8080/mcp/sse"
auth_header = "Bearer database_secret"
```

**Hook to validate queries**:
```rune
/// Validate database queries
#[hook(event = "tool:before", pattern = "db_query", priority = 5)]
pub fn validate_query(ctx, event) {
    let query = event.payload.query;

    // Block dangerous operations
    if query.contains("DROP ") || query.contains("DELETE ") {
        event.cancelled = true;
        ctx.set("error", "Dangerous query blocked");
    }

    event
}
```

## Runtime API

### McpGatewayManager

```rust
use crucible_rune::mcp_gateway::{McpGatewayManager, UpstreamConfig, TransportConfig};
use crucible_rune::event_bus::EventBus;

// Create manager
let bus = EventBus::new();
let mut manager = McpGatewayManager::new(bus);

// Add upstream client
let config = UpstreamConfig {
    name: "github".to_string(),
    transport: TransportConfig::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@modelcontextprotocol/server-github".to_string()],
        env: vec![("GITHUB_TOKEN".to_string(), token)],
    },
    prefix: Some("gh_".to_string()),
    allowed_tools: None,
    blocked_tools: Some(vec!["delete_*".to_string()]),
    auto_reconnect: true,
};

let client = manager.add_client(config);

// Get client
let client = manager.get_client("github").unwrap();

// List all tools
let tools = manager.all_tools().await;

// Call a tool (automatically routes to correct client)
let result = manager.call_tool("gh_search_code", args).await?;
```

### UpstreamMcpClient

```rust
// Check connection status
if client.is_connected().await {
    println!("Connected to {}", client.name());
}

// Get server info
if let Some(info) = client.server_info() {
    println!("Server: {} v{}", info.name, info.version.unwrap_or_default());
}

// List tools
let tools = client.tools().await;
for tool in tools {
    println!("Tool: {} ({})", tool.prefixed_name, tool.description.unwrap_or_default());
}

// Call tool with events
let result = client.call_tool_with_events("gh_search_code", args).await?;
```

## Troubleshooting

### Connection Issues

**Server won't start**:
- Check `command` exists and is executable
- Verify `args` are correct
- Check environment variables are set
- Look for error output in logs

**SSE connection fails**:
- Verify URL is correct and accessible
- Check `auth_header` if required
- Ensure server supports SSE transport

### Tool Discovery

**No tools appear**:
- Check server started successfully
- Verify tools aren't filtered by `allowed_tools`/`blocked_tools`
- Look for `tool:discovered` events in logs

**Wrong tools exposed**:
- Review `allowed_tools` and `blocked_tools` patterns
- Check pattern syntax (glob, not regex)
- Verify patterns match tool names (not descriptions)

### Event Integration

**Hooks not triggered**:
- Check hook pattern matches prefixed tool name (e.g., `gh_*` not `search_*`)
- Verify hook event type is correct (`tool:after` not `tool:before`)
- Check hook is registered and enabled

**Results not transformed**:
- Verify hook modifies `event.payload.result`
- Check hook priority (should run before return)
- Look for hook errors in logs

## Security Considerations

### Token Management

**Don't commit tokens**:
```toml
# ❌ BAD: Token in config file
[gateway.servers.transport.env]
GITHUB_TOKEN = "ghp_actual_token"

# ✅ GOOD: Use environment variable
[gateway.servers.transport.env]
GITHUB_TOKEN = "${GITHUB_TOKEN}"
```

Then set in shell:
```bash
export GITHUB_TOKEN="ghp_actual_token"
```

### Tool Filtering

**Principle of least privilege**:
```toml
# Only expose read-only operations
allowed_tools = ["search_*", "get_*", "list_*"]
blocked_tools = ["delete_*", "create_*", "update_*"]
```

### Validation Hooks

Add validation before calling upstream tools:

```rune
#[hook(event = "tool:before", pattern = "gh_*", priority = 5)]
pub fn validate_github_args(ctx, event) {
    // Validate rate limits
    // Check permissions
    // Sanitize inputs
    event
}
```

## Performance

### Connection Pooling

Each upstream server maintains a single connection. Multiple tool calls are multiplexed over the same connection.

### Caching

Consider caching upstream results:

```rune
#[hook(event = "tool:before", pattern = "gh_search_*", priority = 10)]
pub fn cache_search_results(ctx, event) {
    let cache_key = compute_key(event.payload);

    if let Some(cached) = get_cache(cache_key) {
        ctx.set("cached_result", cached);
        event.cancelled = true;  // Use cache, skip upstream call
    }

    event
}
```

### Rate Limiting

Implement rate limiting in hooks:

```rune
#[hook(event = "tool:before", pattern = "gh_*", priority = 5)]
pub fn rate_limit_github(ctx, event) {
    if is_rate_limited("github") {
        event.cancelled = true;
        ctx.set("error", "Rate limit exceeded");
    }

    event
}
```

## Future Enhancements

**Planned**:
- Transport implementation (currently infrastructure only)
- Connection retry strategies
- Tool schema validation
- Batch tool execution
- Response streaming

**Under consideration**:
- HTTP transport (non-SSE)
- WebSocket transport
- Tool versioning
- A/B testing upstream servers

## See Also

- [../plugins/event-system.md](../plugins/event-system.md) - Event types and handling
- [../plugins/hooks.md](../plugins/hooks.md) - Writing hooks for upstream tools
- [MCP Specification](https://modelcontextprotocol.io/specification) - Official MCP docs
- `crates/crucible-rune/src/mcp_gateway.rs` - Implementation
- `crates/crucible-config/src/components/gateway.rs` - Configuration schema
