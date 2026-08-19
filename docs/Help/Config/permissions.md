---
title: "Permission Configuration"
description: Controlling tool access per session and per agent
status: implemented
tags:
  - help
  - config
  - permissions
  - security
  - acp
---

# Permission Configuration

Crucible lets you control which tools an AI agent can use — and at what level of scrutiny. You can set a global default for all agents, or give each agent its own permission profile.

This page covers the config file. It is one of several layers that can allow or
deny a call; [[Help/Concepts/Permission Precedence]] states the order they run
in and which one wins.

## Global Permissions

Set in `~/.config/crucible/config.toml`. Applies to all agent sessions unless overridden
per-agent. (A kiln's `.crucible/kiln.toml` holds only the kiln's display name — there is
no kiln-level permissions file.)

```toml
[permissions]
# What to do when no rule matches: allow, deny, or ask (default)
default = "ask"

# Always allow these tools (no prompt)
allow = [
  "bash:cargo *",
  "bash:git *",
  "read_file:*",
]

# Always deny these tools (no override possible)
deny = [
  "bash:rm -rf *",
  "bash:sudo *",
]

# Ask user before running these tools
ask = [
  "write_file:*",
  "edit_file:*",
  "bash:*",
]
```

## Per-Agent Permissions

Each ACP agent profile can have its own permission config. When present, it replaces the global `[permissions]` for sessions using that agent.

```toml
# Claude: ask before anything, but wave read-shaped calls through
[acp.agents.claude.permissions]
default = "ask"
allow = ["read:*", "search:*"]

# OpenCode: permissive — allow by default, refuse anything execute-shaped
[acp.agents.opencode.permissions]
default = "allow"
deny = ["bash:*"]

# Gemini: read-only — allow only read- and search-shaped calls
[acp.agents.gemini.permissions]
default = "deny"
allow = ["read:*", "search:*"]
```

**These profiles gate by ACP tool *kind*, not by command or path.** An external agent's
native tool calls reach Crucible labeled only with a kind (`read`, `execute`, …), which
the engine sees under a fixed name — see the kind vocabulary under Rule Format below.
Command- and path-level patterns (`bash:rm *`, `read_file:*`) never match on this path,
so a per-agent deny written that way is silently inert; gate by kind (`deny = ["bash:*"]`)
or rely on the interactive prompt.

### Resolution Order

When a session starts, the permission config is resolved in this priority order:

1. **`--permissions` CLI flag** — overrides the `default` mode for that invocation only
2. **Agent-specific `[acp.agents.<name>.permissions]`** — if present, used in full
3. **Global `[permissions]`** — fallback when agent has no specific config

Note: `--permissions ask` keeps the resolved config's rule lists and only resets the
default. `--permissions allow` and `--permissions deny` are **unconditional** — the
config is replaced with the requested default and *empty* rule lists, so an explicit
`deny` rule does not fire under `--permissions allow` (and an `allow` rule cannot
rescue a tool under `--permissions deny`).

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

Rules follow the pattern `tool:pattern`. The `tool` part must match the tool's name
**exactly** (or be `*`); the `pattern` part is a glob. What that name is — and what the
glob is matched against — depends on which path enforces the rule.

**Internal sessions, Lua `cru.tools.call`, and a workflow's `## Validation`
commands** (which are checked as `bash`) check the tool's own name, as `cru tools`
lists it: `read_file`, `write_file`, `edit_file`, `bash`, MCP gateway tools under their
prefixed names (`gh_search_code`), and so on.

- For `bash`, the pattern matches the **command string** (`cargo test --all`). Chained
  commands (`git log; curl …`) are split and each piece must pass.
- For every other tool, the pattern matches the **raw JSON arguments** of the call —
  e.g. `{"path":"src/main.rs"}` — not a bare path. In practice that makes `*` (match any
  invocation of this tool) the reliable pattern, and path-shaped patterns unreliable.

