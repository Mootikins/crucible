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

Set in `~/.config/crucible/init.lua`. Applies to all agent sessions unless overridden
per-agent. (A kiln's `.crucible/kiln.toml` holds only the kiln's display name — there is
no kiln-level permissions file.)

```lua
cru.config.set({
    permissions = {
        -- What to do when no rule matches: allow, deny, or ask (default)
        default = "ask",

        -- Always allow these tools (no prompt)
        allow = { "bash:cargo *", "bash:git *", "read_file:*" },

        -- Always deny these tools (no override possible)
        deny = { "bash:rm -rf *", "bash:sudo *" },

        -- Ask user before running these tools
        ask = { "write_file:*", "edit_file:*", "bash:*" },
    },
})
```

## Per-Agent Permissions

Each ACP agent profile can have its own permission config. When present, it replaces the global `permissions` for sessions using that agent.

```lua
cru.config.set({
    acp = {
        agents = {
            claude = {
                permissions = {
                    -- Claude: ask before anything, but wave read-shaped calls through
                    default = "ask",
                    allow = { "read:*", "search:*" },
                },
            },
            opencode = {
                permissions = {
                    -- OpenCode: permissive — allow by default, refuse anything execute-shaped
                    default = "allow",
                    deny = { "bash:*" },
                },
            },
            gemini = {
                permissions = {
                    -- Gemini: read-only — allow only read- and search-shaped calls
                    default = "deny",
                    allow = { "read:*", "search:*" },
                },
            },
        },
    },
})
```

**These profiles use the same rules as Crucible's own tools.** A `bash` rule reads the
command line of each shell call of the agent, a file rule reads the paths, and any other
rule reads the canonical tool name. See "What each rule reads" under Rule Format below.

### Resolution Order

A session uses one permission config:

1. **Agent-specific `acp.agents.<name>.permissions`** — if the session's agent names a
   profile with this block, it is used in full. This applies to Crucible's own agents
   and to external ACP agents.
2. **Global `permissions`** — the fallback when the profile has no block of its own.

The `--permissions` CLI flag does not change the config. It is one layer of the order
in [[Help/Concepts/Permission Precedence]]: a `deny` rule and a hardcoded denial still
refuse a call under `--permissions allow`, and an `allow` rule cannot rescue a call
under `--permissions deny`. `--permissions ask` changes nothing.

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

Rules follow the pattern `key:pattern`. The `pattern` part is a glob. The key decides
what the rule reads.

Crucible makes one canonical call from each tool call: from Crucible's own tools, and
from the permission request of an external ACP agent together with the earlier frames of
the same `toolCallId`. The call has a kind (`command`, `file_edit`, `file_read`,
`mcp_tool`, `fetch`, `search` or `tool`), a canonical tool name and typed fields. Every
path reads this call: internal sessions, external ACP agents, Lua `cru.tools.call` and a
workflow's `## Validation` commands. So one rule decides a tool whether an external agent
or Crucible's own agent calls it.

### What each rule reads

- **`bash`** reads the **command line** of each `command` call, whichever tool made it:
  Crucible's `bash`, Claude's `Bash`, codex's `exec_command`, or a shell call that the
  agent does not name. Chained commands (`git log; curl …`) are split and each piece must
  pass. Some agents send a command with no command line that Crucible can read (Hermes
  puts it only in the title). Each `bash` deny rule refuses such a call. With no `bash`
  deny rule, the user is asked, and no `allow` rule and no saved grant can allow it.
  A shell tool of an MCP server (`mcp__srv__bash`) is also a `command` call.
- **`read`** reads each **path** of a `file_read` call. **`edit`**, **`write`** and
  **`delete`** read each path of a `file_edit` call. A `read` rule never reads an edit.
  An ACP agent can send an edit with no path. Each `edit`, `write` or `delete` deny rule
  refuses such a call. With no such deny rule, the user is asked, and no `allow` rule and
  no saved grant can allow it.
  An `allow` rule allows a call only when it matches each path of the call. Of
  Crucible's tools, only `read_file`, `read_note`, `read_metadata`, `glob` and `grep`
  make a `file_read` call. A plugin or MCP gateway tool with a `path` argument is kind
  `tool`, so `allow = ["read:*"]` does not allow it.
