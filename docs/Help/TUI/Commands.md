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
| `:set option^` | Drop the top layer and reveal the one beneath |
| `:set option=` | Remove the key — an assignment with nothing after it |
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
:set contextbudget=128000
```

### Boolean Options

```
:set option=            # REMOVE the key (nothing after the `=`)
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
:set option&            # Reset: drop the layer `:set` writes
:set option^            # Pop: drop the highest layer, and show the one under it
```

For a TUI-local option these walk this client's own stack of modifications.
For an app-config key they call the daemon, which owns the layers — see
[App-Config Keys](#app-config-keys) for the layer order and what each verb
drops. Neither verb edits a file: every layer returns at the next daemon
start.

## Available Options

### Model

| Option | Type | Description |
|--------|------|-------------|
| `model` | string | Current LLM model (e.g., `claude-3-5-sonnet`, `gpt-4o`) |

### Thinking / Reasoning

| Option | Type | Description |
|--------|------|-------------|
| `thinking` | bool | Show thinking/reasoning tokens in this client (TUI-local) |

Crucible sets no cap on how much a model reasons: the model decides, and the
provider default applies. `thinking` controls the display only.

Examples:
```
:set thinking                   # Show reasoning blocks
:set nothinking                 # Hide them
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

### Context Management

| Option | Type | Description |
|--------|------|-------------|
| `contextbudget` | number/`none` | Context token budget (alias: `context_budget`) |
| `contextstrategy` | enum | `truncate`, `sliding_window`, or `summarize` |

### Precognition

| Option | Type | Description |
|--------|------|-------------|
| `precognition` | bool | Toggle precognition (auto-RAG context injection, daemon-side) |

### Permissions

| Option | Type | Description |
|--------|------|-------------|
| `perm.show_diff` | bool | Show diffs in permission modals by default |
| `perm.autoconfirm_session` | bool | Auto-approve all permissions for the session |
| `perm.full_commands` | bool | Show the full command/args (wrapped) in permission prompts; off = compact one-line view. Default: on |

### App-Config Keys

A key the classifier doesn't recognize is not an error: it is app config, and
the daemon store owns it. `:set` writes it there, then reads the store back
and shows you that answer, so `:lua cru.config.get(key)` and plugins see
exactly what `:set key?` shows.

Every spelling goes to the same store:

| Spelling | Verb | What it does |
|----------|------|--------------|
| `:set key?` | `config.get` | Show the value the daemon holds |
| `:set key??` | `config.get` + `config.origin` | Show the value with the source that owns it, and its file and line |
| `:set key=value` | `config.set` | Write the value for this run, then show what the store kept |
| `:set key&` | `config.reset` | Drop the layer `:set` writes, so the key returns to what the defaults and the config files give |
| `:set key^` | `config.pop` | Drop the highest layer holding the key, and show the layer under it |

A dotted key is a path, not a name with a dot in it: `:set myplugin.debug=1`
writes where `cru.config.set { myplugin = { debug = true } }` writes, and
`:set myplugin.debug?` reads it back. A write names one key and leaves every
sibling alone; it can never remove a key. The verb that removes one,
`config.unset`, has no `:set` spelling yet — call it over the RPC.

The TUI keeps no copy of its own. If the daemon refuses the write — the seven
keys that name where the daemon acts are refused at runtime — you get a
warning that names the key, and no value is recorded. `&` and `^` are refused
for the same keys, and for the same reason: both change what the store holds.

The config store keeps the layers it merged, lowest first:

```
default < plugin < settings < toml < lua < registered < cli < rpc
```

`&` and `^` drop layers from that stack and merge again, so what they show is
what the merge rule gives — never a second answer beside it. `&` drops the
`rpc` layer, which is what `:set key=value` writes: the key returns to the
value the next boot would give it. `^` drops one layer per press, so a key
written in both `settings.json` and `init.lua` answers with the `init.lua`
value, and after one `^` with the `settings.json` value.

Both verbs work in memory only, and neither edits a file. Every layer returns
at the next daemon start. To change a durable preference, edit `init.lua`, or
save it through the web settings page, which writes `settings.json`. That save
also drops the `rpc` layer for the key it saves, so a `:set` you made earlier
does not hide the value you just saved. A key your `init.lua` holds is refused
instead, and your `:set` value stands.

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
│  init.lua (you)             │
├─────────────────────────────┤
│  ~/.config/crucible/        │
│  settings.json (the UI)     │
├─────────────────────────────┤
│  Built-in defaults          │ ← Lowest priority
└─────────────────────────────┘
```

Runtime changes do **not** persist to config files. They last for the current session only.

### Modification Tracking

Use `:set option??` to see where a value came from:

```
:set contextbudget??
# Output:
# contextbudget = 128000
#   [Command] 128000 (2025-01-20 14:30:00)
#   [File] 64000 (base config)
```

## Option Shortcuts

Some options have short aliases:

| Shortcut | Full Path |
|----------|-----------|
| `model` | (dynamic — resolved per provider) |
| `thinking` | (virtual, TUI-only) |
| `syntax_theme` | `cli.highlighting.theme` |

## Examples

### Quick Model Switch
```
:model gpt-4o
```

### Show Extended Thinking
```
:set thinking
```

### Check Current Config
```
:set model?
:set contextbudget?
```

### Reset to Defaults
```
:set contextbudget&
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
