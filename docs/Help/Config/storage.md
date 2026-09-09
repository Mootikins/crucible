---
title: "Storage Configuration"
description: Configuration reference for Crucible storage
tags:
  - help
  - config
  - storage
---

# Storage Configuration

Crucible uses a **daemon-backed storage architecture**. All storage operations go through the daemon, which manages SQLite internally.

## How It Works

The daemon is the only storage backend. It starts automatically on first use via `DaemonClient::connect_or_start()` and manages all data access.

It also stops on its own. A signal (SIGTERM or SIGINT) shuts it down cleanly, and it exits after `server.idle_shutdown_minutes` (default 30) with no work outstanding — no connected client, no turn in flight, no background job, and no maintenance pass running. A turn holds the daemon open even after the client that started it has gone, which is what lets you close the TUI on a long autonomous run. It exits at once, rather than after the window, if its socket file has gone, because no client can reach it again. Sessions are persisted, so the next command starts a fresh daemon and resumes them. Set the key to `0` to keep a daemon running for ever; a daemon with `schedules` configured never arms the timer.

Data is stored in:
- `<kiln_path>/.crucible/crucible-sqlite.db` (notes, metadata, FTS index, vector embeddings)

## Configuration

The `[storage]` section was removed. Its one field, `idle_timeout_secs`, never
had an effect; the idle window is `server.idle_shutdown_minutes` above. A config
file that still contains a `[storage]` section loads without an error; the
daemon ignores it.

## Daemon Socket

The daemon listens on a Unix socket, resolved in order:

1. `$CRUCIBLE_SOCKET` environment variable
2. `$XDG_RUNTIME_DIR/crucible.sock`
3. `<tmpdir>/crucible-<uid>/crucible.sock` — a per-uid directory created 0700.
   The daemon refuses to start if it exists and is a symlink, is owned by someone
   else, or is group/world accessible; it never repairs one, because chmod-ing a
   path you do not own is itself a capability.

## Backward Compatibility

Old `storage.mode` values (`sqlite`, `lightweight`, `daemon`) are silently accepted but have no effect. The daemon is always used. Remove `storage.mode` from your config to avoid the deprecation warning.

## Source of Truth

The database is derived data — a cache built from your markdown files. You can delete `.crucible/crucible-sqlite.db` and rebuild with `cru process --force` at any time.

## See Also

- [[Help/CLI/process]] - Processing pipeline
- [[Help/CLI/stats]] - Database statistics