- **Any other key** is a **canonical tool name**, and the pattern matches the raw JSON
  arguments of the call — for example `{"path":"src/main.rs"}`. In practice that makes
  `*` the reliable pattern for such a rule.

The canonical tool name is:

1. **A Crucible tool.** The name as `cru tools` lists it: `read_file`, `edit_file`,
   MCP gateway tools under their prefixed names (`gh_search_code`), and so on. An
   external agent's MCP client adds a prefix to a Crucible tool —
   `mcp__crucible__read_note`, or `mcp.crucible.read_note` — and Crucible strips that
   prefix. Write the internal name: `read_note:*`.
2. **Another MCP server's tool, or the agent's own tool.** The whole name stands, so
   write it as the agent sends it: `mcp__github__create_pr:*`, `Bash:*` for `claude`,
   `exec_command:*` for `codex`. Run the agent once and read the tool name off the
   permission prompt if you are unsure.
3. **A call the agent does not name.** The kind is the name: `command`, `file_edit`,
   `file_read`, `mcp_tool`, `fetch`, `search`, or `tool` (a call that Crucible cannot
   classify). The agent's key table in [[Help/Config/acp]] can give such a call a name.

When rules of different keys match one call, the strongest answer wins: `deny`, then
`ask`, then `allow`. So `deny = ["bash:rm *"]` refuses `rm` even when
`allow = ["Bash:*"]` allows Claude's shell tool.

**A call the agent never asks about is the agent's decision.** An external agent runs
its own tools in its own process and asks only about the calls its own policy does not
already allow. Crucible decides what it is asked, and nothing more. To bound what such
an agent can reach at all, use the session's isolation and kiln trust, not these rules.

| Rule | Matches |
|------|---------|
| `bash:cargo *` | Any `cargo` command, from Crucible's shell or from the shell of an external agent |
| `bash:git *` | Any `git` subcommand, from any shell |
| `bash:*` | Each shell call |
| `edit:src/**` | Each edit of a file under `src/`: `edit_file`, `write_file`, `Edit`, or an edit that the agent does not name |
| `read:docs/**` | Each read of a file under `docs/` |
| `read_file:*` | Any `read_file` call by an internal agent or Lua |
| `gh_search_code:*` | The MCP gateway tool of that (prefixed) name |
| `read_note:*` | Crucible's `read_note`, called by an internal agent OR through an external agent's MCP client |
| `Bash:*` | The `Bash` tool of an external agent that sends that name |
| `file_edit:*` | An edit that an external ACP agent did not name |
| `plugin:<server>:<pattern>` | Parsed but matches nothing on today's call paths — see below |
| `*:*` | Any tool (use carefully) |

The three-part forms `mcp:<server>:<pattern>` and `plugin:<server>:<pattern>` parse and
compile: the server name is compared exactly, and the pattern is globbed against the
part of the checked input after its first `:`. But such a rule only fires when a
permission check arrives with the tool named literally `mcp` or `plugin` and an input of
the shape `<server>:<tool>` — and no current call path submits that shape. A rule that
names a tool reads the canonical tool name with JSON arguments as input, so a
`plugin:…` rule matches nothing today; to gate a plugin-provided tool, write
a rule against the tool's own name as `cru tools` lists it, like any other tool. For any
other three-part rule (`bash:git status:*`), everything after the first colon is the
glob pattern.

## Denial Precedence

The evaluation order is: hardcoded denials → deny rules → ask rules → allow rules → default.

Within a config, `deny` beats `ask` and `allow`: a call matching both a deny and an
allow rule is denied. Nothing outranks a written `deny`, the `--permissions allow`
override included.

```lua
cru.config.set({
    acp = {
        agents = {
            opencode = {
                permissions = {
                    default = "allow",
                    deny = { "bash:*" },  -- fires before any allow rule — but not under --permissions allow
                },
            },
        },
    },
})
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

```lua
cru.config.set({
    permissions = {
        deny = { "bash:*" },  -- no shell at all — the only rule with no spelling to evade
    },
})
```

To prevent a catastrophic action, use containment. Run the agent in a container
([[Help/Extending/Container Isolation]]), give it a workspace it may destroy, and
keep backups. See [[Help/Concepts/Permission Precedence]] for the full list of
limits.
