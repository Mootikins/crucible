---
title: Agent Client Protocol
description: Specification reference for the Agent Client Protocol (ACP) — the stdio JSON-RPC protocol Crucible uses to host external AI agents
status: implemented
tags:
  - concept
  - acp
  - protocol
  - reference
aliases:
  - ACP Specification
  - ACP Spec
---

# Agent Client Protocol (ACP)

The Agent Client Protocol is an open protocol for AI agent hosting. It defines how a **host application** spawns, manages, and communicates with external AI agents over a stdio JSON-RPC connection.

**Key facts:**

- Full name: Agent Client Protocol (not "Agent Context Protocol")
- Specification: [agentclientprotocol.com](https://agentclientprotocol.com)
- Source: [github.com/nichochar/agent-client-protocol](https://github.com/nichochar/agent-client-protocol)
- Transport: stdio JSON-RPC over newline-delimited messages (same pattern as LSP)
- Crucible is the **host**. It spawns external agents (Claude Code, OpenCode, Gemini CLI) as subprocesses.
- The agent binary receives a stdio connection; Crucible drives the session lifecycle.

## Three-Layer Architecture

Crucible's agent integration stacks three protocols, each with a distinct role:

```
Crucible (ACP Host)
├── ACP Layer: Manages agent subprocess lifecycle, sessions, streaming
├── Skills Layer: Context injection from knowledge graph
└── MCP Layer: Exposes kiln tools to the agent

External Agent (e.g. Claude Code)
├── Receives ACP connection from Crucible
├── Loads skills context injected by Crucible
└── Calls MCP tools served by Crucible
```

**ACP** controls the agent. **MCP** provides tools to the agent. **Skills** provides knowledge. These layers compose cleanly: ACP manages the session, skills inject relevant context before each turn, and MCP handles tool calls the agent makes during its response.

## Sessions

ACP is session-oriented. Every agent interaction happens within a session that tracks state across multiple turns.

### Lifecycle

1. **Create**: `session.create` initializes a new session, returning a `session_id`
2. **Configure**: `session.configure_agent` sets model, system prompt, tools, and permissions
3. **Interact**: `session.send_message` sends user messages and streams agent responses
4. **Subscribe**: `session.subscribe` opens an event stream for real-time updates
5. **Pause/Resume**: `session.pause` and `session.resume` suspend and restore sessions
6. **End**: `session.end` terminates the session and cleans up resources

Sessions have a `status` field: `active`, `paused`, or `ended`. State persists between messages within a session. The host can resume a paused session without losing conversation history.

### Session Configuration

When configuring an agent, the host provides:

- **Model**: which LLM to use (e.g. `claude-sonnet-4-20250514`, `gpt-4o`)
- **System prompt**: base instructions for the agent
- **Tools**: MCP tool definitions the agent can call
- **Permissions**: capability scoping (see Permissions below)
- **Working directory**: filesystem context for the agent process

## Message Types

| Method | Direction | Description |
|--------|-----------|-------------|
| `session.create` | host → agent | Create new session |
| `session.configure_agent` | host → agent | Set agent configuration |
| `session.send_message` | host → agent | Send user message |
| `session.switch_model` | host → agent | Change model mid-session |
| `session.cancel` | host → agent | Cancel in-progress response |
| `session.subscribe` | host → agent | Subscribe to event stream |
| `session.unsubscribe` | host → agent | Unsubscribe from events |
| `session.pause` | host → agent | Suspend session |
| `session.resume` | host → agent | Resume suspended session |
| `session.end` | host → agent | Terminate session |
| event: `message` | agent → host | Streaming response chunk |
| event: `thinking` | agent → host | Agent thinking/reasoning content |
| event: `tool_call` | agent → host | Agent requesting tool use |
| event: `tool_result` | agent → host | Tool execution result |
| event: `done` | agent → host | Turn complete |
| event: `error` | agent → host | Error notification |

All messages use JSON-RPC 2.0 format. Each request carries a unique numeric ID for response matching.

## Streaming

ACP supports streaming responses through its event subscription model. The flow works like this:

1. Host calls `session.subscribe` to open an event channel
2. Host sends `session.send_message` with the user's input
3. Agent emits events as it processes:
   - `message` chunks: incremental text of the response
   - `thinking` chunks: reasoning content (if the model supports it)
   - `tool_call` events: when the agent wants to use a tool, with name and arguments
   - `tool_result` events: results returned after tool execution
   - `done`: signals the turn is complete
4. Host renders chunks in real-time (TUI streaming, web SSE, etc.)

The streaming callback can return `false` to cancel the current response, which maps to `session.cancel` on the wire.

## Permissions

ACP defines capability scoping so hosts can restrict what agents are allowed to do. Crucible enforces permissions before forwarding tool calls to MCP.

Available permissions:

| Permission | Grants |
|------------|--------|
| `read_kiln` | Read notes from the kiln |
| `write_kiln` | Create or modify notes |
| `run_tools` | Execute MCP tools |
| `web_search` | Internet access |

Permissions are set during `session.configure_agent`. The host validates each tool call against the agent's granted permissions. A `write_kiln` call from an agent that only has `read_kiln` will be rejected before it reaches the MCP layer.

Custom agent profiles can specify capabilities in `crucible.toml`:

```toml
[acp.agents.read-only-claude]
extends = "claude"
capabilities = ["read_kiln", "run_tools"]
```

## Protocol Details

### Handshake

When Crucible spawns an agent subprocess, it performs a version handshake. The current protocol wire version is `1`. Crucible tracks ACP spec revision `0.10.6` internally for compatibility checks. Versions are compatible if they share the same major version number.

### Transport Configuration

The ACP transport layer has configurable parameters:

- **Timeout**: 30 seconds default per operation
- **Max message size**: 10 MB default
- **Debug logging**: toggleable per session

### Error Handling

Errors propagate as JSON-RPC error responses with standard error codes. The `error` event type notifies the host of asynchronous failures during streaming. Crucible surfaces these in the TUI as inline error messages.

## Built-in Agent Profiles

Crucible ships with profiles for common ACP-compatible agents:

| Agent | Command | Install |
|-------|---------|---------|
| OpenCode | `opencode acp` | `go install github.com/grafana/opencode@latest` |
| Claude Code | `npx @zed-industries/claude-agent-acp` | `npm install -g @zed-industries/claude-agent-acp` |
| Gemini CLI | `gemini` | `npm install -g gemini-cli` |
| Codex | `npx @zed-industries/codex-acp` | `npm install -g @zed-industries/codex-acp` |
| Cursor | `cursor-acp` | `npm install -g cursor-acp` |

Agent discovery uses parallel probing: Crucible checks all known agents concurrently via `which` + `--version`, caches the result, and falls back through the priority list if the preferred agent isn't available.

## Custom Agent Profiles

Define custom profiles in `crucible.toml` using `extends` to inherit from a built-in:

```toml
[acp.agents.my-claude]
extends = "claude"
env = { ANTHROPIC_BASE_URL = "http://localhost:4000" }

[acp.agents.my-agent]
command = "/usr/local/bin/my-agent"
args = ["--mode", "acp"]
```

Then use with: `cru chat -a my-claude`

## Crucible as ACP Host

When you run `cru chat -a claude`, Crucible:

1. **Discovers** the agent binary (parallel probe of known agents)
2. **Spawns** the agent as a subprocess with stdio pipes
3. **Handshakes** over JSON-RPC to negotiate protocol version
4. **Creates** an ACP session and configures the agent
5. **Injects** skill context and Precognition results (semantic search hits from your kiln)
6. **Streams** the conversation through the TUI or web UI
7. **Routes** all tool calls through Crucible's MCP server, enforcing permissions

The agent never touches your kiln directly. Every file read, search, and write goes through Crucible's tool layer, giving you full control over what the agent can access.

### Precognition Integration

Before each turn, Crucible runs [[Help/Concepts/Semantic Search|semantic search]] against your kiln using the user's message as a query. Relevant note fragments are injected into the agent's context alongside any loaded [[Help/Concepts/Agent Skills|skills]]. This means the agent has access to your knowledge without you manually searching for context.

## Crucible as ACP Agent

Crucible also implements the *other* side of the protocol: the **agent** role. Run

```bash
cru acp
```

and Crucible speaks ACP on stdin/stdout, so any ACP host (Zed, JetBrains, Neovim, marimo — or another Crucible instance) can drive it as a knowledge-grounded agent. Point your editor's ACP agent configuration at the `cru acp` command.

What the host gets is the ordinary internal Crucible agent, exposed through a different front door:

1. **`initialize`** — Crucible advertises protocol v1, text prompts, and `loadSession` support.
2. **`session/new`** — creates a normal daemon session (`type = chat`, `agent = internal`) with the host-supplied `cwd` as the workspace. It shows up in `cru session list` and persists like any other session.
3. **`session/prompt`** — the user's message is forwarded to the daemon; the daemon's event stream is translated into ACP `session/update` notifications: text deltas become agent message chunks, thinking becomes thought chunks, and tool calls/results become `tool_call` / `tool_call_update` entries (with a coarse tool-kind for host icons).
4. **`session/request_permission`** — when the daemon needs approval to run a tool, Crucible surfaces it to the host as a permission request with Allow/Reject (once/always) options and maps the choice back to Crucible's permission model.
5. **`session/cancel`** — forwarded to the daemon to stop the turn; the prompt returns `stop_reason = cancelled`.
6. **`session/load`** — resumes an existing daemon session so the host can continue a prior conversation.

Because sessions are real daemon sessions, Precognition, kiln tools, and session digests all apply automatically — the host does not need to know anything about Crucible's knowledge graph.

**Not yet wired (v1):** session modes, model listing/switching and forking over ACP, host-side filesystem/terminal capabilities (tools run daemon-side exactly as for internal sessions), and authentication (none advertised). Non-permission interaction primitives (free-form questions, panels) have no ACP analogue and are auto-declined.

### Dogfood: Crucible hosting Crucible

Because Crucible is both host and agent, you can point one instance at another. Add a profile that runs `cru acp`:

```toml
[acp.agents.crucible]
command = "cru"
args = ["acp"]
```

Then `cru chat -a crucible` runs a full round trip: the host Crucible spawns `cru acp`, which serves the internal agent back over the protocol. This is the end-to-end test of both roles at once. (See the "Manual verification" note in the ACP agent-mode module for a scripted stdio recipe.)

## Comparison with MCP

| Aspect | ACP | MCP |
|--------|-----|-----|
| Purpose | Agent lifecycle and sessions | Tool discovery and execution |
| Direction | Host controls agent | Agent calls tools |
| Transport | stdio JSON-RPC (subprocess) | stdio or SSE |
| State | Session-oriented (multi-turn) | Stateless (per-call) |
| Streaming | Built-in event subscription | Not specified |

ACP and MCP are complementary. ACP manages the agent process and conversation. MCP provides the tools the agent uses during that conversation. In Crucible, both protocols work together: ACP on the outside (host ↔ agent), MCP on the inside (agent ↔ tools).

## See Also

- [[Help/Concepts/Agents & Protocols]]: overview of agent architecture
- [[Help/Concepts/Agent Skills]]: skills specification reference
- [[Help/Extending/MCP Gateway]]: connecting external MCP servers
- [[Help/CLI/chat]]: chat command reference
