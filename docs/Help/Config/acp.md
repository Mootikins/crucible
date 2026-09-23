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

The `acp` section controls how Crucible hosts external agents over the
[[Help/Concepts/Agent Client Protocol|Agent Client Protocol]] — which agent it reaches for
by default, how it discovers them, and what each named profile is allowed to do.

Add it to `~/.config/crucible/init.lua` (or whatever `-C` / `$CRUCIBLE_CONFIG` points
at). Every field has a default, so `acp` is optional.

## `acp`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_agent` | string | *(unset)* | Profile to use when `--acp` is omitted. Unset means auto-discover the first available agent. |
| `streaming_timeout_minutes` | integer | `15` | Time allowed for one complete response |

```lua
cru.config.set({
    acp = {
        default_agent = "claude",
        streaming_timeout_minutes = 15,
    },
})
```

Earlier versions also parsed `enable_discovery`, `session_timeout_minutes`,
`max_message_size_mb` and `lazy_agent_selection`. No code read them, so they are removed.
A config file that still contains them loads without an error; the values are ignored.

`streaming_timeout_minutes` defaults to 15 rather than something tighter because reasoning
models routinely go quiet for minutes at a time mid-turn.

## `acp.agents.<name>` — agent profiles

A profile does not inherit from another profile. There are two kinds, and the name
decides which:

1. The name of a built-in (`opencode`, `claude`, `gemini`, `codex`, `cursor`, `hermes`,
   `antigravity`). The fields you set lay over that built-in.
2. Any other name. The profile must define `command`, because Crucible has nothing
   else to run.

The profile name is what you pass to `cru chat -a <name>`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `command` | string | *(from the built-in of the same name)* | Executable to spawn |
| `args` | array of string | *(from the built-in of the same name)* | Arguments passed to the command |
| `env` | table | `{}` | Environment variables for the agent process |
| `description` | string | *(unset)* | Human-readable label |
| `delegation` | table | *(unset)* | See the delegation sub-table below |
| `permissions` | table | *(unset)* | Per-agent override of the global `permissions` |
| `tools` | array of table | *(from the shipped defaults)* | The key table that classifies the agent's tool calls. See below |

A profile that is not a built-in and defines no `command` is an error. Crucible does not
run the profile name as a command, so a misspelled name fails with the name in the message.

Earlier versions had an `extends` key. It is removed. A config that still sets it gets an
error that names the key: move the fields into the profile named after the built-in, or
give the profile its own `command`.

```lua
cru.config.set({
    acp = {
        agents = {
            claude = {
                -- Point Claude Code at a local proxy
                description = "Claude Code through a local gateway",
                env = { ANTHROPIC_BASE_URL = "http://localhost:4000" },
            },
            ["my-agent"] = {
                -- A completely custom agent binary
                command = "/usr/local/bin/my-agent",
                args = { "--mode", "acp" },
                env = { MY_AGENT_ENDPOINT = "http://localhost:8080" },
            },
        },
    },
})
```

`env` values are passed to the agent process verbatim. Keep secrets out of this table —
the agent inherits Crucible's environment, so exporting the variable in your shell is both
simpler and safer.

### `acp.agents.<name>.delegation`

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

```lua
cru.config.set({
    acp = {
        agents = {
            claude = {
                delegation = {
                    enabled = true,
                    max_depth = 2,
                    allowed_targets = { "researcher", "reviewer" },
                    result_max_bytes = 102400,
                    max_concurrent_delegations = 5,
                    timeout_secs = 600,
                },
            },
        },
    },
})
```

Depth is derived from the child session's parent chain at every level, so a chain cannot be
extended by handing off through an intermediary.

### `acp.agents.<name>.permissions`

Same shape as the global `permissions` section. When set, it replaces the global config
for sessions using this profile — use it to give different agents different trust levels.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default` | string | `"ask"` | Decision when no rule matches: `allow`, `deny`, or `ask` |
| `allow` | array of string | `[]` | Patterns that auto-approve |
| `deny` | array of string | `[]` | Patterns that refuse |
| `ask` | array of string | `[]` | Patterns that always prompt |

```lua
cru.config.set({
    acp = {
        agents = {
            claude = {
                permissions = {
                    default = "ask",
                    deny = { "bash:rm *", "write_file:*" },
                },
            },
            opencode = {
                permissions = {
                    default = "allow",
                    deny = { "bash:rm -rf *" },
                },
            },
        },
    },
})
```

See [[Help/Config/permissions]] for pattern syntax and
[[Help/Concepts/Permission Precedence]] for which layer wins when they disagree.

### `acp.agents.<name>.tools`

Each ACP agent sends a tool call in its own form. Crucible turns each call into one
Crucible tool call with a kind (`command`, `file_edit`, `file_read`, `mcp_tool`, `fetch`,
`search` or `tool`) and typed fields. The key table tells Crucible where an agent puts
these fields. The shipped `runtime/defaults/init.luau` sets the tables of the built-in
agents, and explains each field. A table that you set replaces the shipped table of that
agent.

Each entry applies to a call when all of its match fields match: `name` (the ACP tool
name), `acp_kind` (the ACP kind) and `title` (plain text in the ACP title). Then the entry
gives a `kind` and lists of keys for `args`, `tool`, `command`, `paths`, `url` and `query`.
A key that starts with `/` is a JSON pointer into the whole call.

```lua
cru.config.set({
    acp = {
        agents = {
            ["my-agent"] = {
                command = "/usr/local/bin/my-agent",
                tools = {
                    { acp_kind = "execute", command = { "cmd" } },
                    { title = "Fetch ", kind = "fetch", url = { "/rawInput/target" } },
                },
            },
        },
    },
})
```

## Full example

```lua
cru.config.set({
    acp = {
        default_agent = "claude",
        streaming_timeout_minutes = 30,
        agents = {
            claude = {
                description = "Claude Code through a local gateway",
                env = { ANTHROPIC_BASE_URL = "http://localhost:4000" },
                delegation = {
                    enabled = true,
                    max_depth = 1,
                },
                permissions = {
                    default = "ask",
                    deny = { "bash:rm *" },
                },
            },
        },
    },
})
```

## See Also

- [[Help/Config/agents]] — `chat` and `acp` in the context of agent selection
- [[Help/Concepts/Agent Client Protocol]] — the protocol and the built-in profiles
- [[Help/Concepts/Delegation]] — how delegation works end to end
- [[Help/Config/permissions]] — permission rule syntax
- [[Help/Config/web]] — web server configuration
