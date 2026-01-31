---
description: Runtime permission modes for controlling agent actions
status: implemented
tags:
  - tui
  - agents
  - permissions
---

# Modes

Modes control what actions an agent can take at runtime. They act as a permission layer on top of [[Help/Extending/Agent Cards|agent cards]].

## The Three Modes

| Mode | Behavior | Use When |
|------|----------|----------|
| **Normal** | Auto-read, ask for writes | Normal interactive use (default) |
| **Plan** | Read-only, creates plan files | Exploring options before acting |
| **Auto** | Full access, minimal prompts | Trusted automated workflows |

## Normal Mode

The standard mode for interactive use (and the default when starting a session). The agent can:
- Read files and search freely
- Must ask permission for writes, deletes, or commands

This balances productivity with safety. You stay in control of destructive actions.

## Plan Mode

A read-only mode for exploration and planning. The agent:
- Can read, search, and analyze
- Cannot modify files or run commands
- Creates a plan file instead of taking action

Use plan mode when you want to:
- Understand options before committing
- Review proposed changes before execution
- Explore unfamiliar codebases safely

The plan file can later be executed in auto mode.

## Auto Mode

Full-access mode for trusted workflows. The agent:
- Can perform any allowed action without prompting
- Still respects agent card tool restrictions
- Useful for running pre-approved plans

Use auto mode carefully - it gives the agent significant autonomy.

## Switching Modes

### Keyboard

Press `Shift+Tab` to cycle through modes: Normal → Plan → Auto → Normal

### Slash Commands

```
/normal     Switch to normal mode
/plan       Switch to plan mode  
/auto       Switch to auto mode
/mode       Cycle to next mode
```

### Status Bar

The current mode is shown as a colored badge in the status bar:

```
 NORMAL   claude-sonnet   23% ctx
```

The badge is rendered with inverted colors (colored background, dark text):
- **Normal** — Green badge
- **Plan** — Blue badge
- **Auto** — Yellow badge

The status bar layout is configurable via Lua — see [[Help/Lua/Configuration]].

## Interaction with Agent Cards

Modes and agent cards work together:

1. **Agent card** sets base permissions (which tools exist)
2. **Mode** adds runtime restrictions (when to ask permission)

Example: An agent card allows `write_file: ask`. In different modes:
- **Normal**: Prompts before each write
- **Plan**: Blocked entirely (plan mode is read-only)
- **Auto**: Writes without prompting

## See Also

- [[Help/TUI/Keybindings]] - All keyboard shortcuts
- [[Help/Extending/Agent Cards]] - Configuring agent permissions
- [[Help/TUI/Index]] - TUI overview
