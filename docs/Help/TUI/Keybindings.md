---
title: TUI Keybindings
tags: [help, tui, keybindings]
---

# TUI Keybindings

Crucible's terminal UI supports readline-style editing and custom shortcuts.

## Input Editing (Emacs Mode)

Standard readline keybindings for the input box:

| Key | Action |
|-----|--------|
| `Ctrl+A` | Move to beginning of line |
| `Ctrl+E` | Move to end of line |
| `Ctrl+W` | Delete word backward |
| `Ctrl+U` | Delete to beginning of line |
| `Ctrl+K` | Delete to end of line |
| `Alt+B` | Move word backward |
| `Alt+F` | Move word forward |
| `Ctrl+T` | Transpose characters |

## Navigation

| Key | Action |
|-----|--------|
| `Up/Down` | Scroll conversation history |
| `PageUp/PageDown` | Scroll by page |
| `Home` | Scroll to top |
| `End` | Scroll to bottom |
| `Ctrl+C` | Cancel current operation / Exit |

## Mode Switching

| Key | Action |
|-----|--------|
| `Shift+Tab` | Cycle mode: Default → Plan → Auto → Default |

See [[Help/TUI/Modes]] for details on what each mode does.

## Display Toggles

| Key | Action |
|-----|--------|
| `Alt+T` | Toggle reasoning panel (for thinking models) |
| `Alt+M` | Toggle mouse capture mode |

### Reasoning Panel

When using models that support extended thinking (Claude with thinking budget, Qwen3-thinking, DeepSeek-R1, etc.), press `Alt+T` to show or hide the reasoning panel. This displays the model's internal thought process.

**Thinking Budget:** Configure via `:set thinkingbudget=<value>` or use presets like `high`, `medium`, `low`. See [[Help/TUI/Commands]] for details.

When thinking is visible:
- Reasoning tokens appear in a bordered panel above the response
- Token count displays in the status bar
- Thinking content is styled with dimmed text

### Mouse Capture

By default, mouse capture is enabled for scrolling. Press `Alt+M` to toggle:
- **Enabled**: Scroll with mouse wheel, application handles selection
- **Disabled**: Terminal-native text selection works

## Text Selection

With mouse capture enabled, you can select text by clicking and dragging. Selected text is automatically copied to the clipboard on mouse release.

Selection works across:
- User messages
- Assistant responses
- Code blocks
- Tool outputs

## Popup Navigation

When a popup menu is open (commands, agents, files):

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate items |
| `Enter` | Select item |
| `Escape` | Close popup |
| `Tab` | Accept completion |
| Type | Filter items |

## Command Prefixes

| Prefix | Purpose | Example |
|--------|---------|---------|
| `/` | Slash commands | `/commit`, `/search` |
| `@` | Context references | `@agent-name`, `@file.md` |
| `[[` | Note references | `[[My Note]]`, `[[Help/Config]]` |
| `:` | REPL commands | `:set`, `:model`, `:quit`, `:help` |
| `!` | Shell execution | `!ls -la`, `!git status` |

- `/` triggers after whitespace or at line start
- `@` opens a popup to autocomplete workspace files or agent names
- `[[` opens a popup to autocomplete notes from your kiln (wikilink syntax)
- `:` triggers at line start for REPL commands
- `!` opens a [[TUI/Shell Execution|shell modal]] with streaming output

## See Also

- [[Help/TUI/Commands]] - REPL commands (`:set`, `:model`, etc.)
- [[Help/TUI/Shell Execution]] - Shell Execution
- [[Help/TUI/Index]] - TUI Overview
- [[Help/Configuration]] - Configuration Options
