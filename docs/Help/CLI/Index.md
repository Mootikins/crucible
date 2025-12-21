---
title: CLI Command Reference
description: Complete reference for Crucible CLI commands
tags:
  - help
  - cli
  - reference
---

# CLI Command Reference

Complete reference for all Crucible CLI commands.

## Core Commands

| Command | Description |
|---------|-------------|
| `cru process` | Process vault and sync to database |
| `cru stats` | Show vault statistics |
| `cru search` | Search notes by content |
| `cru semantic` | Semantic similarity search |
| `cru fuzzy` | Fuzzy file name search |

## Agent Commands

| Command | Description |
|---------|-------------|
| `cru chat` | Interactive chat with agents |
| `cru agents list` | List available agents |
| `cru agents info` | Show agent details |

## Database Commands

| Command | Description |
|---------|-------------|
| `cru repl` | Start SurrealDB REPL |

## Global Options

```
--kiln, -k <PATH>    Path to kiln (default: $CRUCIBLE_KILN_PATH)
--verbose, -v        Increase verbosity
--quiet, -q          Suppress output
--help, -h           Show help
```

## See Also

- [[Help/CLI/process]] - Processing pipeline details
- [[Help/Config/storage]] - Storage configuration
