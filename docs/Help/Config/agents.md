---
description: Configuration reference for AI agents in Crucible
tags:
  - help
  - config
  - agents
---

# Agent Configuration

Configure AI agent behavior and provider settings.

## Configuration Location

Agent settings are defined in your kiln's `Config.toml` or globally in `~/.config/crucible/config.toml`.

## Options

```toml
[agents]
# Default provider for chat agents
provider = "ollama"  # or "openai"

# Default model
model = "llama3.2"

# Request timeout in seconds
timeout = 60

# Maximum context tokens
max_context = 8000
```

## Provider-Specific Settings

### Ollama

```toml
[agents.ollama]
endpoint = "http://localhost:11434"
```

### OpenAI

```toml
[agents.openai]
# API key from environment: OPENAI_API_KEY
model = "gpt-4"
```

## ACP Agent Configuration

Configure ACP (Agent Context Protocol) agent behavior:

```toml
[acp]
# Default ACP agent to use (optional)
default_agent = "opencode"  # or "claude", "gemini", etc.

# Enable lazy agent selection (show splash screen to select agent)
# When true (default): Show splash screen to select agent interactively
# When false: Use default agent immediately, skip splash screen
lazy_agent_selection = true

# Session timeout in minutes
session_timeout_minutes = 30

# Streaming response timeout in minutes
streaming_timeout_minutes = 15
```

### Agent Selection Behavior

The `lazy_agent_selection` option controls when and how agents are selected:

**When `lazy_agent_selection = true` (default):**
- Interactive `cru chat` shows a splash screen with available agents
- You can select an agent using vim keys (j/k navigate, Enter confirm)
- Useful when you want to choose different agents for different tasks

**When `lazy_agent_selection = false`:**
- Crucible immediately uses the default agent (or first available)
- Splash screen is skipped
- Useful for automation or when you always use the same agent

**Note:** The `--agent` CLI flag always skips the splash screen regardless of this setting.

## See Also

- [[Help/CLI/chat]] - Chat command reference
- [[Help/Config/llm]] - LLM configuration
- [[Help/Extending/Agent Cards]] - Creating agent definitions
