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

The TUI is built on [[Help/TUI/Component Architecture|composable components]] using ratatui:

- **SessionHistoryWidget** - Conversation history with scrolling
- **InputBoxWidget** - Text input with cursor
- **StatusBarWidget** - Mode indicator, status, token count
- **PopupWidget** - Command/file autocomplete
- **DialogWidget** - Modal confirmations and selections
- **LayerStack** - Event routing through UI layers

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
- **Default** - Auto-read, ask for writes
- **Plan** - Read-only, creates plan files
- **Auto** - Full access, minimal prompts

Toggle with `Shift+Tab`.

## Extending the TUI

See [[Help/TUI/Rune API]] for scripting TUI behavior.

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

## See Also

- [[Help/TUI/Keybindings]] - Complete keyboard shortcuts
- [[Help/TUI/Modes]] - Permission modes (default/plan/auto)
- [[Help/TUI/Shell Execution]] - Running shell commands
- [[Help/TUI/Component Architecture]] - Widget system details
- [[Help/TUI/Rune API]] - Scripting interface
- [[Help/TUI/E2E Testing]] - End-to-end test harness
- [[Help/CLI/chat]] - Chat command reference
