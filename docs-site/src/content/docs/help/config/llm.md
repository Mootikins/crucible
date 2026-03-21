---
title: "LLM Configuration"
description: "Configure language model providers for chat and agents"
---

Configure language model providers for the chat interface and agents.

## Configuration File

Add to `~/.config/crucible/config.toml`:

```toml
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"
```

The `[llm]` section has one field:

- `default` — name of the provider to use by default

Each provider lives under `[llm.providers.NAME]` where `NAME` is whatever label you choose.

## Provider Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Provider backend (see below) |
| `default_model` | string | no | Model to use (falls back to provider default) |
| `endpoint` | string | no | API endpoint (falls back to provider default) |
| `api_key` | string | no | API key, or `{env:VAR_NAME}` to read from environment |
| `temperature` | float | no | Randomness 0.0–2.0 (default: 0.7) |
| `max_tokens` | integer | no | Max response tokens (default: 4096) |
| `timeout_secs` | integer | no | Request timeout in seconds (default: 120) |

## Providers

### Ollama (Local)

Run models locally with Ollama:

```toml
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"
```

All fields except `type` are optional. Ollama defaults to `llama3.2` on `http://localhost:11434`.

**Setup:**
```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.2

# Verify it's running
ollama list
```

### OpenAI

```toml
[llm]
default = "openai"

[llm.providers.openai]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"
```

Defaults to `gpt-4o` on `https://api.openai.com/v1` if not specified.

**Environment variable:**
```bash
export OPENAI_API_KEY=your-api-key
```

### Anthropic

```toml
[llm]
default = "anthropic"

[llm.providers.anthropic]
type = "anthropic"
default_model = "claude-3-5-sonnet-20241022"
api_key = "{env:ANTHROPIC_API_KEY}"
```

Defaults to `claude-3-5-sonnet-20241022` on `https://api.anthropic.com/v1` if not specified. Available models depend on your account. Run `cru models` to see the current list.

**Environment variable:**
```bash
export ANTHROPIC_API_KEY=your-api-key
```

### Other Providers

Additional provider types are supported: `openrouter`, `zai`, `github-copilot`, `vertexai`, `cohere`, and `custom`. They follow the same `[llm.providers.NAME]` format. Run `cru models` to see all available models across your configured providers.

## Parameters

### temperature

Controls randomness in responses (0.0–2.0):

```toml
[llm.providers.local]
type = "ollama"
temperature = 0.7
```

- `0.0` — Deterministic, focused
- `0.7` — Balanced (default)
- `1.0+` — More creative, varied

### max_tokens

Maximum tokens in response:

```toml
[llm.providers.openai]
type = "openai"
default_model = "gpt-4o"
max_tokens = 4096
```

### endpoint

Custom API endpoint:

```toml
[llm.providers.local]
type = "ollama"
endpoint = "http://192.168.1.100:11434"
```

### api_key

Set directly or reference an environment variable with `{env:VAR_NAME}`:

```toml
[llm.providers.openai]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"
```

## Multiple Providers

You can configure several providers and switch between them:

```toml
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"

[llm.providers.cloud]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"

[llm.providers.claude]
type = "anthropic"
default_model = "claude-3-5-sonnet-20241022"
api_key = "{env:ANTHROPIC_API_KEY}"
```

Change the active provider by setting `default` under `[llm]`, or switch at runtime with the `:model` command in the TUI.

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OLLAMA_HOST` | Ollama endpoint (default: localhost:11434) |

## Example Configurations

### Local Development

```toml
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
temperature = 0.7
```

### Production with OpenAI

```toml
[llm]
default = "openai"

[llm.providers.openai]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"
max_tokens = 4096
```

### Cost-Conscious

```toml
[llm]
default = "openai-mini"

[llm.providers.openai-mini]
type = "openai"
default_model = "gpt-4o-mini"
api_key = "{env:OPENAI_API_KEY}"
temperature = 0.5
max_tokens = 2048
```

## Troubleshooting

### "Connection refused" with Ollama

Check Ollama is running:
```bash
ollama list
```

Start if needed:
```bash
ollama serve
```

### "Invalid API key" with OpenAI/Anthropic

Verify environment variable:
```bash
echo $OPENAI_API_KEY
```

### Model not found

For Ollama, pull the model first:
```bash
ollama pull llama3.2
```

For cloud providers, check that the model name is correct. Run `cru models` to list available models.

## Implementation

**Source code:** `crates/crucible-config/src/components/llm.rs`

## See Also

- `:h config.embedding` — Embedding configuration
- `:h chat` — Chat command reference
- [chat](../cli/chat/) — Chat usage guide
