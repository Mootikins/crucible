---
description: Terminal User Interface reference documentation
tags:
  - tui
  - reference
  - ui
status: implemented
---

# TUI Reference

Crucible's Terminal User Interface (TUI) provides an interactive chat experience with streaming responses, tool call visualization, and modal interactions.

## Architecture

The TUI uses the **Oil** renderer — a React-like immediate-mode UI with flexbox layout (taffy):

- **ChatApp** - Main application state and event handling
- **InputBox** - Text input with cursor
- **StatusBar** - Mode indicator, status, model info
- **MessageList** - Conversation history with markdown rendering
- **Popup** - Command/file/agent autocomplete

## Key Concepts

### Layers

The UI renders in three layers:
1. **Base** - Main conversation view (history + input + status)
2. **Popup** - Autocomplete overlays
3. **Modal** - Dialog boxes (capture all input)

### Event Flow

Events propagate top-down through layers:
- Modal dialogs capture all events
- Popups receive events when focused
- Base layer handles remaining events

### Modes

Three session modes control agent permissions (see [[Help/TUI/Modes]]):
- **Normal** - Auto-read, ask for writes (default)
- **Plan** - Read-only, creates plan files
- **Auto** - Full access, minimal prompts

Toggle with `Shift+Tab`.

## Extending the TUI

The TUI status bar is driven by Lua configuration. Define your own layout with `crucible.statusline.setup()` — choose which components appear (mode badge, model name, context usage, notifications) and style them with colors and formatting. If no Lua config is present, the TUI uses a sensible default layout.

See [[Help/Lua/Configuration]] for the full statusline API and examples.

## Keyboard Shortcuts

Quick reference (see [[Help/TUI/Keybindings]] for complete list):

| Key | Action |
|-----|--------|
| `Enter` | Send message / confirm |
| `Ctrl+C` | Cancel (double to exit) |
| `Shift+Tab` | Cycle mode |
| `Alt+T` | Toggle reasoning panel |
| `Alt+M` | Toggle mouse capture |
| `/` | Open command popup |
| `@` | Open file/agent popup |
| `[[` | Open notes popup |
| `!` | Execute shell command |
| `Esc` | Dismiss popup/dialog |

Readline-style editing (`Ctrl+A/E/W/U/K`, `Alt+B/F`) is supported in the input box.

## Testing

For developers contributing to the TUI, see [[Help/TUI/E2E Testing]] for information on the expectrl-based test harness that enables PTY-based end-to-end testing.

## REPL Commands

The TUI supports vim-style `:` commands for runtime configuration:

| Command | Description |
|---------|-------------|
| `:set option=value` | Set configuration option |
| `:set option?` | Query current value |
| `:set option!` | Toggle boolean option |
| `:model` | Open model picker popup |
| `:model <name>` | Switch to specific model |
| `:quit` / `:q` | Exit chat |
| `:help` | Show help |

See [[Help/TUI/Commands]] for the complete command reference.

## See Also

- [[Help/TUI/Commands]] - REPL commands (`:set`, `:model`, etc.)
- [[Help/TUI/Keybindings]] - Complete keyboard shortcuts
- [[Help/TUI/Modes]] - Permission modes (normal/plan/auto)
- [[Help/TUI/Shell Execution]] - Running shell commands
- [[Help/TUI/Component Architecture]] - Widget system details
- [[Help/Lua/Configuration]] - TUI customization via Lua
- [[Help/TUI/E2E Testing]] - End-to-end test harness
- [[Help/CLI/chat]] - Chat command reference
