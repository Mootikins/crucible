---
title: Basic Commands
description: Essential CLI commands for everyday use
tags:
  - guide
  - cli
  - beginner
order: 3
---

# Basic Commands

This guide covers the essential Crucible commands you'll use daily.

## Overview

| Command | Purpose |
|---------|---------|
| `cru` | Start interactive chat |
| `cru process` | Index notes |
| `cru search` | Search notes (text + semantic) |
| `cru stats` | View kiln statistics |
| `cru chat` | Chat with context |
| `cru mcp` | Start MCP server |

## cru (Default)

Running `cru` with no arguments starts interactive chat:

```bash
cru
```

This is the primary way to interact with your kiln. The AI agent can search, read, and (in auto mode) modify your notes.

### Chat Modes

**Normal Mode** (default): Agent reads freely; asks permission before writes or commands
```
/default
```

**Plan Mode**: Read-only; agent explores but doesn't modify
```
/plan
```

**Auto Mode**: Agent can create and modify notes without prompting
```
/auto
```

### Useful Commands in Chat

- `/default` - Switch to normal (ask-for-writes) mode
- `/plan` - Switch to read-only mode
- `/auto` - Enable full-access mode
- `/mode` - Cycle through modes
- `/undo` - Undo the last exchange
- `/help` - Show help
- `Shift+Tab` - Cycle modes
- `Ctrl+C` - Cancel (double to exit)

Any other `/command` is forwarded to the agent as chat text. To resume a
previous session, use `cru chat --resume <id>` or `cru session open <id>`; to
switch models, use `:model`.

## cru process

Index your notes for search and AI features:

```bash
cru process
```

### Options

**Force full reprocessing:**
```bash
cru process --force
```

**Watch for changes:**
```bash
cru process --watch
```

**Preview without processing:**
```bash
cru process --dry-run
```

### When to Run

- After adding many new notes
- After major reorganization
- Before important searches
- First time setup

See [[Help/CLI/process]] for full documentation.

## cru stats

View kiln statistics:

```bash
cru stats
```

Output shows:
- Total files
- Markdown file count
- Total size
- Kiln path

Useful for:
- Verifying kiln configuration
- Monitoring growth
- Quick health check

See [[Help/CLI/stats]] for full documentation.

## cru chat

Start chat with a specific message:

```bash
cru chat "What do I know about Rust?"
```

### Options

**Choose a provider (internal agent):**
```bash
cru chat --provider ollama "Summarize my notes on testing"
```

**Specify model:**
```bash
cru chat --provider openai --set model=gpt-4o "Help me plan"
```

There is no `--model` flag — model selection uses the same `--set` syntax as
the TUI `:set` command.

## cru config

Manage configuration:

```bash
# Show current config
cru config show

# Show where each value came from (file, env, cli, default)
cru config show --sources

# Initialize default config
cru config init
```

## cru mcp

Start the MCP server for external tool integration:

```bash
# Default: SSE transport on port 3847
cru mcp

# Stdio transport (for clients like Claude Desktop)
cru mcp --stdio
```

This exposes your kiln to AI tools like Claude Code. The server is hosted by
the background daemon; pass the global `--standalone` flag to run it in-process
instead.

## cru status

Check storage status:

```bash
cru status
```

Shows database connection info and storage statistics.

## Command Patterns

### Daily Workflow

```bash
# Morning: Check what's there
cru stats

# Working: Search and explore via chat
cru chat "What are my open tasks?"

# Adding notes: Keep index fresh
cru process --watch
```

### Finding Information

Search directly, through chat, or via MCP tools:

```bash
# Direct search (text + semantic)
cru search "project planning"

# Interactive exploration
cru chat "Help me find notes about project planning"

# Or use MCP with external tools
cru mcp --stdio
```

### Maintenance

```bash
# Full reindex after changes
cru process --force

# Check for issues
cru stats

# View current configuration
cru config show
```

## Global Options

These work with any command:

```bash
# Specify config file
cru -C /path/to/config.toml stats

# Verbose output
cru --verbose process

# JSON output (where supported)
cru stats --format json

```

## Getting Help

```bash
# General help
cru --help

# Command-specific help
cru process --help
cru chat --help
```

## Next Steps

- [[Getting Started]] - Full setup guide
- [[Your First Kiln]] - Create a kiln from scratch
- [[Help/CLI/Index]] - Full CLI reference
- [[Help/CLI/process]] - Processing options

## See Also

- `:h cli` - CLI reference
- `:h process` - Processing reference
- `:h config` - Configuration options
