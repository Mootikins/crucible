---
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
| `cru stats` | View kiln statistics |
| `cru chat` | Chat with context |
| `cru mcp` | Start MCP server |

## cru (Default)

Running `cru` with no arguments starts interactive chat:

```bash
cru
```

This is the primary way to interact with your kiln. The AI agent can search, read, and (in act mode) modify your notes.

### Chat Modes

**Plan Mode** (default): Read-only, agent explores but doesn't modify
```
/plan
```

**Act Mode**: Agent can create and modify notes
```
/act
```

### Useful Commands in Chat

- `/help` - Show available commands
- `/plan` - Switch to read-only mode
- `/act` - Enable write mode
- `/clear` - Clear conversation history
- `Shift+Tab` - Cycle modes
- `Ctrl+C` - Cancel (double to exit)

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

**Use internal agent:**
```bash
cru chat --internal --provider ollama "Summarize my notes on testing"
```

**Specify model:**
```bash
cru chat --internal --provider openai --model gpt-4o "Help me plan"
```

## cru config

Manage configuration:

```bash
# Show current config
cru config show

# Show config file location
cru config path

# Initialize default config
cru config init
```

## cru mcp

Start the MCP server for external tool integration:

```bash
cru mcp --stdio
```

This exposes your kiln to AI tools like Claude Code.

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

All search happens through the chat interface or MCP tools:

```bash
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

# Skip processing on startup
cru --no-process chat
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
