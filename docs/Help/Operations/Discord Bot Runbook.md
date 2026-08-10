---
title: Discord Bot Runbook
description: What to do when the hosted bot misbehaves — stopping it, rotating its secrets, and what it can reach.
tags:
  - operations
  - discord
  - runbook
---

# Discord Bot Runbook

For whoever is on the hook when the hosted bot does something it should not. Read it once
before the bot is public; the point of a runbook is that you are not reasoning from first
principles at the time.

Setup and configuration live in [[Help/Extending/Discord]]. This page is only about incidents.

## Stop it — under 30 seconds

Three levers, fastest first. **Reach for the first one that fits**; they are not alternatives to
each other.

### 1. Stop it answering (keeps the process, keeps the logs)

Add one line to the `[plugins.discord]` section that already exists in
`~/.config/crucible/config.toml` — do **not** add a second `[plugins.discord]` header, which is
a duplicate-table TOML error that takes the whole config down and looks like a fix:

```toml
[plugins.discord]
enabled = false     # <- the only line you add
```

Then `cru daemon restart`.

Verify with `cru plugin list` (shows `Disabled`) **and** by sending the bot a message. The
gateway connection is not the thing you are checking — the bot may still show online.

### 2. Stop the whole daemon

```sh
cru daemon stop
```

Use this when you do not know what is wrong. It takes everything down, including anything else
that daemon hosts — which for a correctly deployed bot is nothing (see
[What it can reach](#what-it-can-reach)).

### 3. Revoke the bot token

In the Discord developer portal, regenerate the token. Use this when the token itself may be
compromised, or when you cannot reach the host. It is the only lever that works without shell
access, and it is not reversible — you will be doing [Rotate the secrets](#rotate-the-secrets)
afterwards either way.

## Rotate the secrets

Two secrets reach the host, and both must be rotatable without a redeploy.

**Bot token** — regenerate in the Discord developer portal, update
`[plugins.discord] bot_token` (or the `DISCORD_BOT_TOKEN` environment variable, which takes
over when the config value is empty), `cru daemon restart`. The old token stops working the
moment you regenerate, so the bot is offline between those steps. That is the intended order:
revoke first, restore second.

**Provider key** — update the provider credential the bot uses, then restart. If
`[plugins.discord] provider_key` names a specific credential, that is the one to rotate; if it
is unset the bot uses the default provider credential, which is probably shared with your own
sessions. **Prefer a dedicated key for the bot** so that rotating it after an incident does not
also interrupt you.

## What it can reach

Know this before you need it, because "what did it have access to?" is the first question after
any incident.

- **Its kiln, and only its kiln.** `[plugins.discord] kiln` plus anything in `kilns`. Reads and
  writes are bounded to those.
- **Not the session directory.** `.crucible/sessions/` is excluded from a plugin session's
  reachable roots, so one user cannot have the agent read another's transcript.
- **Not a shared workspace.** Every Discord session gets a private scratch directory as its
  containment boundary, never the kiln path itself.
- **Whatever `access` grants.** `read` is read tools only; `write` adds file and note writes;
  `ask` requires a named approver to say yes first. `bash` is in no tier — if it runs, someone
  granted it explicitly through `tool_policy`.

**Hard deployment invariant: the bot's daemon must host nothing else.** `ModeRegistry` is
process-global and writable from any Lua in the process, and `session.set_mode` is an
unauthenticated RPC — so anything that reaches that daemon's socket can relax the bot's stance,
or your own TUI's if they share a process. Give the bot its own `CRUCIBLE_SOCKET` and its own
data root, and do not run `cru web` on the same host.

## Common situations

**The bot answered someone it should not have.** Check `allowed_users` and `allowed_guilds` —
both default to empty meaning *nobody*, so anyone getting an answer is on a list or in a listed
server. Remember `allowed_guilds` admits *everyone* in that server, and a `role:` grant admits
whoever holds the role.

**The bot went quiet on its own.** Most likely a retry budget: each outage gets ten attempts,
and a cycle that never connects ends the service with nothing to restart it. `/discord status`
distinguishes "not connected" from "not answering". `cru daemon restart` recovers it. If it
recurs, the gateway is failing to connect at all, not dropping.

**Spend looks wrong.** `quota_turns_per_day` is per user per day. There is **no
per-deployment cap** — that is a known gap, so the total is (users × quota), not a number you
set. If that total is more than you are willing to lose in a day, lower the quota or shorten
the allowlist.

**Someone reports a vulnerability.** `SECURITY.md` points at GitHub private vulnerability
reporting. Reports will be against whatever version is *deployed*, which may be ahead of or
behind the last release — check which before responding.

## Before you need this page

- [ ] A dedicated provider key for the bot, separate from your own.
- [ ] The bot's daemon on its own socket and data root, hosting nothing else.
- [ ] A backup of the public kiln, with a restore you have actually performed once.
- [ ] The daily-spend number at which you degrade to read-only, written down while calm.
- [ ] Kill switch executed once against the *production* deployment, not a local one.
