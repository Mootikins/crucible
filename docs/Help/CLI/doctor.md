---
title: "cru doctor"
description: Run installation diagnostics for Crucible
tags:
  - reference
  - cli
---

# cru doctor

Run bounded installation diagnostics for your Crucible setup.

## Synopsis

```
cru doctor [-C <config>] [-f json]
```

## Description

The `doctor` command runs targeted health checks against your Crucible installation. It's the fastest way to diagnose setup problems after a fresh install or when something stops working.

### Checks performed

| Check | Pass condition | Fail suggestion |
|-------|---------------|-----------------|
| Daemon reachability | `DaemonClient::connect()` succeeds | `cru daemon start` |
| Config validity | Config file exists and parses without errors | `cru config init` |
| Provider connectivity | Each configured LLM provider responds within 2 seconds | Check provider URL and service status |
| Kiln accessibility | Kiln path exists, is a directory, and is writable | `cru init` |
| Embedding backend | FastEmbed compiled in, or Ollama reachable | Enable the `fastembed` feature or configure Ollama |
| Plugins | The daemon answers `plugin.list` | Warning only; skipped entirely if the daemon is down |
| Kiln references | Every kiln named by a `[projects.*]` entry exists in `[kilns]` | Add the kiln to `[kilns]` or drop the reference |
| Config validation | The loaded config passed structural validation | See the Config check above |

Not every check emits a line on every run: the plugin check is skipped when the daemon is
unreachable, the kiln-reference check is skipped when no projects are registered, and an
unreachable provider produces one line per provider. The count in the summary is the number
of lines actually emitted, so it varies with your setup.

### Exit codes

- **0** if all checks pass (warnings are allowed)
- **1** if any check fails

Warnings (read-only kiln, no providers configured, config parse errors) are reported but don't cause a non-zero exit. `-f json` prints the raw results and always exits 0.

## Examples

```bash
# Run all checks
cru doctor

# Machine-readable results
cru doctor -f json
```

Typical healthy output:

```
Crucible Doctor - Installation Health Check
───────────────────────────────────────────
✓ Daemon running
✓ Config found at /home/you/.config/crucible/config.toml
✓ All 1 provider(s) reachable
✓ Kiln accessible at /home/you/notes
✓ Embeddings available (fastembed)
✓ 3 plugin(s) loaded
✓ Config parsed and validated

All 7 checks passed.
```

## See Also

- [[Help/CLI/Index]] - Full CLI command reference
- [[Help/Config/storage]] - Storage configuration
