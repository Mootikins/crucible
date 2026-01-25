---
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
| `:session list` | List sessions |
| `:session load <id>` | Load session |
| `:quit` / `:q` | Exit chat |
| `:help` | Show help |

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
:set thinkingbudget=8000
:set temperature=0.7
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
:set verbose!           # Toggle verbose mode
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

### Model & Provider

| Option | Type | Description |
|--------|------|-------------|
| `model` | string | Current LLM model (e.g., `claude-3-5-sonnet`, `gpt-4o`) |
| `provider` | string | LLM provider (`ollama`, `openai`, `anthropic`) |

### Thinking / Reasoning

| Option | Type | Description |
|--------|------|-------------|
| `thinking` | bool | Show thinking/reasoning tokens in UI |
| `thinkingbudget` | number/preset | Token budget for extended thinking |

**Thinking Budget Presets:**

| Preset | Tokens | Description |
|--------|--------|-------------|
| `off` | 0 | Disable extended thinking |
| `minimal` | 512 | Brief reasoning |
| `low` | 1024 | Light reasoning |
| `medium` | 4096 | Moderate reasoning |
| `high` | 8192 | Thorough reasoning |
| `max` | unlimited | Maximum reasoning |

Examples:
```
:set thinkingbudget=4096        # Set exact token count
:set thinkingbudget=high        # Use preset
:set thinkingbudget=off         # Disable thinking
```

### Display

| Option | Type | Description |
|--------|------|-------------|
| `theme` | string | Syntax highlighting theme |
| `verbose` | bool | Verbose output mode |

### Generation

| Option | Type | Description |
|--------|------|-------------|
| `temperature` | float | Response randomness (0.0 - 2.0) |
| `maxtokens` | number | Maximum response tokens |

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

Model changes persist for the session and sync to the daemon (if using daemon mode).

## Session Commands

Manage chat sessions:

```
:session list           # Show available sessions
:session load <id>      # Resume existing session
:session new            # Start new session
```

Sessions auto-save and can be resumed across TUI restarts.

## Other Commands

```
:quit                   # Exit chat (alias: :q)
:help                   # Show help
:clear                  # Clear conversation (start fresh)
:palette                # Open command palette
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
# thinkingbudget = 8192
#   [Command] 8192 (2025-01-20 14:30:00)
#   [File] 4096 (base config)
```

## Option Shortcuts

Some options have short aliases:

| Shortcut | Full Path |
|----------|-----------|
| `model` | `llm.providers.{provider}.default_model` |
| `thinking` | (virtual, TUI-only) |
| `thinkingbudget` | `llm.thinking_budget` |
| `theme` | `cli.highlighting.theme` |
| `verbose` | `cli.verbose` |

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
:set temperature&
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
