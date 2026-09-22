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

A mode is a **name, a tool set, and a permission stance**. Four ship by
default, but they are not privileged: they are declared in Lua exactly the way
yours would be, and you can add, replace, or remove any of them. Where a mode
sits in the order of everything else that can allow or deny a call is
[[Help/Concepts/Permission Precedence|its own page]].

## The Built-in Modes

| Mode | Behavior | Use When |
|------|----------|----------|
| **Ask** | Auto-read, ask for writes | Normal interactive use (default) |
| **Plan** | Read-only tool set | Exploring options before acting |
| **Auto** | Full access, minimal prompts | Trusted automated workflows |
| **Propose** | Full access, minimal prompts, note writes become proposals | Reviewing agent-authored notes before they land |

## Ask Mode

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
the conversation, and you switch to ask or auto mode to execute it.

## Auto Mode

Full-access mode for trusted workflows. The agent:
- Can perform any allowed action without prompting
- Still respects agent card tool restrictions
- Useful for running pre-approved plans

Use auto mode carefully - it gives the agent significant autonomy.

## Propose Mode

A mode for review before a note write reaches disk. The agent:
- Sees every tool, the same as auto mode
- Runs with the `allow` permission stance, so nothing prompts
- Turns each note write into a **proposal** instead of writing the file

The file on disk does not change when the agent runs `create_note` or
`update_note`. The daemon holds the write as a proposal until you decide.
Accept it to write the file, reject it with a reason, or dismiss it with no
decision. Review proposals with `cru proposal` or in the web Inbox and diff
pane — see [[Help/CLI/proposal]].

Propose mode is what the [[Help/Concepts/Reflection Pass|reflection pass]]
runs in, so a plugin's note writes wait for you instead of landing unread.

Propose mode only holds back the internal agent's own note tools. A session
that runs an external agent over ACP writes with its own tools in its own
process, so the daemon cannot turn its writes into proposals: such a session
always applies its writes, whatever the mode says.

Use propose mode when you want an agent's note changes queued for review
rather than applied immediately.

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
/default    Switch to the default mode (ask)
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
- Anything you declared — the default colour, until per-mode colours land

A mode change made from another client — the web UI, a Lua handler — updates
this badge too; the daemon is the one authority on which mode a session is in.

The status bar layout is configurable via Lua — see [[Help/Lua/Configuration]].

## Declaring Your Own

```lua
cru.modes.review = {
  -- What the TUI and web show for this mode. Optional: without it the id is
  -- humanized, so `deepReview` reads as "Deep review" and `read-only` as
  -- "Read only". Declare one when the derived name is not what you want.
  label = "Deep review",

  -- Which tools the agent can see at all. Globs use the same syntax as
  -- `cru.on`'s `pattern`.
  tools = { "read_*", "grep", "glob", "bash" },

  -- What to do with the tools it can see. A bare string is a stance;
  -- a table adds rules in the `[permissions]` grammar.
  permissions = {
    default = "deny",
    allow = { "bash:rg *", "bash:git log *" },
  },

  -- What a note write does in this mode. `"apply"` (the default) writes the
  -- file. `"propose"` records the write as a proposal and leaves the file on
  -- disk unchanged, for the user to accept or reject later.
  writes = "apply",
}
```

`permissions` may also be just `"allow"`, `"deny"`, or `"ask"`.

`writes` takes only `"apply"` or `"propose"`. The internal agent keeps the
declared value. A session running an external agent over ACP writes with its
own tools, which the daemon cannot hold back, so its effective `writes` is
always `"apply"` no matter what the mode declares.

Names are sentence case everywhere — "Accept edits", not "Accept Edits" or
"acceptEdits" — so a mode you declare and one an external agent advertises
read the same in the same list. The modeline applies the same rule to the id
and upper-cases the result, so `acceptEdits` shows as ` ACCEPT EDITS `; that
is the modeline's styling, not a second name.

Rules use the same engine as the global `permissions` config, so
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

Declare a mode in `~/.config/crucible/init.lua`. The daemon runs the shipped
defaults file first and your file second, so your declaration wins and
`cru.modes.plan = nil` removes a built-in.

For decisions that depend on the arguments rather than the tool, use a
permission hook instead — see [[Help/Concepts/Permission Precedence]].

## Modes in an ACP Session

A session that runs an external agent — `cru chat --acp claude`, or any
`acp.agents.*` profile — shows **that agent's** modes, not the ones you
declared in Lua. The agent owns them: claude-agent-acp offers five
(`default`, `acceptEdits`, `plan`, `auto`, `bypassPermissions`), codex-acp
offers its own three, and an agent rejects a mode it never declared.

The agent reports its modes when Crucible connects to it. Connecting used to
wait for the first message, which left the session offering Crucible's own
modes until then; today a mode or model list fetch connects the agent itself,
so the list you see is the agent's from the moment you ask for it. If a list
was fetched before the connection came up, the daemon's `mode_changed` event
still corrects it — the TUI and the web UI both refresh themselves when it
arrives.

An external agent that declares no modes leaves the session on Crucible's
set.

Switching mode in such a session sends `session/set_mode` to the agent. The
agent enforces it in its own process, which is also why the review policy
chip reads `post_turn` for an ACP session: a gate that runs in the daemon
arrives after a write the agent already made.

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
