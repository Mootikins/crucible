---
title: doctor
description: Run installation diagnostics for Crucible
tags:
  - reference
  - cli
---

# cru doctor

Run bounded installation diagnostics for your Crucible setup.

## Synopsis

```
cru doctor
```

## Description

The `doctor` command runs five targeted health checks against your Crucible installation. It's the fastest way to diagnose setup problems after a fresh install or when something stops working.

### Checks performed

| # | Check | Pass condition | Fail suggestion |
|---|-------|---------------|-----------------|
| 1 | Daemon reachability | `DaemonClient::connect()` succeeds | `cru daemon start` |
| 2 | Config validity | Config file exists and parses without errors | `cru config init` |
| 3 | Provider connectivity | Each configured LLM provider responds within 2 seconds | Check provider URL and service status |
| 4 | Kiln accessibility | Kiln path exists, is a directory, and is writable | `cru init` |
| 5 | Embedding backend | FastEmbed compiled in, or Ollama reachable with embeddings | Enable `fastembed` feature or configure Ollama |

### Exit codes

- **0** if all checks pass (warnings are allowed)
- **1** if any check fails

Warnings (read-only kiln, no providers configured, config parse errors) are reported but don't cause a non-zero exit.

## Examples

```bash
# Run all checks
cru doctor

# Typical healthy output
✓ Daemon running
✓ Config found at ~/.config/crucible/config.toml
✓ Provider reachable: default (ollama)
✓ Kiln accessible at ~/notes
✓ Embeddings available (fastembed)

All 5 checks passed.
```

## See Also

- [[Help/CLI/Index]] - Full CLI command reference
- [[Help/Config/storage]] - Storage configuration
