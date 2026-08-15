---
title: "TUI Commands"
description: Vim-style REPL commands for TUI configuration and control
tags:
  - tui
  - commands
  - configuration
  - reference
status: implemented
---

# TUI Commands

The TUI supports vim-style `:` commands for runtime configuration and control. Type `:` at the beginning of a line to enter command mode.

## Quick Reference

| Command | Description |
|---------|-------------|
| `:set option=value` | Set configuration option |
| `:set option?` | Query current value |
| `:set option??` | Show modification history |
| `:set option!` | Toggle boolean option |
| `:set option&` | Reset to default |
| `:set` | Show modified options |
| `:set all` | Show all options |
| `:model` | Open model picker |
| `:model <name>` | Switch to model |
| `:clear` | Clear conversation |
| `:undo [N]` | Undo the last N agent turns (default 1) |
| `:export <path>` | Export session to markdown |
| `:messages` | Toggle the notification drawer (aliases: `:msgs`, `:notifications`) |
| `:palette` | Open command palette (alias: `:commands`, key: `F1`) |
| `:pick [source]` | Open a fuzzy picker (notes, files, commands) |
| `:mcp` | Show MCP server status |
| `:plugins` | Show loaded plugins |
| `:reload [name]` | Reload a plugin (no name = all) |
| `:config` | Show current configuration |
| `:lua <expr>` | Evaluate Lua daemon-side (shorthand: `:= <expr>`) |
| `:quit` / `:q` | Exit chat |
| `:help [topic]` | Show help (alias: `:h`) |

An unknown `:` command shows a warning with a did-you-mean suggestion.

## The `:set` Command

Crucible's `:set` command follows Vim conventions for runtime configuration.

### Setting Values

```
:set option=value       # Set string/number option
:set option:value       # Alternative syntax
:set option value       # Space-separated (if unambiguous)
```

Examples:
```
:set model=claude-3-5-sonnet
:set thinkingbudget=high
```

### Boolean Options

```
:set option             # Enable boolean option
:set nooption           # Disable boolean option (prefix with 'no')
:set option!            # Toggle option
:set invoption          # Toggle option (alternative)
```

Examples:
```
:set thinking           # Enable thinking display
:set nothinking         # Disable thinking display
:set precognition!      # Toggle precognition
```

### Querying Values

```
:set option?            # Show current value
:set option??           # Show modification history
:set                    # Show all modified options
:set all                # Show all options with values
```

### Resetting Values

```
:set option&            # Reset to default value
:set option^            # Pop one modification (undo last change)
```

## Available Options

### Model

| Option | Type | Description |
|--------|------|-------------|
| `model` | string | Current LLM model (e.g., `claude-3-5-sonnet`, `gpt-4o`) |

### Thinking / Reasoning

| Option | Type | Description |
|--------|------|-------------|
| `thinking` | bool | Show thinking/reasoning tokens in this client (TUI-local) |
| `thinkingbudget` | preset | Token budget for extended thinking (presets only) |

**Thinking Budget Presets:**

| Preset | Tokens | Description |
|--------|--------|-------------|
| `off` | 0 | Disable extended thinking |
| `minimal` | 512 | Brief reasoning |
| `low` | 1024 | Light reasoning |
| `medium` | 4096 | Moderate reasoning |
| `high` | 8192 | Thorough reasoning |
| `max` | unlimited | Maximum reasoning |

`thinkingbudget` accepts presets only — a raw token count like
`:set thinkingbudget=8000` is rejected with the list of valid presets.

Examples:
```
:set thinkingbudget=high        # Use preset
:set thinkingbudget=off         # Disable thinking
```

### Display

| Option | Type | Description |
|--------|------|-------------|
| `syntax_theme` | string | Syntax highlighting theme for code blocks and diffs. Validated against the loaded theme set; `derived` follows the UI colorscheme |
| `show_diffs` | bool | Render inline diffs for file-edit tool calls |
| `completion_style` | enum | Popup presentation: `auto` (minimal anchored boxes for `@`/`[[` completions, full-width panel for `/` and `:`), `panel`, or `minimal` |

### Agent Loop

These sync to the daemon and are session-scoped:

| Option | Type | Description |
|--------|------|-------------|
| `maxiterations` | number/`none` | Cap on agent loop iterations per turn |
| `executiontimeout` | seconds/`none` | Tool execution timeout |
| `outputvalidation` | string | Output validation mode |
| `validationretries` | number | Retries when output validation fails |

### Context Management

