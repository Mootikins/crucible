---
title: CLI
description: Complete reference for Crucible CLI commands
tags:
  - help
  - cli
  - reference
status: implemented
---

# CLI Command Reference

Complete reference for all Crucible CLI commands.

## Core Commands

| Command | Description |
|---------|-------------|
| `cru chat` | Interactive AI chat with session persistence and tool access |
| `cru process` | Process markdown files through the pipeline (parse, enrich, store) |
| `cru init` | Initialize a new kiln (Crucible workspace) |
| `cru stats` | Display kiln statistics |
| `cru status` | Display storage status and statistics for the knowledge base |
| `cru models` | List available models from configured LLM provider |

## Agent & Integration Commands

| Command | Description |
|---------|-------------|
| `cru agents` | Manage agent cards (list, show, validate) |
| `cru mcp` | Start MCP server exposing Crucible tools for external AI agents |
| `cru skills` | Discover and manage agent skills (list, show, search) |
| `cru tools` | Discover and manage tools (list, show) |

## Session & Configuration Commands

| Command | Description |
|---------|-------------|
| `cru session` | Manage chat sessions (list, show, resume, export, search) |
| `cru config` | Manage Crucible configuration (initialize, view, export) |
| `cru auth` | Manage LLM provider credentials (login, logout, list) |
| `cru set` | Configure a running session's settings (same syntax as TUI :set) |

## System & Development Commands

| Command | Description |
|---------|-------------|
| `cru daemon` | Manage the Crucible daemon (start, stop, status, logs) |
| `cru storage` | Manage storage operations (migration, verification, backup, cleanup) |
| `cru tasks` | Manage tasks from a TASKS.md file (list, next, pick, done) |
| `cru plugin` | Manage and develop Lua plugins |
| `cru web` | Start the web UI server for browser-based chat |
| `cru doctor` | Run installation diagnostics (daemon, config, providers, kiln, embeddings) |

## Global Options

```
-l, --log-level <LEVEL>     Set log level (off, error, warn, info, debug, trace)
-v, --verbose               Enable verbose logging (--log-level=debug)
-C, --config <PATH>         Config file path (defaults to ~/.config/crucible/config.toml)
-f, --format <FORMAT>       Output format: table, json, csv (default: table)
    --embedding-url <URL>   Embedding service URL (overrides config)
    --embedding-model <MODEL> Embedding model name (overrides config)
    --standalone            Run with in-process daemon (no background server required)
-h, --help                  Show help
-V, --version               Print version
```

## See Also

- [[Help/CLI/process]] - Processing pipeline details
- [[Help/CLI/chat]] - Chat command reference
- [[Help/CLI/stats]] - Statistics command
- [[Help/Config/storage]] - Storage configuration
