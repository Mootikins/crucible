---
title: "Agent Client Protocol (ACP)"
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
- Source: [github.com/agentclientprotocol/agent-client-protocol](https://github.com/agentclientprotocol/agent-client-protocol)
- Crucible uses the `agent-client-protocol` crate, version 2.0.0 (wire schema 1.5.0)
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

ACP is session-oriented. Every agent interaction happens within a session that tracks state across multiple turns. State persists between prompts within a session, and a host can load a prior session to continue its conversation.

Don't confuse ACP sessions with Crucible's *internal* daemon sessions. The daemon exposes its own JSON-RPC surface (`session.create`, `session.send_message`, `session.subscribe`, ...) over the Unix socket — that is Crucible's private client protocol, not ACP. When Crucible hosts or serves ACP, it bridges between the two.

## Wire Methods

The ACP wire protocol is JSON-RPC 2.0 over stdio. The methods that matter in practice:

| Method | Direction | Description |
|--------|-----------|-------------|
| `initialize` | client → agent | Version handshake and capability exchange |
| `session/new` | client → agent | Create a new session (with working directory) |
| `session/prompt` | client → agent | Send a user prompt; the response ends the turn with a stop reason |
| `session/load` | client → agent | Resume a previously created session, with a replay of its history |
| `session/resume` | client → agent | Continue an earlier session without a replay of its history |
| `session/set_config_option` | client → agent | Set one session config option; a model switch uses the `model_config` category |
| `session/list` | client → agent | List the sessions that the agent stores (Crucible does not send it yet) |
| `session/delete` | client → agent | Delete one stored session (Crucible does not send it yet) |
| `session/cancel` | client → agent | Cancel the in-progress turn |
| `session/close` | client → agent | Close a session and release its resources |
| `session/update` | agent → client | Streaming notification: message chunks, thought chunks, `tool_call` / `tool_call_update` entries |
| `session/request_permission` | agent → client | Ask the client to approve a tool call (allow/reject, once/always) |
| `elicitation/create` | agent → client | Ask the user a free-form question; Crucible refuses it with `-32601` today |

## Streaming

A prompt turn streams through `session/update` notifications:

1. Client sends `session/prompt` with the user's input
2. The agent emits `session/update` notifications as it works: incremental message text, thought chunks (if the model exposes reasoning), and `tool_call` / `tool_call_update` entries as tools start and finish
3. If a tool needs approval, the agent sends `session/request_permission` and waits for the client's answer
4. The `session/prompt` response returns with a stop reason (`end_turn`, `cancelled`, ...) when the turn completes

The client renders updates in real time (TUI streaming, web SSE, etc.) and can cancel mid-turn with `session/cancel`.

### Tool calls are an upsert

The agent reports tool calls as an upsert keyed by `toolCallId`. The spec sets no order
between `tool_call` and `tool_call_update`: an update can arrive before its call, and some
agents send only updates. To model this, Crucible keeps one tool-call table per turn.
Crucible announces each call once, at the first update that carries a name. At the end of
the turn, Crucible announces each nameless call under the label "Unnamed tool", and closes
each call that has no completion with an error that names the stop reason.

## Permissions

Crucible does **not** enforce a per-capability ACP permission model. Tool calls from a hosted agent go through the same permission gate as every other session — permission patterns, agent-card tool policy, Lua hooks, and the `permissions` config (see [[Help/Concepts/Permission Precedence]]) — and interactive approvals surface as ACP `session/request_permission` requests.

The `capabilities` field on an `acp.agents.*` profile is parsed and stored but **never read for enforcement**. Setting `capabilities = ["read_kiln"]` on a profile has no effect today; do not rely on it to restrict an agent. Use the permission system instead.

## Protocol Details

### Handshake

When Crucible spawns an agent subprocess, it performs a version handshake via `initialize`. The current protocol wire version is `1`. Versions are compatible if they share the same major version number.

### Transport Configuration

Timeouts and limits under `acp` in `init.lua`:

- `streaming_timeout_minutes` (default 15) — how long a streaming turn may go without completing before it is cut off.
- The removed fields `session_timeout_minutes` and `max_message_size_mb` still load without an error; the values are ignored.

### Error Handling

Errors propagate as JSON-RPC error responses with standard error codes. The `error` event type notifies the host of asynchronous failures during streaming. Crucible surfaces these in the TUI as inline error messages.

## Built-in Agent Profiles

Crucible ships with profiles for common ACP-compatible agents:

| Profile | Command | Install |
|---------|---------|---------|
| `opencode` | `opencode acp` | `npm install -g opencode-ai@latest` (or `curl -fsSL https://opencode.ai/install \| bash`) |
| `claude` | `npx @agentclientprotocol/claude-agent-acp` | `npm install -g @agentclientprotocol/claude-agent-acp` (bridges to the Claude Code CLI) |
| `gemini` | `gemini` | `npm install -g @google/gemini-cli` |
| `codex` | `npx @agentclientprotocol/codex-acp` | `npm install -g @agentclientprotocol/codex-acp` (bridges to the OpenAI Codex CLI) |
| `cursor` | `cursor-agent acp` | `curl https://cursor.com/install -fsS \| bash` |
| `hermes` | `hermes acp` | `curl -fsSL https://hermes-agent.nousresearch.com/install.sh \| bash` |

`opencode`, `gemini`, `cursor` and `hermes` speak ACP directly; `claude` and `codex` are
bridges that also need the underlying vendor CLI installed. The `@agentclientprotocol/*`
packages are the renamed `@zed-industries/*` ones — npm warns and stops updating the old
names. The Cursor bridge `cursor-acp` on npm is an unrelated third-party package abandoned
at 0.1.0; `cursor-agent acp` is Cursor's own ACP server. Hermes notes: a polished tool's result arrives in
`content` text blocks with no `rawOutput`; permission requests carry a fresh `perm-check-N`
id that matches no announced tool call; `session/close` and `ping` answer `-32601`, which is
not an error. `cru` prints the same install lines when no agent is
found, so if this table ever disagrees with the binary, trust the binary.

Agent discovery uses parallel probing: Crucible checks all known agents concurrently via `which` + `--version`, caches the result, and falls back through the priority list if the preferred agent isn't available.

## Custom Agent Profiles

Define custom profiles in `init.lua` using `extends` to inherit from a built-in:

```lua
cru.config.set({
    acp = {
        agents = {
            ["my-claude"] = {
                extends = "claude",
                env = { ANTHROPIC_BASE_URL = "http://localhost:4000" },
            },
            ["my-agent"] = {
                command = "/usr/local/bin/my-agent",
                args = { "--mode", "acp" },
            },
        },
    },
})
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

After a daemon restart, Crucible sends `session/resume` to continue the agent session.
A `-32601` reply means the agent does not have the method, and a `-32002` reply means the
agent no longer knows the stored session (claude-agent-acp and codex-acp answer this for
a session that never completed a turn, and reaped sessions answer it too): Crucible opens
a new session instead and announces the fallback, because the agent kept none of the
conversation. Any other error reply means the agent has the method and refuses this call
— an agent-side breakage, most often — and the connect fails rather than starting a
session that silently forgets. To switch the model, Crucible sends
`session/set_config_option` with the agent's `model_config` option. At shutdown, Crucible
sends `session/close` when the agent advertises the capability. A `-32601` reply to
`session/close` is not an error.

The agent never touches your kiln directly. Every file read, search, and write goes through Crucible's tool layer, giving you full control over what the agent can access.

### Precognition Integration

On the first message of a session, Crucible runs [[Help/Concepts/Semantic Search|semantic search]] against your kiln using that message as a query. Relevant note fragments are injected into the agent's context alongside any loaded [[Help/Concepts/Agent Skills|skills]]. This means the agent has access to your knowledge without you manually searching for context.

## Session Settings

An ACP session does not have Crucible's settings. The protocol has no
temperature, no token cap and no system prompt field, and the external agent
runs its own turn loop and owns its own history — so a daemon-side iteration
cap, context budget or compaction threshold governs work the daemon does not
do. Crucible refuses those settings on an ACP session rather than storing a
value the agent will never read.

What stays: the mode, the model (when the agent advertises a selector), and
Precognition. Retrieval is the daemon's own work and reaches the agent as
injected prompt text, so it behaves exactly as it does for an internal agent.

`session.list_knobs` reports which settings a session has, and the TUI and the
web UI both draw their controls from it. Support is the session's answer, not
the agent type's: two ACP sessions differ if one agent advertises a model
selector and the other does not.

Changing a setting does not restart the agent. It used to: the change evicted
the cached handle, and dropping that handle sends `session/close` and then
kills the agent process.

### The Agent's Own Settings

An agent advertises settings of its own in `configOptions` — a reasoning-level
selector (`thought_level`), or anything else it invents. Crucible does not
interpret these. `session.list_agent_options` reports them and
`session.set_agent_option` sets one, which reaches the agent as
`session/set_config_option`. A client renders whatever the agent listed, so an
option Crucible has never heard of still gets a control.

The reasoning level is deliberately **not** mapped onto Crucible's thinking
budget. One is a select of names and the other is a token count, and the number
that connects them does not exist.

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

Because sessions are real daemon sessions, Precognition and kiln tools apply automatically — the host does not need to know anything about Crucible's knowledge graph.

**Not yet wired (v1):** session modes, model listing/switching and forking over ACP, host-side filesystem/terminal capabilities (tools run daemon-side exactly as for internal sessions), and authentication (none advertised). A free-form question maps to ACP `elicitation/create`, which Crucible does not serve yet; panels have no ACP analogue. Crucible auto-declines both.

### Dogfood: Crucible hosting Crucible

Because Crucible is both host and agent, you can point one instance at another. Add a profile that runs `cru acp`:

```lua
cru.config.set({
    acp = {
        agents = {
            crucible = {
                command = "cru",
                args = { "acp" },
            },
        },
    },
})
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
