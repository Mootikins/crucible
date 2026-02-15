---
description: Set up Z.AI GLM Coding Plan as an Anthropic-compatible LLM provider
tags:
  - guide
  - llm
  - zai
  - providers
---

# Z.AI GLM Coding Plan Setup

This guide walks you through setting up Z.AI GLM Coding Plan as an LLM provider in Crucible.

## What is Z.AI GLM Coding Plan?

Z.AI offers a GLM Coding Plan that exposes an Anthropic-compatible API endpoint. You send requests using Claude model names (e.g., `claude-sonnet-4-20250514`), and Z.AI maps them to equivalent GLM models. The `x-api-key` header works identically to Anthropic's API.

## Prerequisites

- Crucible CLI installed
- Z.AI GLM Coding Plan account with auth token

## Setup

### Step 1: Get Your Auth Token

1. Log in to your Z.AI account
2. Navigate to your API credentials or settings
3. Copy your GLM Coding Plan auth token (typically 49 characters)

### Step 2: Set the Environment Variable

```bash
export GLM_AUTH_TOKEN="your-token-here"
```

Add this to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) to persist across sessions.

### Step 3: Configure the Provider

Add to your `crucible.toml` or `llm_providers.toml`:

```toml
[llm]
default = "zai-coding"

[llm.providers.zai-coding]
type = "anthropic"
endpoint = "https://api.z.ai/api/anthropic"
api_key = "{env:GLM_AUTH_TOKEN}"
default_model = "claude-sonnet-4-20250514"
temperature = 0.7
max_tokens = 4096
```

The `{env:GLM_AUTH_TOKEN}` syntax reads the key from your environment variable at runtime, keeping secrets out of config files.

## Important: Model Names

You **MUST use Claude model names**, NOT GLM model names. Z.AI maps them internally.

- ✅ `claude-sonnet-4-20250514`
- ❌ `glm-4-flash`

Using GLM model names directly will return an "Unknown Model" error.

## Usage

### Start a Chat

```bash
# Use default Z.AI provider
cru chat

# Specify provider explicitly
cru chat --provider zai-coding
```

### Switch Models at Runtime

Inside a chat session, use the `:model` command to switch models without restarting:

```
:model claude-opus-4-20250805
```

## Troubleshooting

### "Missing API key"

Ensure `GLM_AUTH_TOKEN` is set in your environment:

```bash
echo $GLM_AUTH_TOKEN
```

If empty, set it and restart your terminal.

### "Unknown Model"

You are using a GLM model name instead of a Claude model name. Z.AI requires Claude format:

- ✅ `claude-sonnet-4-20250514`
- ❌ `glm-4-flash`

### Connection errors

Verify the endpoint URL has no trailing slash:

- ✅ `https://api.z.ai/api/anthropic`
- ❌ `https://api.z.ai/api/anthropic/`

## See Also

- [[Guides/OpenRouter-Setup|OpenRouter Setup]]
- [[Guides/GitHub Copilot Setup|GitHub Copilot Setup]]
- [[Help/Config/LLM Providers|LLM Providers Reference]]
