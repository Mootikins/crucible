---
description: Map of all configuration options for Crucible
status: implemented
tags:
  - moc
  - config
  - setup
---

# Configuration

Crucible is configured through `Config.toml` in your kiln root. This map connects all configuration topics.

## Provider Setup

Configure AI and embedding backends:

- [[Help/Config/llm]] - LLM provider (Ollama, OpenAI, Anthropic)
- [[Help/Config/embedding]] - Embedding provider for semantic search
- [[Help/Config/agents]] - Agent card directories and defaults

## Storage

Configure where data lives:

- [[Help/Config/storage]] - Database backend options

## Quick Reference

Common configuration patterns:

### Local AI (Ollama)
```toml
[llm]
provider = "ollama"
model = "llama3.2"

[embedding]
provider = "ollama"
model = "nomic-embed-text"
```

### Cloud AI (OpenAI)
```toml
[llm]
provider = "openai"
model = "gpt-4"
api_key_env = "OPENAI_API_KEY"

[embedding]
provider = "openai"
model = "text-embedding-3-small"
```

## Getting Started

If you're new to configuration:

1. [[Guides/Getting Started]] - Initial setup
2. [[Guides/Your First Kiln]] - Creating a kiln with config
3. Copy from this kiln's `Config.toml` as a starting point

## Related

- [[AI Features]] - What you can do with configured AI
- [[Extending Crucible]] - Extension configuration

## See Also

- [[Index]] - Return to main index
- `:h config` - Configuration help
- `Config.toml` in this kiln - Full example configuration
