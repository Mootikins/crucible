---
title: storage
description: Configuration reference for Crucible storage
tags:
  - help
  - config
  - storage
---

# Storage Configuration

Crucible uses a **daemon-backed storage architecture**. All storage operations go through the daemon, which manages SQLite and LanceDB internally.

## How It Works

The daemon is the only storage backend. It starts automatically on first use via `DaemonClient::connect_or_start()` and manages all data access.

Data is stored in:
- `<kiln_path>/.crucible/crucible-sqlite.db` (notes, metadata, FTS index)
- `<kiln_path>/.crucible/lance/` (vector embeddings)

## Configuration

```toml
[storage]
# Seconds of inactivity before daemon auto-shuts down (default: 300)
idle_timeout_secs = 300
```

## Daemon Socket

The daemon listens on a Unix socket, resolved in order:

1. `$CRUCIBLE_SOCKET` environment variable
2. `$XDG_RUNTIME_DIR/crucible.sock`
3. `/tmp/crucible.sock`

## Backward Compatibility

Old `storage.mode` values (`sqlite`, `lightweight`, `daemon`) are silently accepted but have no effect. The daemon is always used. Remove `storage.mode` from your config to avoid the deprecation warning.

## Source of Truth

The database is derived data — a cache built from your markdown files. You can delete `.crucible/crucible-sqlite.db` and rebuild with `cru process --force` at any time.

## See Also

- [[Help/CLI/process]] - Processing pipeline
- [[Help/CLI/stats]] - Database statistics
