---
description: Documentation note for Shell Execution.
title: Shell Execution
tags: [help, tui, shell]
---

# Shell Execution

Crucible's TUI allows you to execute shell commands directly and optionally insert their output into your next message.

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
- **Status**: Running, exit code with duration, or cancelled (header icon)
- **Output**: Real-time streaming stdout/stderr (stderr in red)

### Modal Keybindings

While the command is **running**:

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down one line |
| `k` / `Up` | Scroll up one line |
| `d` / `u` | Scroll down / up half a page |
| `PageDown` / `PageUp` | Scroll by a full page |
| `Ctrl+C` | Cancel the running command |

After the command **finishes** (or is cancelled), these are added:

| Key | Action |
|-----|--------|
| `i` | Insert the full output into the chat input |
| `t` | Insert truncated output (last 20 lines) |
| `e` | Open output in `$EDITOR` |
| `g` / `G` | Jump to top / bottom |
| `q` / `Escape` | Dismiss modal |

## Inserting Output into the Composer

After a command completes, you can pull the output into your next message:

- Press `i` to insert the **full output** at the cursor
- Press `t` to insert only the **last 20 lines** (useful for long build logs)

The output is inserted into the chat input as a fenced code block prefixed
with "Here is the output of a shell command I ran", including the command
line itself. Exit code, duration, and working directory are **not** part of
the inserted text — they are recorded in the saved output file (below).
Nothing is sent until you press Enter, so you can edit or add context first.

After the modal closes, the transcript shows a shell-execution entry with the
command, exit code, and the tail of its output.

## Output Persistence

Closing the modal saves the output to your session directory:

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

Then press `t` to put the failures in your message and ask for debugging help.

### Checking Git Status

```
!git diff --stat
```

Press `i` to include the changes when asking for commit message suggestions.

### Build Errors

```
!cargo build 2>&1
```

Press `i` and ask the agent to diagnose the compilation errors.

## Tips

- Commands run in the current working directory (where you started `cru chat`)
- The header shows a pending icon while running; press `Ctrl+C` to cancel
- Use `e` to open output in your editor for manual selection/copying
- The modal follows the output as it streams; scrolling manually stops the
  auto-follow, and completion jumps back to the top of the output

## See Also

- [[TUI/Keybindings|All Keybindings]]
- [[TUI/Index|TUI Overview]]
