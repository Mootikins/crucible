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

## See Also

- [[Help/CLI/chat]] - Chat command reference
- [[Help/Config/llm]] - LLM configuration
- [[Help/Extending/Agent Cards]] - Creating agent definitions
