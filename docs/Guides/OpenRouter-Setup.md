---
description: Set up OpenRouter as an LLM provider for access to 100+ models
tags:
  - guide
  - llm
  - openrouter
  - providers
---

# OpenRouter Setup

This guide walks you through setting up [OpenRouter](https://openrouter.ai) as an LLM provider in Crucible.

## What is OpenRouter?

OpenRouter is a **meta-provider** that gives you access to 100+ language models through a single API key. Instead of managing separate accounts with OpenAI, Anthropic, Google, Meta, and others, you configure one provider and choose any model at runtime.

Models are referenced in `provider/model` format — for example, `openai/gpt-4o`, `anthropic/claude-3.5-sonnet`, or `meta-llama/llama-3.1-405b`.

## Prerequisites

- Crucible CLI installed
- An OpenRouter account with API key

## Setup

### Step 1: Get an API Key

1. Visit [openrouter.ai](https://openrouter.ai) and create an account
2. Go to **Keys** in your dashboard
3. Click **Create Key**
4. Copy the key (starts with `sk-or-`)

### Step 2: Set the Environment Variable

```bash
export OPENROUTER_API_KEY="sk-or-v1-xxxxxxxxxxxx"
```

Add this to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) to persist across sessions.

### Step 3: Configure the Provider

Add to your `crucible.toml` or `llm_providers.toml`:

```toml
[llm]
default = "openrouter"

[llm.providers.openrouter]
type = "openrouter"
api_key = "{env:OPENROUTER_API_KEY}"
default_model = "openai/gpt-4o"
temperature = 0.7
max_tokens = 4096
```

The `{env:OPENROUTER_API_KEY}` syntax reads the key from your environment variable at runtime, keeping secrets out of config files.

## Configuration

### Provider Type Aliases

Crucible accepts several aliases for the provider type:

```toml
type = "openrouter"    # preferred
type = "open_router"   # also works
type = "open-router"   # also works
```

### Model Format

OpenRouter uses `provider/model` format. Some popular choices:

| Model | ID | Notes |
|-------|----|-------|
| GPT-4o | `openai/gpt-4o` | Fast, capable, good default |
| GPT-4o Mini | `openai/gpt-4o-mini` | Cheaper, still capable |
| Claude 3.5 Sonnet | `anthropic/claude-3.5-sonnet` | Strong reasoning |
| Claude 3 Haiku | `anthropic/claude-3-haiku` | Fast and cheap |
| Llama 3.1 405B | `meta-llama/llama-3.1-405b-instruct` | Open-source, large |
| Gemini Pro | `google/gemini-pro-1.5` | Google's flagship |

Browse the full model list at [openrouter.ai/models](https://openrouter.ai/models).

### Multiple Configurations

You can define multiple OpenRouter instances with different default models:

```toml
[llm.providers.or-fast]
type = "openrouter"
api_key = "{env:OPENROUTER_API_KEY}"
default_model = "openai/gpt-4o-mini"
temperature = 0.3

[llm.providers.or-smart]
type = "openrouter"
api_key = "{env:OPENROUTER_API_KEY}"
default_model = "anthropic/claude-3.5-sonnet"
temperature = 0.7
max_tokens = 8192
```

## Usage

### Start a Chat

```bash
# Use default OpenRouter provider
cru chat

# Specify provider explicitly
cru chat --provider openrouter
```

### Switch Models at Runtime

Inside a chat session, use the `:model` command to switch models without restarting:

```
:model openai/gpt-4o-mini
```

### Use with Named Providers

If you defined named providers (like `or-fast` and `or-smart` above):

```bash
cru chat --provider or-fast
cru chat --provider or-smart
```

## Troubleshooting

### "Missing API key"

Ensure `OPENROUTER_API_KEY` is set in your environment:

```bash
echo $OPENROUTER_API_KEY
```

If empty, set it and restart your terminal.

### "Model not found"

Check the model ID format. OpenRouter requires `provider/model` format:

- ✅ `openai/gpt-4o`
- ❌ `gpt-4o`

Browse available models at [openrouter.ai/models](https://openrouter.ai/models).

### Rate limiting

OpenRouter applies per-model rate limits based on the upstream provider. If you hit limits, try a different model or wait briefly.

### Billing

OpenRouter charges per-token based on the upstream model's pricing. Check your usage at [openrouter.ai/activity](https://openrouter.ai/activity).

## See Also

- [[Guides/Getting Started|Getting Started Guide]]
- [[Guides/GitHub Copilot Setup|GitHub Copilot Setup]]
- [[Help/Config/LLM Providers|LLM Providers Reference]]
