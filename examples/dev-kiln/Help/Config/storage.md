---
title: Storage Configuration
description: Configuration reference for Crucible storage backends
tags:
  - help
  - config
  - storage
---

# Storage Configuration

Configure where and how Crucible stores your data.

## Default Storage

By default, Crucible uses SurrealDB with RocksDB backend, storing data in:
- `~/.local/share/crucible/` (Linux)
- `~/Library/Application Support/crucible/` (macOS)

## Configuration

```toml
[storage]
# Storage backend: "surrealdb" (default)
backend = "surrealdb"

# Data directory (optional, uses default if not set)
data_dir = "/path/to/data"

# Database name
database = "crucible"

# Namespace
namespace = "crucible"
```

## SurrealDB Options

```toml
[storage.surrealdb]
# Connection mode: "embedded" or "remote"
mode = "embedded"

# For remote mode
endpoint = "ws://localhost:8000"
username = "root"
password = "root"
```

## See Also

- [[Help/CLI/process]] - Processing pipeline
- [[Help/CLI/index]] - Indexing commands
