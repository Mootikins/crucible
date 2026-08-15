---
title: "ACP Configuration"
description: Every field on the [acp] config section, including agent profiles and delegation
tags:
  - help
  - config
  - acp
  - agents
---

# ACP Configuration

The `[acp]` section controls how Crucible hosts external agents over the
[[Help/Concepts/Agent Client Protocol|Agent Client Protocol]] — which agent it reaches for
by default, how it discovers them, and what each named profile is allowed to do.

Add it to `~/.config/crucible/config.toml` (or whatever `-C` / `$CRUCIBLE_CONFIG` points
at). Every field has a default, so `[acp]` is optional.

## `[acp]`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_agent` | string | *(unset)* | Profile to use when `--acp` is omitted. Unset means auto-discover the first available agent. |
| `streaming_timeout_minutes` | integer | `15` | Time allowed for one complete response |
| `enable_discovery` | bool | `true` | **Currently unread** — agents are discovered unconditionally |
| `session_timeout_minutes` | integer | `30` | **Currently unread** — no idle-drop is wired to it |
| `max_message_size_mb` | integer | `25` | **Currently unread** — no size check is wired to it |
| `lazy_agent_selection` | bool | `true` | **Currently unread** — nothing consults it |

```toml
[acp]
default_agent = "claude"
streaming_timeout_minutes = 15
```

Of the scalar fields, `default_agent` and `streaming_timeout_minutes` are the two with
behavior behind them. The other four parse and round-trip through `cru config`, but no
code path reads them today.

`streaming_timeout_minutes` defaults to 15 rather than something tighter because reasoning
models routinely go quiet for minutes at a time mid-turn.

## `[acp.agents.<name>]` — agent profiles

A profile either extends a built-in (`opencode`, `claude`, `gemini`, `codex`, `cursor`) or
defines its own command. The profile name is what you pass to `cru chat -a <name>`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `extends` | string | *(unset)* | Built-in profile to inherit command and args from |
| `command` | string | *(from `extends`)* | Executable to spawn |
| `args` | array of string | *(from `extends`)* | Arguments passed to the command |
| `env` | table | `{}` | Environment variables for the agent process |
| `description` | string | *(unset)* | Human-readable label |
| `capabilities` | array of string | *(unset)* | Informational only — merged into the resolved profile but never enforced or acted on |
| `delegation` | table | *(unset)* | See the delegation sub-table below |
| `permissions` | table | *(unset)* | Per-agent override of the global `[permissions]` |

A profile with neither `command` nor a resolvable `extends` is rejected at spawn time —
Crucible has nothing to run.

```toml
# Point Claude Code at a local proxy
[acp.agents.claude-proxy]
extends = "claude"
description = "Claude Code through a local gateway"
env = { ANTHROPIC_BASE_URL = "http://localhost:4000" }

# A completely custom agent binary
[acp.agents.my-agent]
command = "/usr/local/bin/my-agent"
args = ["--mode", "acp"]
env = { MY_AGENT_ENDPOINT = "http://localhost:8080" }
```

`env` values are passed to the agent process verbatim. Keep secrets out of this table —
the agent inherits Crucible's environment, so exporting the variable in your shell is both
simpler and safer.

### `[acp.agents.<name>.delegation]`

Controls whether this agent may hand work to another agent via the `delegate_session` tool.
Absent means no delegation configuration, which leaves the tool unadvertised.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Whether this agent may delegate at all |
| `max_depth` | integer | `1` | Deepest delegation chain permitted. `0` disables delegation; `1` allows delegation but no nesting; `2` lets a delegated child delegate once more |
| `allowed_targets` | array of string | *(unset — any target)* | Restrict which agents may be delegated to |
| `result_max_bytes` | integer | `51200` | Truncation limit for a delegated result |
| `max_concurrent_delegations` | integer | `3` | Concurrent children one session may spawn |
| `timeout_secs` | integer | `300` | Seconds a delegated child may run before cancellation, blocking or background |

```toml
[acp.agents.orchestrator]
extends = "claude"

[acp.agents.orchestrator.delegation]
enabled = true
max_depth = 2
allowed_targets = ["researcher", "reviewer"]
result_max_bytes = 102400
max_concurrent_delegations = 5
timeout_secs = 600
```

Depth is derived from the child session's parent chain at every level, so a chain cannot be
extended by handing off through an intermediary.

### `[acp.agents.<name>.permissions]`

Same shape as the global `[permissions]` section. When set, it replaces the global config
for sessions using this profile — use it to give different agents different trust levels.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default` | string | `"ask"` | Decision when no rule matches: `allow`, `deny`, or `ask` |
| `allow` | array of string | `[]` | Patterns that auto-approve |
| `deny` | array of string | `[]` | Patterns that refuse |
| `ask` | array of string | `[]` | Patterns that always prompt |

```toml
[acp.agents.claude.permissions]
default = "ask"
deny = ["bash:rm *", "write_file:*"]

[acp.agents.opencode.permissions]
default = "allow"
deny = ["bash:rm -rf *"]
```

See [[Help/Config/permissions]] for pattern syntax and
[[Help/Concepts/Permission Precedence]] for which layer wins when they disagree.

## Full example

```toml
[acp]
default_agent = "claude-proxy"
streaming_timeout_minutes = 30

[acp.agents.claude-proxy]
extends = "claude"
description = "Claude Code through a local gateway"
env = { ANTHROPIC_BASE_URL = "http://localhost:4000" }

[acp.agents.claude-proxy.delegation]
enabled = true
max_depth = 1

[acp.agents.claude-proxy.permissions]
default = "ask"
deny = ["bash:rm *"]
```

## See Also

- [[Help/Config/agents]] — `[chat]` and `[acp]` in the context of agent selection
- [[Help/Concepts/Agent Client Protocol]] — the protocol and the built-in profiles
- [[Help/Concepts/Delegation]] — how delegation works end to end
- [[Help/Config/permissions]] — permission rule syntax
- [[Help/Config/web]] — web server configuration
