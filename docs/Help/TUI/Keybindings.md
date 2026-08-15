---
description: Documentation note for Keybindings.
title: TUI Keybindings
tags: [help, tui, keybindings]
---

# TUI Keybindings

Crucible's terminal UI supports readline-style editing and custom shortcuts.

## Global Keys

| Key | Action |
|-----|--------|
| `Enter` | Send message / confirm |
| `Ctrl+C` | Clear input; with empty input, press twice (within 300ms) to quit |
| `Esc` | Cancel streaming / close popup or modal |
| `Ctrl+Enter` | During streaming: cancel the turn, keeping your draft in the input |
| `Shift+Tab` | Cycle mode: Normal → Plan → Auto |
| `Ctrl+T` | Toggle thinking/reasoning display |
| `F1` | Open command palette |

## Input Editing (Emacs Mode)

Readline-style keybindings for the input box:

| Key | Action |
|-----|--------|
| `Ctrl+A` / `Home` | Move to beginning of line |
| `Ctrl+E` / `End` | Move to end of line |
| `Ctrl+B` / `Ctrl+F` | Move one character left / right |
| `Alt+B` / `Ctrl+Left` | Move word backward |
| `Alt+F` / `Ctrl+Right` | Move word forward |
| `Ctrl+W` | Delete word backward |
| `Ctrl+U` | Clear the entire input |
| `Delete` | Delete character forward |
| `Ctrl+J` | Insert a newline (multi-line input) |
| `Up` / `Ctrl+P` | Previous input history entry |
| `Down` / `Ctrl+N` | Next input history entry (back to draft at the end) |

`Up`/`Down` recall previously submitted input; the draft you were typing is
preserved and restored when you cycle past the newest entry.

## Scrolling the Conversation

Completed conversation content graduates to the terminal's own scrollback —
the live viewport stays small. To review history, use your terminal's native
scrolling (mouse wheel, `Shift+PageUp`, tmux copy mode, etc.). The TUI does
not capture the mouse, so terminal-native text selection and copying work as
usual. `PageUp`/`PageDown` are not bound in the chat view (the shell modal
and pager modals bind them — see [[Help/TUI/Shell Execution]]).

## Mode Switching

| Key | Action |
|-----|--------|
| `Shift+Tab` | Cycle mode: Normal → Plan → Auto → Normal |

See [[Help/TUI/Modes]] for details on what each mode does.

## Thinking Display

When using models that support extended thinking (Claude with thinking
budget, Qwen3-thinking, DeepSeek-R1, etc.), press `Ctrl+T` to show or hide
thinking blocks. This works during streaming too, and applies retroactively
to visible blocks. A toast confirms the new state.

**Thinking Budget:** Configure via `:set thinkingbudget=<preset>` using
presets like `high`, `medium`, `low`. See [[Help/TUI/Commands]] for details.

## Popup Navigation

When a completion popup is open (commands, files, notes, models):

| Key | Action |
|-----|--------|
| `Up/Down` | Navigate items |
| `Enter` / `Tab` | Accept selection |
| `Escape` | Close popup |
| `Backspace` | Narrow filter (closes popup when the trigger is deleted) |
| Type | Filter items |

## Modal Keymaps

Modals capture all input while open.

### Permission modal

Shown when the agent needs approval for a tool call (see the footer hints):

| Key | Action |
|-----|--------|
| `y` | Allow this call |
| `n` | Deny this call |
| `a` | Allowlist: save the suggested pattern project-scoped and allow |
| `Up/Down` / `k`/`j` | Move between Yes / No / Allowlist |
| `Enter` | Confirm the highlighted option |
| `Shift+Enter` | On Allowlist: save the rule user-scoped (global) instead |
| `Tab` | Edit free text (deny reason, or the allowlist pattern) |
| `h` | Expand/collapse the diff (file operations) |
| `Esc` / `Ctrl+C` | Deny and close |

While editing text: `Enter` sends, `Esc` returns to the options.
`:set perm.show_diff`, `perm.autoconfirm_session`, and `perm.full_commands`
tune this modal — see [[Help/TUI/Commands]].

### Ask modals

Agent questions (single-select, multi-select, or free-text "other"):

| Key | Action |
|-----|--------|
| `Up/Down` / `k`/`j` | Navigate choices |
| `Space` | Toggle a choice (multi-select) |
| `Tab` | Jump to free-text entry (when offered); in batched questions, next question |
| `Enter` | Confirm selection / submit text |
| `Esc` / `Ctrl+C` | Cancel (sends a cancelled response) |

### Show (pager) modal

Read-only content the agent asks you to review:

| Key | Action |
|-----|--------|
| `j`/`k`, `Down`/`Up` | Scroll one line |
| `PageDown` / `Space`, `PageUp` | Scroll one page |
| `g` / `G` | Jump to top / bottom |
| `q` / `Esc` | Close |

### Edit modal

Inline text editing with a small vim-like model: `h/j/k/l` or arrows move,
`i`/`a`/`o` enter insert mode, `Esc` leaves insert mode (or cancels from
normal mode), `Ctrl+S` saves and submits the edited content.

## Command Prefixes

| Prefix | Purpose | Example |
|--------|---------|---------|
| `/` | Slash commands | `/mode`, `/plan`, `/undo` |
| `@` | Attach a workspace file | `@src/main.rs`, `@notes/todo.md` |
| `[[` | Note references | `[[My Note]]`, `[[Help/Config]]` |
| `:` | REPL commands | `:set`, `:model`, `:quit`, `:help` |
| `!` | Shell execution | `!ls -la`, `!git status` |

- `/` triggers after whitespace or at line start
- `@` opens a popup to autocomplete workspace files. The file's contents are
  attached to that message, so the agent reads them without a tool call — the
  path must be inside the workspace, and a big file is truncated with a note
- `[[` opens a popup to autocomplete notes from your kiln (wikilink syntax)
- `:` triggers at line start for REPL commands
- `!` opens a [[TUI/Shell Execution|shell modal]] with streaming output

## See Also

- [[Help/TUI/Commands]] - REPL commands (`:set`, `:model`, etc.)
- [[Help/TUI/Shell Execution]] - Shell Execution
- [[Help/TUI/Index]] - TUI Overview
- [[Help/Configuration]] - Configuration Options
