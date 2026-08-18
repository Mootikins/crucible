---
title: Modes
description: Runtime permission modes for controlling agent actions
status: implemented
tags:
  - tui
  - agents
  - permissions
---

# Modes

Modes control what actions an agent can take at runtime. They act as a permission layer on top of [[Help/Extending/Agent Cards|agent cards]].

A mode is a **name, a tool set, and a permission stance**. Three ship by
default, but they are not privileged: they are declared in Lua exactly the way
yours would be, and you can add, replace, or remove any of them. Where a mode
sits in the order of everything else that can allow or deny a call is
[[Help/Concepts/Permission Precedence|its own page]].

## The Built-in Modes

| Mode | Behavior | Use When |
|------|----------|----------|
| **Normal** | Auto-read, ask for writes | Normal interactive use (default) |
| **Plan** | Read-only tool set | Exploring options before acting |
| **Auto** | Full access, minimal prompts | Trusted automated workflows |

## Normal Mode

The standard mode for interactive use (and the default when starting a session). The agent can:
- Read files and search freely
- Must ask permission for writes, deletes, or commands

This balances productivity with safety. You stay in control of destructive actions.

## Plan Mode

A read-only mode for exploration and planning. The agent:
- Sees only read-only tools (search, read, metadata) — write tools and
  command execution are filtered out of its tool set entirely
- Has each prompt prefixed with a plan-mode notice reminding it that write
  tools are disabled

Use plan mode when you want to:
- Understand options before committing
- Review proposed changes before execution
- Explore unfamiliar codebases safely

Plan mode does not write anything itself — the agent describes its plan in
the conversation, and you switch to normal or auto mode to execute it.

## Auto Mode

Full-access mode for trusted workflows. The agent:
- Can perform any allowed action without prompting
- Still respects agent card tool restrictions
- Useful for running pre-approved plans

Use auto mode carefully - it gives the agent significant autonomy.

## Switching Modes

### Keyboard

Press `Shift+Tab` to cycle through the modes your session declares, in
declaration order, wrapping at the end.

### Slash Commands

Every declared mode is its own slash command, so a mode you named `review` gets
`/review` for free.

```
/mode       Cycle to the next declared mode
/<name>     Switch to that mode (/plan, /auto, /review, …)
/default    Switch to the default mode (normal)
```

### Status Bar

The current mode is shown as a colored badge in the status bar:

```
 NORMAL   claude-sonnet   23% ctx
```

The badge is the mode's name in upper case, rendered with inverted colors
(colored background, dark text):
- **Normal** — Green badge
- **Plan** — Blue badge
- **Auto** — Yellow badge
- Anything you declared — the normal colour, until per-mode colours land

A mode change made from another client — the web UI, a Lua handler — updates
this badge too; the daemon is the one authority on which mode a session is in.

The status bar layout is configurable via Lua — see [[Help/Lua/Configuration]].

## Declaring Your Own

```lua
cru.modes.review = {
  -- Which tools the agent can see at all. Globs use the same syntax as
  -- `crucible.on`'s `pattern`.
  tools = { "read_*", "grep", "glob", "bash" },

  -- What to do with the tools it can see. A bare string is a stance;
  -- a table adds rules in the `[permissions]` grammar.
  permissions = {
    default = "deny",
    allow = { "bash:rg *", "bash:git log *" },
  },
}
```

`permissions` may also be just `"allow"`, `"deny"`, or `"ask"`.

Rules use the same engine as the global `[permissions]` config, so
`bash:rg *` inherits its handling of chained commands — permitting `rg` does
not thereby permit `rg foo && rm -rf /`, `rg foo; rm -rf /`, or the same line
with `&`, `|`, `||` or a newline in place of the `&&`. A construct the splitter
cannot read — `` ` ``, `$(…)`, `<(…)`, `>(…)`, an unclosed quote — drops the
decision to the mode's `default` rather than to the leading command, so it
prompts instead of silently allowing.

A `deny` rule follows the command through the ways a shell spells it — `sudo rm`,
`\rm`, `"rm"`, `/bin/rm`, `env FOO=1 rm`, `xargs rm`, `timeout 5 rm`. It reports
`eval` and `sh -c` rather than reading them, so those prompt.

Two edges the split does not reach. It does not model *where* an allowed command
writes; `bash:echo *` permits `echo hi > file`. It does not follow an *effect* to
a different program; a rule that names `rm` never covers `find . -delete`.

**Do not treat a `deny` rule as a safety barrier.** It reduces accidents. It does
not stop a determined caller. [[Help/Concepts/Permission Precedence]] states the
guarantee and its limits in full.

Declare a mode in `~/.config/crucible/init.lua` for every session, or in
`<workspace>/.crucible/lua/init.lua` for one project. That path is the
session's **workspace** — the directory work happens in — not its kiln; see
`crates/crucible-daemon/src/agent_manager/session_vm.rs`. Setting
`cru.modes.plan = nil` removes a built-in.

For decisions that depend on the arguments rather than the tool, use a
permission hook instead — see [[Help/Concepts/Permission Precedence]].

## Interaction with Agent Cards

Modes and agent cards work together:

1. **Agent card** sets base permissions (which tools exist)
2. **Mode** adds runtime restrictions (when to ask permission)

Example: An agent card allows `write_file: ask`. In different modes:
- **Normal**: Prompts before each write
- **Plan**: Blocked entirely (plan mode is read-only)
- **Auto**: Writes without prompting

## See Also

- [[Help/Concepts/Permission Precedence]] - Where modes sit among the other layers
- [[Help/TUI/Keybindings]] - All keyboard shortcuts
- [[Help/Extending/Agent Cards]] - Configuring agent permissions
- [[Help/TUI/Index]] - TUI overview
