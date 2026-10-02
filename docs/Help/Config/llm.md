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

### Request endpoints

A request can also name an endpoint for one session: `cru session configure --endpoint`,
`session.create` or `session.configure_agent` over RPC, a Lua plugin's `configure_agent`, or
the web API. The daemon dials that endpoint. A web browser on another machine can send it,
so the daemon checks every such endpoint before it stores the session's agent.

The daemon accepts an endpoint with no further check when its origin (scheme, host and port)
is one that the operator configured:

- the `endpoint` of a provider in `llm.providers`;
- the default endpoint of a provider type, such as `http://localhost:11434` for Ollama;
- `OLLAMA_HOST`;
- `chat.endpoint`.

The daemon dials these with no request, so a request that names one gets no new reach. A
path on a configured origin is accepted, because the same server answers it.

Any other endpoint must use `http` or `https`, and **every** address its host maps to must be
a globally routable unicast address. For IPv4 the daemon refuses loopback, the RFC 1918
private ranges, link-local `169.254.0.0/16` (the cloud metadata address `169.254.169.254`),
CGNAT `100.64.0.0/10`, `0.0.0.0/8`, `192.0.0.0/24`, `198.18.0.0/15`, `240.0.0.0/4` and
multicast. For IPv6 it accepts only global unicast `2000::/3`, minus the documentation prefix
`2001:db8::/32`. An IPv6 form that encodes an IPv4 address (v4-mapped, v4-compatible,
v4-translated, 6to4, NAT64) is judged as that IPv4 address. The URL parser also normalizes
other spellings, so `http://2130706433` is `127.0.0.1`.

The daemon resolves a hostname and judges all of its answers. One internal answer refuses the
endpoint. A host that does not resolve is refused. The refusal is `INVALID_PARAMS`:

```text
Invalid configuration: Endpoint must not target a private/internal address: 10.0.0.1 →
10.0.0.1. To use a server on this machine or on a private network, add its endpoint to a
provider under `llm.providers` in the config.
```

To use a model server on this machine or on your LAN, add it as a provider (see
[endpoint](#endpoint) above). Then a request can name it.

Two limits:

- The check runs when the request arrives, not when the daemon connects. The dialer resolves
  the host again, so a DNS record that changes between the two lookups (DNS rebinding) is not
  stopped. The model-listing and context-length probes do not follow redirects, so a
  redirect cannot send them to an internal address.
- The check applies to a request's endpoint only. An endpoint that you write to the config,
  with `config.set` or `llm.register_provider`, is operator configuration and is not checked.

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

## Session titles

The daemon generates one canonical title after three successfully completed
user or relay turns. Set `[chat] title_after_turns = 0` to disable automatic
titling, or choose another positive threshold. Eligibility is reconstructed
from persisted session events, including after a restart. Plugin, failed,
and cancelled turns do not count.

The core Luau module `crucible.session_title` formats a bounded conversation
prompt and sanitizes the answer; the daemon owns generation and persistence.
ACP sessions use the configured default LLM for this background completion.
A failed or blank answer leaves the session untitled and allows a later
successful turn to retry. Existing titles remain unchanged; `/generate`
explicitly regenerates through the same owner when the compatibility command
is installed. All clients receive the same persisted `title_changed` event.
