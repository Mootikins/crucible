---
title: Shell Execution
tags: [help, tui, shell]
---

# Shell Execution

Crucible's TUI allows you to execute shell commands directly and optionally share their output with the AI agent.

## Quick Start

Type `!` followed by a command and press Enter:

```
!ls -la
!git status
!cargo build
```

A modal window opens showing the command output in real-time.

## Shell Modal

When a shell command runs, a full-screen modal displays:

- **Command**: The command being executed
- **Status**: Running, completed (exit code), or failed
- **Output**: Real-time streaming stdout/stderr

### Modal Keybindings

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down one line |
| `k` / `Up` | Scroll up one line |
| `d` | Scroll down half page |
| `u` | Scroll up half page |
| `G` | Jump to bottom |
| `g` | Jump to top |
| `Ctrl+C` | Cancel running command |
| `s` | Save and send full output to agent |
| `t` | Save and send truncated output (last 50 lines) |
| `e` | Open output in `$EDITOR` |
| `Enter` / `Escape` | Dismiss modal |

## Sending Output to Agent

After a command completes, you can share the output with the AI:

- Press `s` to send the **full output** as context
- Press `t` to send only the **last 50 lines** (useful for long build logs)

The agent receives the output formatted with the command, exit code, and working directory.

## Output Persistence

Shell outputs are saved to your session directory:

```
<kiln>/.crucible/sessions/<session-id>/shell/<timestamp>-<command>.output
```

File format:
```
$ git status
Exit: 0
Duration: 0.15s
Cwd: /home/user/project
---
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

## Use Cases

### Running Tests

```
!cargo test
```

Then press `t` to send failures to the agent for debugging help.

### Checking Git Status

```
!git diff --stat
```

Press `s` to share changes with the agent for commit message suggestions.

### Build Errors

```
!cargo build 2>&1
```

Press `s` to let the agent help diagnose compilation errors.

## Tips

- Commands run in the current working directory (where you started `cru chat`)
- Long-running commands show a spinner; press `Ctrl+C` to cancel
- Use `e` to open output in your editor for manual selection/copying
- The modal auto-scrolls to bottom during streaming; scroll up to pause

## See Also

- [[TUI/Keybindings|All Keybindings]]
- [[TUI/Index|TUI Overview]]
