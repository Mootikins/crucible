---
title: Agents & Protocols
description: Understanding AI agents and how they connect to your kiln
status: implemented
tags:
  - concept
  - agents
  - mcp
  - acp
---

# Agents & Protocols

Crucible connects AI agents to your knowledge. This page explains how that connection works.

## What is an Agent?

An **agent** is an AI that can take actions - not just answer questions, but search your notes, create files, and use tools. Agents have:

- **A model** - The AI (like Claude, GPT-4, or Llama)
- **Tools** - Actions they can take (search, read, create)
- **Context** - Information they can access (your kiln)

## Agent Cards

An **agent card** configures how an AI behaves. It's a markdown file: YAML frontmatter for configuration, and the markdown body as the system prompt. The `tools:` block is a per-tool permission map (`true`/`allow`, `ask`, `false`/`deny`), not a membership list:

```markdown
---
name: Researcher
description: Explores and synthesizes knowledge
tools:
  semantic_search: true
  read_note: true
  bash: deny
---

You help explore and synthesize knowledge.
Always cite sources using [[wikilinks]].
```

See [[Help/Extending/Agent Cards]] for full details.

## Protocols: MCP and ACP

Crucible uses two protocols for agent communication:

### MCP (Model Context Protocol)

MCP is a standard for AI tools. It defines how agents discover and use capabilities.

**Use MCP when:**
- Connecting external tools (GitHub, databases, APIs)
- Sharing tools between different AI systems
- Building general-purpose integrations

See [[Help/Extending/MCP Gateway]] for connecting MCP servers.

### ACP (Agent Client Protocol)

ACP is a protocol for hosting agents. It defines how a host application
spawns, manages, and talks to an external AI agent (Claude Code, OpenCode,
Gemini CLI) over stdio JSON-RPC:

- Session lifecycle (create, prompt, pause/resume, end)
- Streaming responses, thinking, and tool-call events
- Permission requests from agent to host

ACP does not extend MCP — the two are complementary. ACP runs on the
outside (host ↔ agent, controlling the process and session), MCP on the
inside (agent ↔ tools, discovering and calling capabilities). When Crucible
hosts an external agent over ACP, it exposes kiln tools to that agent via
MCP.

**Use ACP when:**
- Delegating work to an external agent (`cru chat -a claude`)
- Driving Crucible itself from an editor (`cru acp`)

## Using Agents

Start a chat session:

```bash
cru chat
```

Use a specific agent card:

```bash
cru session create --agent Researcher
```

`cru chat` takes `--acp` (an external agent subprocess), not a card: it
resolves its agent client-side rather than through the daemon's session
create, which is where cards are looked up.

## Context Management

Agents need context to work effectively, but context windows are finite and attention degrades in long conversations.

**Key strategies:**

1. **File-as-state**: Store progress in files (like [[Help/Task Management|TASKS.md]]) instead of accumulating message history
2. **Cached prefixes**: Put static context (system prompt, task definitions) at the start—cached tokens are 75% cheaper
3. **Curated handoffs**: Pass summaries between agents, not full conversation history

See [[Help/Task Management#Context Optimization]] for implementation details.

## Tool Execution

When an agent calls a tool during a session, the daemon dispatches the call through a `ToolDispatcher` that routes to the correct executor (built-in tools, Lua plugins, or MCP servers).

**Timeout**: Tool calls have a hard 30-second dispatch timeout. If a tool doesn't return within 30 seconds, the call is cancelled and the agent receives an error message like `Tool 'semantic_search' timed out after 30 seconds`. The agent can then retry or try a different approach.

The one exception is `delegate_session`, which legitimately runs a whole child session inside the call: it gets the delegation timeout (default 300 seconds) plus a 30-second margin as its outer backstop — the delegation layer cancels the child on its own timeout first.

This timeout prevents runaway tool calls from blocking a session indefinitely. Aside from the delegation exception, it applies to all tool types: built-in Rust tools, Lua plugin tools, and tools proxied from external MCP servers.

## See Also

- [[AI Features]] - All AI capabilities
- [[Help/CLI/chat]] - Chat command reference
- [[Help/Extending/Agent Cards]] - Creating agents
- [[Help/Extending/Custom Tools]] - Adding agent capabilities
- [[Help/Task Management]] - TASKS.md format and context optimization
- [[Help/Concepts/Agent Client Protocol]] - ACP specification reference
- [[Help/Concepts/Model Context Protocol]] - MCP specification reference
- [[Help/Concepts/Agent Skills]] - Skills specification reference
