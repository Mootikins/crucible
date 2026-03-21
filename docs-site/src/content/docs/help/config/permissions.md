---
title: "Permission Configuration"
description: "Controlling tool access per session and per agent"
---

Crucible lets you control which tools an AI agent can use — and at what level of scrutiny. You can set a global default for all agents, or give each agent its own permission profile.

## Global Permissions

Set in `~/.config/crucible/config.toml` or your kiln's `Config.toml`. Applies to all agent sessions unless overridden per-agent.

```toml
[permissions]
# What to do when no rule matches: allow, deny, or ask (default)
default = "ask"

# Always allow these tools (no prompt)
allow = [
  "bash:cargo *",
  "bash:git *",
  "read:*",
]

# Always deny these tools (no override possible)
deny = [
  "bash:rm -rf *",
  "bash:sudo *",
]

# Ask user before running these tools
ask = [
  "write:*",
  "bash:*",
]
```

## Per-Agent Permissions

Each ACP agent profile can have its own permission config. When present, it replaces the global `[permissions]` for sessions using that agent.

```toml
# Claude: cautious — ask before any write or shell command
[acp.agents.claude.permissions]
default = "ask"
deny = ["bash:rm *", "bash:sudo *"]

# OpenCode: permissive — allow by default, block only destructive commands
[acp.agents.opencode.permissions]
default = "allow"
deny = ["bash:rm -rf *", "bash:sudo *"]
allow = ["bash:*", "read:*", "write:*"]

# Gemini: read-only — deny any write
[acp.agents.gemini.permissions]
default = "deny"
allow = ["read:*", "bash:cargo *", "bash:git status", "bash:git log *"]
```

### Resolution Order

When a session starts, the permission config is resolved in this priority order:

1. **`--permissions` CLI flag** — overrides the `default` mode for that invocation only
2. **Agent-specific `[acp.agents.<name>.permissions]`** — if present, used in full
3. **Global `[permissions]`** — fallback when agent has no specific config

Note: The `--permissions` flag only changes the `default` mode (allow/deny/ask). Explicit `deny` rules always fire regardless of the mode.

## Per-Session Override (CLI)

Override the default permission mode for a single `cru session send` or `cru session create` call:

```bash
# Allow all tools for this session
cru session create --permissions allow

# Deny all non-safe tools for this send
cru session send --permissions deny <session-id> "summarize this file"
```

## Environment Variable (CI / Headless)

Set `CRUCIBLE_PERMISSIONS` to control the default mode in scripts and CI pipelines:

```bash
# Allow all tools in CI
CRUCIBLE_PERMISSIONS=allow cru session send "$SID" "run the test suite"

# Override: CLI flag wins over env var
CRUCIBLE_PERMISSIONS=deny cru session send --permissions allow "$SID" "do something"
```

Valid values: `allow`, `deny`, `ask`.

## Rule Format

Rules follow the pattern `tool:pattern` where glob matching is supported.

| Rule | Matches |
|------|---------|
| `bash:cargo *` | Any `cargo` command |
| `bash:git *` | Any `git` subcommand |
| `read:*` | Any file read |
| `write:src/**` | Writes inside `src/` |
| `mcp:github:*` | Any GitHub MCP tool |
| `*` | Any tool (use carefully) |

## Denial Precedence

The evaluation order is: hardcoded denials → deny rules → ask rules → allow rules → default.

Explicit `deny` rules always fire. Even `--permissions allow` cannot override an explicit `deny` rule — the rule wins.

```toml
[acp.agents.opencode.permissions]
default = "allow"
deny = ["bash:rm -rf *"]  # this ALWAYS fires, even with --permissions allow
```