| Option | Type | Description |
|--------|------|-------------|
| `contextbudget` | number/`none` | Context token budget (alias: `context_budget`) |
| `contextstrategy` | enum | `truncate`, `sliding_window`, or `summarize` |
| `contextwindow` | number/`none` | Sliding window size in message pairs |
| `autocompact_threshold` | 0.0–1.0/`off`/`default` | Auto-compaction trigger as a fraction of the context budget |

### Precognition

| Option | Type | Description |
|--------|------|-------------|
| `precognition` | bool | Toggle precognition (auto-RAG context injection, daemon-side) |
| `precognition.results` | number | Number of precognition results to inject (1–20, default: 5) |

### Permissions

| Option | Type | Description |
|--------|------|-------------|
| `perm.show_diff` | bool | Show diffs in permission modals by default |
| `perm.autoconfirm_session` | bool | Auto-approve all permissions for the session |
| `perm.full_commands` | bool | Show the full command/args (wrapped) in permission prompts; off = compact one-line view. Default: on |

### Unknown Keys

A key the classifier doesn't recognize is not an error: it is stored locally
(so `:set key?` round-trips) **and** mirrored into the daemon's app-config
store, so `:lua cru.config.get(key)` and plugins see the same typed value.

## The `:model` Command

Switch models at runtime:

```
:model                  # Open model picker popup
:model <name>           # Switch directly to model
```

The model picker shows available models from your configured provider. Navigate with arrow keys, select with Enter.

Examples:
```
:model claude-3-5-sonnet
:model gpt-4o
:model llama3.2
```

Model changes persist for the session and sync to the daemon.

## The `:pick` Command

Open a fuzzy picker popup:

```
:pick                   # Pick from notes, files, and commands
:pick notes             # Notes from your kiln
:pick files             # Workspace files
:pick commands          # Slash and REPL commands
```

Selecting a note inserts a `[[wikilink]]`, a file inserts an `@path`
attachment, and a command puts the command in the input. (`:pick sessions`
is accepted but currently lists nothing — sessions aren't tracked in TUI
state yet.)

## Other Commands

```
:quit                   # Exit chat (alias: :q)
:help [topic]           # Show help (alias: :h; topics: commands, keys, config, tools)
:clear                  # Clear conversation (start fresh)
:undo [N]               # Undo the last N agent turns (also /undo)
:export <path>          # Export session to markdown (~ expands)
:messages               # Toggle notification drawer
:palette                # Open command palette (F1)
:mcp                    # MCP servers with connection status and tool counts
:plugins                # Loaded plugins with state and version
:reload [name]          # Reload one plugin, or all when no name given
:config                 # Show current configuration summary
:lua <expr>             # Evaluate a Lua expression in the daemon's plugin
                        # runtime; result renders as a system message (:= works too)
```

## Configuration Layers

The `:set` command modifies a **runtime overlay** on top of your base configuration:

```
┌─────────────────────────────┐
│  :set commands (runtime)    │ ← Highest priority
├─────────────────────────────┤
│  Environment variables      │
├─────────────────────────────┤
│  ~/.config/crucible/        │
│  config.toml (user)         │
├─────────────────────────────┤
│  Built-in defaults          │ ← Lowest priority
└─────────────────────────────┘
```

Runtime changes do **not** persist to config files. They last for the current session only.

### Modification Tracking

Use `:set option??` to see where a value came from:

```
:set thinkingbudget??
# Output:
# thinkingbudget = high
#   [Command] high (2025-01-20 14:30:00)
#   [File] medium (base config)
```

## Option Shortcuts

Some options have short aliases:

| Shortcut | Full Path |
|----------|-----------|
| `model` | (dynamic — resolved per provider) |
| `thinking` | (virtual, TUI-only) |
| `thinkingbudget` | `llm.thinking_budget` |
| `syntax_theme` | `cli.highlighting.theme` |

## Examples

### Quick Model Switch
```
:model gpt-4o
```

### Enable Extended Thinking
```
:set thinking
:set thinkingbudget=high
```

### Check Current Config
```
:set model?
:set thinkingbudget?
```

### Reset to Defaults
```
:set thinkingbudget&
```

### Debug Configuration
```
:set all                # See everything
:set                    # See what you changed
:set model??            # See modification history
```

## See Also

- [[Help/TUI/Index]] — TUI overview
- [[Help/TUI/Keybindings]] — Keyboard shortcuts
- [[Help/Core/Sessions]] — Session management
- [[Help/Configuration]] — Config file reference
- [[Help/Config/llm]] — LLM provider configuration
