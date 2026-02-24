---
description: Configuration reference for Crucible storage backends
tags:
  - help
  - config
  - storage
---

# Storage Configuration

Configure where and how Crucible stores your data.

## Default Storage

By default, Crucible uses **SQLite** - fast, lightweight, and recommended for most users.

Data is stored in:
- `<kiln_path>/.crucible/crucible-sqlite.db` (in your kiln)
- Or `~/.local/share/crucible/` (Linux) / `~/Library/Application Support/crucible/` (macOS)

## Storage Modes

```toml
[storage]
# Storage mode: "sqlite" (default), "daemon", or "lightweight"
mode = "sqlite"
```

| Mode | Description | Use Case |
|------|-------------|----------|
| `sqlite` | SQLite database (default) | Most users, single-user |
| `daemon` | Connect to crucible-daemon | Multi-client, cloud |
| `lightweight` | Minimal mode with LanceDB | Testing, CI |

## SQLite (Default)

No configuration needed - just works:

```toml
[storage]
mode = "sqlite"
```

## Daemon Mode

Connect to the daemon for multi-client support (auto-started on first use):

```toml
[storage]
mode = "daemon"
# Socket auto-detected from $CRUCIBLE_SOCKET or $XDG_RUNTIME_DIR/crucible.sock
# Only set explicitly if using a custom socket path:
# daemon_socket = "/tmp/crucible.sock"
```

## See Also

- [[Help/CLI/process]] - Processing pipeline
- [[Help/CLI/stats]] - Database statistics
