---
title: "TUI Reference"
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

- **OilChatApp** - Main application state and event handling
- **InputComponent** - Text input with cursor
- **StatusBar** - Mode indicator, status, model info
- **ContainerList** - Conversation history with markdown rendering
- **PopupOverlay** - Command/file/note autocomplete

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
- **Plan** - Read-only tool set
- **Auto** - Full access, minimal prompts

Toggle with `Shift+Tab`.

## Extending the TUI

The TUI status bar is driven by Lua. A bar is a list of items — `crucible.statusline.setup{}` chooses which appear (mode badge, model, context usage, notifications), where they sit, and how they are styled; you can define more than one bar and anchor them above or below the input. Values the daemon computes, like a git branch, are placed with `sl.expr` and pushed from a handler. With no Lua config the TUI uses a sensible default. See [[Lua/Configuration]] and [[Extending/Scripted UI]].

See [[Help/Lua/Configuration]] for the full statusline API and examples.

## Keyboard Shortcuts

Quick reference (see [[Help/TUI/Keybindings]] for complete list):

| Key | Action |
|-----|--------|
| `Enter` | Send message / confirm |
| `Ctrl+C` | Cancel (double to exit) |
| `Shift+Tab` | Cycle mode |
| `Ctrl+T` | Toggle thinking display |
| `F1` | Open command palette |
| `/` | Open command popup |
| `@` | Open file popup |
| `[[` | Open notes popup |
| `!` | Execute shell command |
| `Esc` | Dismiss popup / cancel streaming |

Readline-style editing (`Ctrl+A/E/W/U`, `Alt+B/F`) is supported in the input box.

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
- [[Help/Lua/Configuration]] - TUI customization via Lua
- [[Help/CLI/chat]] - Chat command reference
