---
title: "Model Context Protocol (MCP)"
description: "Specification reference for the Model Context Protocol (MCP) — the standard Crucible uses for tool exposure and agent integration"
---

The Model Context Protocol is an open standard for connecting AI models to external tools and data sources. It defines a uniform interface so any AI agent can discover, call, and receive results from any compatible server.

**Key facts:**

- Created by Anthropic, now adopted across the AI tool ecosystem
- Specification: [modelcontextprotocol.io](https://modelcontextprotocol.io)
- Transport: stdio (subprocess) or HTTP+SSE (remote servers)
- Message format: JSON-RPC 2.0
- Two roles: **Server** (exposes capabilities) and **Client** (consumes capabilities)
- Crucible is both: an MCP server (exposing kiln tools) and an MCP client (connecting to external servers)

## Architecture

MCP follows a client-server model. A client (typically an AI agent or its host) connects to one or more servers. Each server exposes a combination of three primitive types:

```
MCP Client (AI Agent)
├── Calls tools     → MCP Server
├── Reads resources → MCP Server
└── Uses prompts    → MCP Server

MCP Server (Crucible or external)
├── tools:     Callable actions with typed parameters
├── resources: Readable data addressed by URI
└── prompts:   Reusable prompt templates
```

A single client can connect to multiple servers simultaneously. Crucible's [MCP Gateway](../extending/mcp-gateway/) does exactly this, aggregating tools from many upstream servers into a unified namespace.

## Server Primitives

Servers expose capabilities through three primitive types. Tools are the most widely used; resources and prompts see less adoption in practice but fill important roles.

### Tools

Tools are callable functions with typed input parameters and structured output. They're the primary way agents take actions through MCP.

Each tool declares:

- **Name**: unique identifier (e.g. `semantic_search`)
- **Description**: human-readable explanation of what the tool does
- **Input schema**: JSON Schema defining accepted parameters
- **Output**: structured result returned after execution

Tools can have side effects. A `create_note` tool writes to disk. A `semantic_search` tool queries a vector database. The server executes the tool and returns the result; the client never runs tool logic directly.

Example tool definition (conceptual):

```json
{
  "name": "semantic_search",
  "description": "Search notes using semantic similarity",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": { "type": "string" },
      "limit": { "type": "integer", "default": 10 }
    },
    "required": ["query"]
  }
}
```

### Resources

Resources are readable data sources addressed by URI. They let agents pull context without calling tools.

- URI-addressed (e.g. `kiln://notes/My Note`, `file:///path/to/data.csv`)
- Can be static (read once) or dynamic (content changes over time)
- Servers can list available resources and support subscriptions for change notifications
- Agents read resources to gather context before acting

Resources differ from tools in that they're read-only and don't trigger side effects. Think of them as "files the agent can see" rather than "actions the agent can take."

### Prompts

Prompts are reusable templates that servers offer to clients. They provide pre-built conversation starters, system prompt fragments, or structured workflows.

- Parameterized: templates accept arguments filled at request time
- Discoverable: clients list available prompts and select which to use
- Composable: a prompt can reference resources or suggest tool usage

Example: a server might offer a "summarize" prompt that accepts a `topic` parameter and returns a system message instructing the agent how to summarize notes on that topic.

## Client Primitives

Clients can expose capabilities back to the server. These reverse-direction primitives enable richer interactions.

### Sampling

Sampling lets a server request text generation from the client's LLM. The server sends a `sampling/createMessage` request with messages and generation parameters. The client passes this to its model and returns the completion.

This is powerful: it means an MCP server can prompt an LLM without having direct API access to one. The server delegates generation to whatever model the client is using.

Use cases:
- Server-side prompt chains that need LLM reasoning at intermediate steps
- Agentic workflows where the server orchestrates multi-step generation
- Tool servers that need to interpret or transform results using natural language

### Elicitation

Elicitation lets a server ask the user for input through the client. The server sends an `elicitation/create` request with a schema describing what information it needs. The client displays this to the user and returns their response.

This enables interactive server behaviors: confirmation dialogs, choice selection, credential entry, or any situation where the server needs human input to proceed.

### Roots

Roots let the client declare filesystem directories it has access to. The server uses this information to scope resource access and tool behavior to appropriate paths.

## Transport

MCP supports two transport mechanisms. Both use the same JSON-RPC 2.0 message format, so the protocol logic is identical regardless of transport.

### stdio

The server runs as a subprocess. The client spawns it and communicates over stdin/stdout. This is the default for local tools and the most common transport in practice.

```
Client spawns: npx @modelcontextprotocol/server-github
Client writes JSON-RPC to server's stdin
Server writes JSON-RPC to stdout
```

### HTTP+SSE

The server runs as an HTTP endpoint. The client connects via Server-Sent Events for streaming responses and sends requests as HTTP POST. This transport suits remote or shared servers.

```
Server listens: http://localhost:3000/sse
Client connects via SSE for server→client messages
Client POSTs JSON-RPC for client→server messages
```

## Crucible as MCP Server

Running `cru mcp` starts Crucible's built-in MCP server on stdio. External agents (Claude Desktop, Claude Code, Cursor, etc.) connect to it and gain access to your kiln.

### Available Tools

Crucible exposes 12 tools across three categories:

**Note Tools (6)**

| Tool | Description |
|------|-------------|
| `create_note` | Create a new note in the kiln |
| `read_note` | Read note content with optional line range |
| `read_metadata` | Read note metadata without loading full content |
| `update_note` | Update an existing note |
| `delete_note` | Delete a note from the kiln |
| `list_notes` | List notes in a directory |

**Search Tools (3)**

| Tool | Description |
|------|-------------|
| `semantic_search` | Search notes using semantic similarity |
| `text_search` | Fast full-text search across notes |
| `property_search` | Search notes by frontmatter properties (includes tags) |

**Kiln Tools (3)**

| Tool | Description |
|------|-------------|
| `get_kiln_info` | Get kiln name and file statistics (total_files, markdown_files, total_size_bytes) |

### Connecting External Agents

Add Crucible as an MCP server in Claude Code:

```bash
claude mcp add crucible -- cru mcp
```

Or start the server directly for other clients:

```bash
cru mcp
```

The server communicates over stdio using JSON-RPC 2.0. Any MCP-compatible client can connect.

### Extended Server

Beyond the 12 core tools, Crucible's MCP server also exposes:

- **Lua plugin tools**: Scripts from `plugins/` directories, prefixed with `lua_`
- **Gateway tools**: Tools from upstream MCP servers with configured prefixes

These dynamic tools are discovered at startup and appear alongside the built-in tools.

## Crucible as MCP Client (Gateway)

Crucible can also act as an MCP client, connecting to external MCP servers and aggregating their tools. This is the MCP Gateway.

Configure external servers in your `config.toml` or `mcps.toml`:

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

Crucible spawns each configured server, discovers its tools, and exposes them to agents with the configured prefix. An agent using Crucible sees both kiln tools and gateway tools in a single unified interface.

See [MCP Gateway](../extending/mcp-gateway/) for full configuration details and [mcp](../config/mcp/) for the configuration reference.

## MCP vs ACP

Crucible uses two protocols that serve different purposes. MCP handles tools and data. ACP handles agent lifecycle and sessions.

| Aspect | MCP | ACP |
|--------|-----|-----|
| Purpose | Tool discovery and execution | Agent lifecycle management |
| Direction | Agent calls server for tools | Host controls agent subprocess |
| Transport | stdio or HTTP+SSE | stdio JSON-RPC |
| State | Stateless (per-call) | Session-oriented (multi-turn) |
| Streaming | Not specified | Built-in event subscription |
| Crucible role | Server + Client | Host |

In Crucible's architecture, these protocols compose: ACP manages the agent process and conversation on the outside, while MCP provides the tools the agent calls during that conversation on the inside. See [Agent Client Protocol](./agent-client-protocol/) for the ACP specification reference.

## Protocol Lifecycle

A typical MCP session follows this sequence:

1. **Initialize**: Client connects and exchanges capabilities with the server
2. **Discover**: Client calls `tools/list`, `resources/list`, `prompts/list` to learn what's available
3. **Use**: Client calls `tools/call`, `resources/read`, or `prompts/get` as needed
4. **Repeat**: Steps 2-3 can repeat; the server can notify the client when available tools change
5. **Disconnect**: Client closes the connection

The initialization handshake includes protocol version negotiation and capability advertisement. Both sides declare what they support (tools, resources, prompts, sampling, etc.) so neither side calls unsupported methods.

## See Also

- [Agent Client Protocol](./agent-client-protocol/): ACP spec (uses MCP for tools)
- [Agents & Protocols](./agents-and-protocols/): overview of agent architecture
- [MCP Gateway](../extending/mcp-gateway/): connecting external MCP servers
- [mcp](../config/mcp/): MCP server configuration reference
- [mcp](../config/mcp/): `cru mcp` command reference
