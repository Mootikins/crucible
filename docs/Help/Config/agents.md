---
title: agents
description: Configuration reference for AI agents in Crucible
tags:
  - help
  - config
  - agents
---

# Agent Configuration

Crucible runs AI agents through two complementary systems:

1. **Chat agents** (`[chat]`) — Crucible's own REPL agent backed by one of the configured LLM providers.
2. **ACP agents** (`[acp]`) — external agents (Claude Code, OpenCode, Gemini, etc.) hosted via the [[Help/Concepts/Agent Client Protocol|Agent Client Protocol]].

LLM provider credentials and endpoints live in [[Help/Config/llm|[llm]]]; this page covers the two agent-selection sections.

## Configuration Location

Agent settings live in `~/.config/crucible/config.toml` (global) or in a kiln's `.crucible/config.toml` (scoped).

## `[chat]` — Chat Defaults

Applied when you run `cru chat` without `--agent` or `--provider`.

```toml
[chat]
# Override the default model (otherwise inherited from the default provider)
model = "llama3.2"

# Render markdown in responses (default true)
enable_markdown = true

# Prefer external ACP agents or Crucible's built-in agent
# Values: "acp" or "crucible" (default)
agent_preference = "crucible"

# Override provider endpoint for Ollama/compatible
# endpoint = "http://localhost:11434"

# Generation controls (optional)
# temperature = 0.7
# max_tokens = 4096
# timeout_secs = 120

# Stream thinking/reasoning tokens below the spinner
show_thinking = false
```

The actual **provider** default lives under `[llm]`:

```toml
[llm]
default = "ollama"          # key from [llm.providers.*]

[llm.providers.ollama]
type = "ollama"
default_model = "llama3.2"
```

See [[Help/Config/llm|[llm]]] for the full provider reference.

## `[acp]` — ACP Agent Defaults

Applied when you run `cru chat --agent <name>` or bring up the agent picker.

```toml
[acp]
# Default ACP agent to use when --agent is omitted (optional)
default_agent = "opencode"  # or "claude", "gemini", "codex", "cursor"

# Show splash picker in `cru chat` when no agent is chosen.
# true (default): interactive picker with j/k navigation.
# false: use default_agent immediately, skip picker.
lazy_agent_selection = true

# Session timeout in minutes
session_timeout_minutes = 30

# Streaming response timeout in minutes
streaming_timeout_minutes = 15
```

The `--agent` CLI flag always bypasses the picker regardless of `lazy_agent_selection`.

### Custom ACP Agent Profiles

Extend a built-in agent profile with additional environment or arguments under `[acp.agents.<name>]`:

```toml
[acp.agents.my-claude]
extends = "claude"
env = { ANTHROPIC_BASE_URL = "http://localhost:4000" }
```

The extended name is then selectable via `cru chat --agent my-claude`.

## See Also

- [[Help/CLI/chat]] — `cru chat` reference
- [[Help/Config/llm]] — LLM provider configuration
- [[Help/Extending/Agent Cards]] — authoring agent cards
- [[Help/Concepts/Agent Client Protocol]] — ACP architecture