**External ACP agents** (sessions gated by `[acp.agents.<name>.permissions]`, or by the
global config as their fallback) are checked differently: the agent's native tool calls
arrive labeled only with an ACP tool *kind*, and the engine sees the kind's fixed name —
`read`, `edit`, `delete`, `write` (a move), `search`, `bash` (execute), `fetch`,
`think`, `switch_mode`, or `acp_tool` (any call whose kind is unset or unrecognized).
The input is the raw JSON arguments even for `bash`, so this path can gate by kind
(`read:*`, `bash:*`) but **not** by command or path — `bash:cargo *` and `read_file:*`
never match here. A per-agent `deny = ["bash:rm *"]` is silently inert.

| Rule | Matches |
|------|---------|
| `bash:cargo *` | Any `cargo` command — internal/Lua path only |
| `bash:git *` | Any `git` subcommand — internal/Lua path only |
| `read_file:*` | Any `read_file` call by an internal agent or Lua |
| `write_file:*` | Any `write_file` call by an internal agent or Lua |
| `edit_file:*` | Any `edit_file` call by an internal agent or Lua |
| `gh_search_code:*` | The MCP gateway tool of that (prefixed) name |
| `read:*` | Any read-kind call by an external ACP agent |
| `bash:*` | Any `bash` call (internal) or execute-kind call (ACP) |
| `plugin:<server>:<pattern>` | Parsed but matches nothing on today's call paths — see below |
| `*:*` | Any tool (use carefully) |

Path-shaped patterns like `write:src/**` belong to a structured request vocabulary the
daemon's permission gate understands (a `read`/`write` grant on a bare path), but no
current code path submits requests in that shape — today `read:*`/`write:*` fire only
as ACP kind rules, matching JSON input, where only `*` is dependable.

The three-part forms `mcp:<server>:<pattern>` and `plugin:<server>:<pattern>` parse and
compile: the server name is compared exactly, and the pattern is globbed against the
part of the checked input after its first `:`. But such a rule only fires when a
permission check arrives with the tool named literally `mcp` or `plugin` and an input of
the shape `<server>:<tool>` — and no current call path submits that shape. Tool calls
are checked under the tool's own name (or its ACP kind name) with JSON arguments as
input, so a `plugin:…` rule matches nothing today; to gate a plugin-provided tool, write
a rule against the tool's own name as `cru tools` lists it, like any other tool. For any
other three-part rule (`bash:git status:*`), everything after the first colon is the
glob pattern.

## Denial Precedence

The evaluation order is: hardcoded denials → deny rules → ask rules → allow rules → default.

Within a config, `deny` beats `ask` and `allow`: a call matching both a deny and an
allow rule is denied. The one thing that outranks a written `deny` is the
`--permissions allow` override, which discards the rule lists entirely (see the note
under Resolution Order).

```toml
[acp.agents.opencode.permissions]
default = "allow"
deny = ["bash:*"]  # fires before any allow rule — but not under --permissions allow
```

## What a `deny` rule cannot do

**Command blocking is best-effort. Do not rely on it to prevent a catastrophic
action.**

A rule matches the text of a command. The engine follows a command through the
spellings it can see — `sudo rm`, `\rm`, `"rm"`, `/bin/rm`, `env FOO=1 rm`,
`xargs rm`, `timeout 5 rm` — and it prompts instead of guessing when a line hands
its program to `eval`, `sh -c`, `python -c`, or a name built from a variable.

It still cannot see an alias, a shell function, `$PATH` order, a wrapper program
it does not know, or a different program with the same effect. A rule that names
`rm` does not cover `find . -delete`.

A `deny` rule guards against an accident. It does not stop intent. Deny the whole
tool when the risk is real:

```toml
[permissions]
deny = ["bash:*"]     # no shell at all — the only rule with no spelling to evade
```

To prevent a catastrophic action, use containment. Run the agent in a container
([[Help/Extending/Container Isolation]]), give it a workspace it may destroy, and
keep backups. See [[Help/Concepts/Permission Precedence]] for the full list of
limits.
