---
title: "CLI Command Reference"
description: "Complete reference for Crucible CLI commands"
---

Complete reference for all Crucible CLI commands.

## Core Commands

| Command | Description |
|---------|-------------|
| `cru process` | Process kiln and sync to database |
| `cru stats` | Show kiln statistics |
| `cru status` | Show storage status and statistics |
| `cru config` | Configuration management |

## Agent Commands

| Command | Description |
|---------|-------------|
| `cru chat` | Interactive chat with agents |
| `cru agents list` | List available agents |
| `cru agents info <name>` | Show agent details |
| `cru mcp` | Start MCP server for external tools |

## Management Commands

| Command | Description |
|---------|-------------|
| `cru storage` | Storage management and operations |
| `cru tasks` | Task harness management |
| `cru daemon` | Daemon management (start, stop, status) |
| `cru skills` | Agent skills management |

## Global Options

```
-l, --log-level <LEVEL>     Set log level (off, error, warn, info, debug, trace)
-v, --verbose               Enable verbose logging (--log-level=debug)
-C, --config <PATH>         Config file path (defaults to ~/.config/crucible/config.toml)
-f, --format <FORMAT>       Output format: table, json, csv (default: table)
    --embedding-url <URL>   Embedding service URL (overrides config)
    --embedding-model <MODEL> Embedding model name (overrides config)
    --no-process            Skip file processing on startup
    --process-timeout <SEC> Processing timeout in seconds (default: 300)
-h, --help                  Show help
-V, --version               Print version
```

## See Also

- [process](./process/) - Processing pipeline details
- [chat](./chat/) - Chat command reference
- [stats](./stats/) - Statistics command
- [storage](../config/storage/) - Storage configuration
