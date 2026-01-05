---
tags:
  - guide
  - llm
  - authentication
  - github-copilot
---

# GitHub Copilot Setup

This guide walks you through setting up GitHub Copilot as an LLM provider in Crucible.

## Prerequisites

- Active GitHub Copilot subscription (Individual, Business, or Enterprise)
- Crucible CLI installed

## User Story

**As a** Crucible user with a GitHub Copilot subscription,
**I want to** use my existing Copilot access for chat completions,
**So that** I can leverage models like GPT-4o without additional API costs.

## Setup Flow

### Step 1: Initiate Device Flow Authentication

Run the Crucible auth command:

```bash
cru auth copilot
```

This starts the OAuth device flow and displays:

```
To authenticate with GitHub Copilot:
1. Visit: https://github.com/login/device
2. Enter code: ABCD-1234
3. Authorize the application

Waiting for authorization...
```

### Step 2: Authorize in Browser

1. Open https://github.com/login/device in your browser
2. Enter the code shown in your terminal
3. Click "Authorize" when prompted
4. You'll see "Congratulations, you're all set!"

### Step 3: Token Storage

Once authorized, Crucible stores your OAuth token securely. The CLI confirms:

```
Authentication successful!
Token saved to ~/.config/crucible/copilot-token

Add to your config:
  [llm.providers.copilot]
  type = "copilot"
  api_key = "{env:GITHUB_COPILOT_TOKEN}"
```

### Step 4: Configure Provider

Add to your `crucible.toml`:

```toml
[llm]
default = "copilot"

[llm.providers.copilot]
type = "copilot"
api_key = "{env:GITHUB_COPILOT_TOKEN}"
# default_model = "gpt-4o"  # optional, gpt-4o is default
```

Or set the environment variable directly:

```bash
export GITHUB_COPILOT_TOKEN="gho_xxxxxxxxxxxx"
```

## Available Models

List models available through your Copilot subscription:

```bash
cru models --provider copilot
```

Typical models include:
- `gpt-4o` - Default, best for general use
- `gpt-4o-mini` - Faster, lower cost
- `claude-3.5-sonnet` - Available on some plans

## How It Works

```
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
│   Crucible  │────▶│   GitHub    │────▶│  GitHub Copilot  │
│     CLI     │     │    OAuth    │     │       API        │
└─────────────┘     └─────────────┘     └──────────────────┘
       │                   │                     │
       │ 1. Device flow    │                     │
       │────────────────▶  │                     │
       │                   │                     │
       │ 2. User authorizes│                     │
       │   (in browser)    │                     │
       │                   │                     │
       │ 3. OAuth token    │                     │
       │◀────────────────  │                     │
       │   (gho_xxx)       │                     │
       │                   │                     │
       │ 4. Exchange for   │                     │
       │    API token      │─────────────────────▶│
       │                   │                     │
       │ 5. API token      │                     │
       │◀────────────────  │◀────────────────────│
       │   (30min TTL)     │                     │
       │                   │                     │
       │ 6. Chat request   │                     │
       │─────────────────────────────────────────▶│
```

## Token Lifecycle

- **OAuth token** (`gho_xxx`): Long-lived, stored in config
- **API token**: 30-minute TTL, auto-refreshed by Crucible

You only need to re-authenticate if:
- You revoke the OAuth token in GitHub settings
- Your Copilot subscription lapses

## Troubleshooting

### "Access denied" during authorization

Your GitHub account may not have an active Copilot subscription. Check:
https://github.com/settings/copilot

### "Token exchange failed"

The OAuth token may have been revoked. Re-run:

```bash
cru auth copilot --force
```

### API errors after working previously

The API token (30-min TTL) refreshes automatically, but if issues persist:

1. Check Copilot status: https://www.githubstatus.com/
2. Verify subscription is active
3. Try re-authenticating

## Security Notes

- OAuth tokens are stored with user-only permissions
- Never commit tokens to version control
- Use `{env:VAR}` syntax in config files
- Tokens can be revoked at: https://github.com/settings/applications

## See Also

- [[Help/Configuration|Configuration Reference]]
- [[Help/Config/LLM Providers|LLM Providers]]
- [[Guides/Getting Started|Getting Started Guide]]
