---
title: Plugin Manifest
description: Plugin manifest format for declaring metadata, dependencies, and capabilities
status: implemented
tags:
  - extending
  - plugins
  - configuration
aliases:
  - plugin.yaml
  - Manifest Format
---

# Plugin Manifest

Plugins can include a `plugin.yaml` manifest to declare metadata, dependencies, capabilities, and exports. While simple plugins work without a manifest, adding one enables:

- Dependency management
- Capability-based permissions
- Plugin enable/disable
- Version tracking

## Location

The manifest file goes in the plugin directory root:

```
plugins/my-plugin/
├── plugin.yaml     # Manifest
├── init.lua        # Main entry point
└── lib/            # Additional files
```

Accepted filenames: `plugin.yaml`, `plugin.yml`, `manifest.yaml`, `manifest.yml`

## Minimal Manifest

```yaml
name: my-plugin
version: "1.0.0"
```

Only `name` and `version` are required. Everything else has sensible defaults.

## Full Example

```yaml
name: task-manager
version: "2.1.0"
description: Task management with TASKS.md format
author: Your Name <you@example.com>
license: MIT

main: lua/init.lua

# Informational — not enforced (all plugins share one Lua VM). An invalid
# value fails manifest parsing and the plugin never loads.
capabilities:
  - filesystem
  - shell
  - kiln

dependencies:
  - name: core-utils
  - name: markdown-parser
    optional: true

enabled: true
```

## Fields Reference

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Plugin identifier (lowercase, hyphens allowed) |
| `version` | string | Semantic version (e.g., "1.0.0", "2.1.0-beta") |

### Metadata Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `description` | string | "" | Brief description |
| `author` | string | "" | Author name and email |
| `license` | string | null | License identifier (MIT, Apache-2.0, etc.) |

### Entry Point Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `main` | string | "init.lua" | Main file path relative to plugin dir |

### Capabilities

Capabilities declare what resources the plugin needs access to:

```yaml
capabilities:
  - filesystem    # Read/write files outside kiln
  - network       # Make HTTP requests
  - shell         # Execute shell commands
  - kiln          # Access knowledge kiln
  - agent         # Interact with AI agents
  - ui            # Create custom UI views
  - config        # Access user configuration
  - system        # Access system information
  - websocket     # Open WebSocket connections
```

Those nine values are the complete set.

Capabilities are informational: all plugins share one Lua VM, so per-plugin
module gating is not enforced. There is no restricted sandbox and no
grant prompt — declare capabilities as documentation of what the plugin
touches. An invalid value fails manifest parsing and the plugin never loads.

### Dependencies

Declare dependencies on other plugins:

```yaml
dependencies:
  - name: core-utils
  - name: optional-dep
    optional: true        # Won't block load if missing
```

Dependencies are matched by name only — there are no version constraints, and
a `version:` key under a dependency is silently ignored. Plugins load in
dependency order automatically, and a missing required dependency blocks the
load.

### Exports

```yaml
exports:
  tools:
    - my_tool_1
  commands:
    - /my-command
  auto_discover: false
```

**Parsed but unused.** The `exports` block (its `tools`, `commands`, `views`,
`handlers` lists and the `auto_discover` flag) is accepted by the manifest
parser and then consulted by nothing. Plugin callables come exclusively from
the spec table `init.lua` returns; annotated functions are not scanned for
plugins. You can omit the block entirely.

### Configuration

There is no manifest-level config schema. Plugin configuration is the
`[plugins.<name>]` section of `config.toml`, handed to your `setup()` at
load, overridable from `~/.config/crucible/init.lua` — see
[[Help/Extending/Creating Plugins]] for the precedence rules (Lua beats
TOML). A `config:` block in plugin.yaml is ignored.


### Enable/Disable

```yaml
enabled: false  # Plugin won't load (default: true)
```

Use this to temporarily disable a plugin without removing it.

## Plugin Lifecycle

1. **Discovery**: Crucible scans plugin directories for manifests
2. **Validation**: Manifest is parsed and validated
3. **Dependency Resolution**: Load order is determined
4. **Loading**: Main file is executed
5. **Initialization**: Optional init function is called
6. **Export Discovery**: Tools, commands, views are registered

## Programmatic Access

There is no Lua API for querying the plugin manager — `cru.plugins` does not
exist. To see what is installed and loaded, use the `plugin.list` RPC (the TUI
and `cru plugin list` consume it).

`crucible-lua` does contain a `PluginManager` registration API with builders
(`ToolBuilder`, `HandlerBuilder`, ...), but registrations made through it feed
only that standalone manager — the daemon's registry never consults them, so
nothing registered that way reaches an agent. Treat it as internal; the spec
table a plugin returns is the one registration path that works end to end.

## Validation Rules

### Plugin Name
- Lowercase letters, numbers, hyphens, underscores
- Must start with a letter
- Cannot end with hyphen or underscore
- Maximum 64 characters

### Version
- Semver format: MAJOR.MINOR.PATCH
- Optional prerelease: 1.0.0-beta, 1.0.0-rc.1

## See Also

- [[Help/Extending/Creating Plugins]] - Plugin basics
- [[Help/Extending/Custom Tools]] - Tool development
- [[Help/Extending/Event Hooks]] - Hook system
- [[Help/Config/workspaces]] - Workspace configuration
