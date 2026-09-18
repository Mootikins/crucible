---
title: "LLM Configuration"
description: Configure language model providers for chat and agents
tags:
  - reference
  - config
---

# LLM Configuration

Configure language model providers for the chat interface and agents.

## Configuration File

Add to `~/.config/crucible/init.lua`:

```lua
cru.config.set({
    llm = {
        default = "local",
        providers = {
            ["local"] = {
                type = "ollama",
                default_model = "llama3.2",
                endpoint = "http://localhost:11434",
            },
        },
    },
})
```

The `llm` section has three fields:

- `default` — name of the provider to use by default
- `providers` — the named provider instances (`llm.providers.NAME` tables)
- `models` — a specialty → model mapping (`llm.models`) used by agent cards that
  declare a `specialty:` but no explicit `model:`, e.g. `reasoning = "openai/o1"` or
  `coder = "qwen2.5-coder"` (provider inherited when unprefixed)

Each provider lives under `llm.providers.NAME` where `NAME` is whatever label you choose.

## Provider Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Provider backend (see below) |
| `default_model` | string | no | Model to use (falls back to provider default) |
| `endpoint` | string | no | API endpoint (falls back to provider default) |
| `api_key` | string | no | API key. Read it from the environment with `os.getenv("VAR_NAME")` |
| `available_models` | list | no | Models to advertise for this provider (otherwise discovered dynamically) |
| `trust_level` | string | no | Override the backend's default trust level — see [[Help/Concepts/Trust and Classification]] |
| `name` | string | no | Custom display name shown in model lists/UI |

A `llm.default` naming a provider that does not exist in `[llm.providers.*]` is not an error: the session falls back to the built-in Ollama default. Check the spelling of the provider name if a new session comes up on a model you did not pick.

## Providers

### Ollama (Local)

Run models locally with Ollama:

```lua
cru.config.set({
    llm = {
        default = "local",
        providers = {
            ["local"] = {
                type = "ollama",
                default_model = "llama3.2",
                endpoint = "http://localhost:11434",
            },
        },
    },
})
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

```lua
cru.config.set({
    llm = {
        default = "openai",
        providers = {
            openai = {
                type = "openai",
                default_model = "gpt-4o",
                api_key = os.getenv("OPENAI_API_KEY"),
            },
        },
    },
})
```

Defaults to `gpt-4o` on `https://api.openai.com/v1` if not specified.

**Environment variable:**
```bash
export OPENAI_API_KEY=your-api-key
```

### Anthropic

```lua
cru.config.set({
    llm = {
        default = "anthropic",
        providers = {
            anthropic = {
                type = "anthropic",
                default_model = "claude-sonnet-5",
                api_key = os.getenv("ANTHROPIC_API_KEY"),
            },
        },
    },
})
```

Defaults to `claude-sonnet-5` on `https://api.anthropic.com/v1` if not specified. Available models depend on your account. Run `cru models` to see the current list.

**Environment variable:**
```bash
export ANTHROPIC_API_KEY=your-api-key
```

### Other Providers

Additional provider types are supported for chat: `openrouter`, `zai`, `github-copilot`,
`cohere`, and `custom` (generic OpenAI-compatible). They follow the same
`llm.providers.NAME` format. `vertexai` parses but has no chat backend at runtime. Run
`cru models` to see all available models across your configured providers.

## Parameters

> **No `temperature` and no `max_tokens`.** Both are per-model inference
> settings, so Crucible leaves them to the provider — genai picks the right
> default for the model actually being called. Crucible's own 4096 cap used to
> truncate every Anthropic reply that genai would have allowed 64000.

### endpoint

Custom API endpoint:

```lua
cru.config.set({
    llm = {
        providers = {
            ["local"] = {
                type = "ollama",
                endpoint = "http://192.168.1.100:11434",
            },
        },
    },
})
```

### api_key

Set it directly, or read it from the environment with `os.getenv("VAR_NAME")`:

```lua
cru.config.set({
    llm = {
        providers = {
            openai = {
                type = "openai",
                api_key = os.getenv("OPENAI_API_KEY"),
            },
        },
    },
})
```

## Multiple Providers

You can configure several providers and switch between them:

```lua
cru.config.set({
    llm = {
        default = "local",
        providers = {
            ["local"] = {
                type = "ollama",
                default_model = "llama3.2",
            },
            cloud = {
                type = "openai",
                default_model = "gpt-4o",
                api_key = os.getenv("OPENAI_API_KEY"),
            },
            claude = {
                type = "anthropic",
                default_model = "claude-sonnet-5",
                api_key = os.getenv("ANTHROPIC_API_KEY"),
            },
        },
    },
})
```

Change the active provider by setting `default` under `llm`, or switch at runtime with the `:model` command in the TUI.

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |

These are read only where your config calls `os.getenv` for them. The Ollama
endpoint is configured with the provider's `endpoint` field — `OLLAMA_HOST` is consulted
only by `cru init`'s provider detection, not by chat.

## Example Configurations

### Local Development

```lua
cru.config.set({
    llm = {
        default = "local",
        providers = {
            ["local"] = {
                type = "ollama",
                default_model = "llama3.2",
            },
        },
    },
})
```

### Production with OpenAI

```lua
cru.config.set({
    llm = {
        default = "openai",
        providers = {
            openai = {
                type = "openai",
                default_model = "gpt-4o",
                api_key = os.getenv("OPENAI_API_KEY"),
            },
        },
    },
})
```

### Cost-Conscious

```lua
cru.config.set({
    llm = {
        default = "openai-mini",
        providers = {
            ["openai-mini"] = {
                type = "openai",
                default_model = "gpt-4o-mini",
                api_key = os.getenv("OPENAI_API_KEY"),
            },
        },
    },
})
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

## See Also

- `:h config.embedding` — Embedding configuration
- `:h chat` — Chat command reference
- [[Help/CLI/chat]] — Chat usage guide
