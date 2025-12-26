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

Three session modes control agent behavior:
- **Plan** - Agent explains before acting
- **Act** - Agent executes with confirmation
- **Auto** - Agent executes autonomously

Toggle with `Shift+Tab`.

## Extending the TUI

See [[Help/TUI/Rune API]] for scripting TUI behavior.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message / confirm |
| `Ctrl+C` | Cancel (double to exit) |
| `Ctrl+D` | Exit immediately |
| `Shift+Tab` | Cycle mode |
| `Ctrl+Up/Down` | Scroll by 3 lines |
| `Page Up/Down` | Scroll by 10 lines |
| `/` | Open command popup |
| `@` | Open agent/file popup |
| `Esc` | Dismiss popup/dialog |

## Testing

For developers contributing to the TUI, see [[Help/TUI/E2E Testing]] for information on the expectrl-based test harness that enables PTY-based end-to-end testing.

## See Also

- [[Help/TUI/Component Architecture]] - Widget system details
- [[Help/TUI/Rune API]] - Scripting interface
- [[Help/TUI/E2E Testing]] - End-to-end test harness
- [[Help/CLI/chat]] - Chat command reference
