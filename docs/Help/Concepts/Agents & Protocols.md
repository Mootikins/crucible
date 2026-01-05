---
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

An **agent card** configures how an AI behaves:

```yaml
name: Researcher
model: claude-3-opus
tools:
  - semantic_search
  - read_note
instructions: |
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

### ACP (Agent Context Protocol)

ACP extends MCP with features for continuous agent interaction:

- Session persistence
- Multi-turn conversations
- Workflow orchestration

**Use ACP when:**
- Building complex agent workflows
- Agents need to coordinate
- Long-running tasks with state

## Using Agents

Start a chat session:

```bash
cru chat
```

Use a specific agent:

```bash
cru chat --agent Researcher
```

## Context Management

Agents need context to work effectively, but context windows are finite and attention degrades in long conversations.

**Key strategies:**

1. **File-as-state**: Store progress in files (like [[Help/Task Management|TASKS.md]]) instead of accumulating message history
2. **Cached prefixes**: Put static context (system prompt, task definitions) at the start—cached tokens are 75% cheaper
3. **Curated handoffs**: Pass summaries between agents, not full conversation history

See [[Help/Task Management#Context Optimization]] for implementation details.

## See Also

- [[AI Features]] - All AI capabilities
- [[Help/CLI/chat]] - Chat command reference
- [[Help/Extending/Agent Cards]] - Creating agents
- [[Help/Extending/Custom Tools]] - Adding agent capabilities
- [[Help/Task Management]] - TASKS.md format and context optimization
