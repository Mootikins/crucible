---
title: "Z.AI GLM Coding Plan Setup"
description: "Set up Z.AI GLM Coding Plan as an LLM provider in Crucible"
---

This guide walks you through setting up Z.AI GLM Coding Plan as an LLM provider in Crucible.

## What is Z.AI GLM Coding Plan?

Z.AI offers a GLM Coding Plan that provides two API endpoints for use with Crucible:

1. **Native ZAI endpoint** (recommended): An OpenAI-compatible API at `https://api.z.ai/api/coding/paas/v4`. You use GLM model names like `GLM-4.7` directly. Configure with `type = "zai"`.

2. **Anthropic proxy endpoint** (alternative): An Anthropic-compatible API at `https://api.z.ai/api/anthropic`. You send requests using Claude model names (e.g., `claude-sonnet-4-20250514`), and Z.AI maps them to equivalent GLM models. Configure with `type = "anthropic"`.

Both endpoints use the same auth token via the `x-api-key` header.

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

Add to your `crucible.toml` or `llm_providers.toml`.

**Native ZAI (recommended):**

```toml
[llm]
default = "zai-coding"

[llm.providers.zai-coding]
type = "zai"
endpoint = "https://api.z.ai/api/coding/paas/v4"
api_key = "{env:GLM_AUTH_TOKEN}"
default_model = "GLM-4.7"
temperature = 0.7
max_tokens = 4096
```

**Anthropic proxy (alternative):**

```toml
[llm.providers.zai-anthropic]
type = "anthropic"
endpoint = "https://api.z.ai/api/anthropic"
api_key = "{env:GLM_AUTH_TOKEN}"
default_model = "claude-sonnet-4-20250514"
```

The `{env:GLM_AUTH_TOKEN}` syntax reads the key from your environment variable at runtime, keeping secrets out of config files.

## Important: Model Names

Which model names to use depends on which endpoint you configured:

- **Native ZAI** (`type = "zai"`): Use GLM model names like `GLM-4.7`
- **Anthropic proxy** (`type = "anthropic"`): Use Claude model names like `claude-sonnet-4-20250514`

Mixing them up will return an "Unknown Model" error. GLM names won't work on the Anthropic proxy, and Claude names won't work on the native endpoint.

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
:model GLM-4.7
```

Use model names that match your configured endpoint type.

## Troubleshooting

### "Missing API key"

Ensure `GLM_AUTH_TOKEN` is set in your environment:

```bash
echo $GLM_AUTH_TOKEN
```

If empty, set it and restart your terminal.

### "Unknown Model"

You're using a model name that doesn't match your endpoint type.

- Native ZAI (`type = "zai"`): use `GLM-4.7`, not `claude-sonnet-4-20250514`
- Anthropic proxy (`type = "anthropic"`): use `claude-sonnet-4-20250514`, not `glm-4-flash`

### Empty response / no output

Crucible surfaces this as: `"LLM returned empty response — no content received from provider"`. The provider returned no content in its response.

Check:
- Your endpoint URL is correct for your provider type
- The model name matches the endpoint type (GLM names for native ZAI, Claude names for Anthropic proxy)
- Your auth token is valid and hasn't expired

### Stream timeout

Error: `"LLM stream timed out — no response within timeout period"`. The provider is reachable but isn't responding within the timeout window.

Check:
- Network connectivity to `api.z.ai`
- Z.AI service status
- Try again after a brief wait

### "401 Unauthorized"

Ensure `GLM_AUTH_TOKEN` is set in the environment where `cru` runs, not just where you configured it. The Crucible daemon inherits the environment from the shell that started it. If you set the variable after the daemon launched, restart the daemon:

```bash
cru daemon stop
export GLM_AUTH_TOKEN="your-token-here"
cru chat  # daemon auto-starts with the new environment
```

## See Also

- [OpenRouter Setup](./openrouter-setup/)
- [GitHub Copilot Setup](./github-copilot-setup/)
- LLM Providers Reference
