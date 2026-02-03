---
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
homepage: https://github.com/you/crucible-task-manager
repository: https://github.com/you/crucible-task-manager

main: lua/init.lua
init: setup

capabilities:
  - filesystem
  - shell
  - kiln

dependencies:
  - name: core-utils
    version: ">=1.0"
  - name: markdown-parser
    optional: true

exports:
  tools:
    - tasks_list
    - tasks_add
    - tasks_complete
  commands:
    - /tasks
  views:
    - task-board
  auto_discover: false

config:
  properties:
    default_file:
      type: string
      description: Default TASKS.md location
      default: "TASKS.md"
    auto_archive:
      type: boolean
      default: true
  required: []

keywords:
  - tasks
  - productivity
  - todo

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
| `homepage` | string | null | Project homepage URL |
| `repository` | string | null | Source repository URL |
| `keywords` | string[] | [] | Searchable keywords |

### Entry Point Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `main` | string | "init.lua" | Main file path relative to plugin dir |
| `init` | string | null | Optional init function name to call after load |

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
```

Plugins without declared capabilities run in a restricted sandbox. Users may be prompted to grant capabilities on first use.

### Dependencies

Declare dependencies on other plugins:

```yaml
dependencies:
  - name: core-utils
    version: ">=1.0"      # Version constraint (optional)
  - name: optional-dep
    optional: true        # Won't block load if missing
```

Version constraints support:
- Exact: `"1.0.0"`
- Minimum: `">=1.0"`
- Range: `">=1.0 <2.0"`

Plugins load in dependency order automatically.

### Exports

Explicitly declare what the plugin provides:

```yaml
exports:
  tools:
    - my_tool_1
    - my_tool_2
  commands:
    - /my-command
  views:
    - my-view
  handlers:
    - my_handler
  auto_discover: true   # Also scan files for @tool/@command annotations
```

If `auto_discover` is true (default), Crucible also scans plugin files for annotated functions. Set to `false` to only export explicitly listed items.

### Configuration Schema

Define plugin-specific configuration:

```yaml
config:
  properties:
    api_key:
      type: string
      description: API key for external service
    max_results:
      type: number
      default: 10
    enabled_features:
      type: array
  required:
    - api_key
```

Supported types: `string`, `number`, `boolean`, `array`, `object`

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

```lua
-- In Lua, access plugin manager
local plugins = crucible.plugins

-- List loaded plugins
for name, plugin in pairs(plugins.list()) do
    print(name, plugin.version)
end

-- Check capabilities
if plugins.has_capability("my-plugin", "shell") then
    -- Plugin can run shell commands
end
```

## Programmatic Registration (Rust API)

Tools, commands, views, and handlers can be registered programmatically without annotations. This is useful for:

- Dynamic tool creation at runtime
- Temporary handlers that auto-remove
- Testing and mocking
- Plugin-generated tools

### Builder Pattern

```rust
use crucible_lua::{PluginManager, ToolBuilder, HandlerBuilder};

let mut manager = PluginManager::new();

// Register a tool using the builder
let tool = ToolBuilder::new("my_search")
    .description("Search notes by query")
    .param("query", "string")
    .param_optional("limit", "number")
    .returns("SearchResult[]")
    .build();

let handle = manager.register_tool(tool, None);  // No owner

// Later: remove it
manager.unregister(handle);
```

### Owned Registration

Associate registrations with an owner for bulk removal:

```rust
// Register multiple items owned by a workflow
let tool = ToolBuilder::new("workflow_tool").build();
let handler = HandlerBuilder::new("workflow_handler", "tool:after").build();

manager.register_tool(tool, Some("my_workflow"));
manager.register_handler(handler, Some("my_workflow"));

// Later: remove all items owned by this workflow
let removed_count = manager.unregister_by_owner("my_workflow");
```

### Available Builders

| Builder | Creates | Key Methods |
|---------|---------|-------------|
| `ToolBuilder` | `DiscoveredTool` | `param()`, `param_optional()`, `returns()` |
| `CommandBuilder` | `DiscoveredCommand` | `hint()`, `handler_fn()`, `param()` |
| `HandlerBuilder` | `DiscoveredHandler` | `pattern()`, `priority()`, `handler_fn()` |
| `ViewBuilder` | `DiscoveredView` | `handler_fn()`, `view_fn()` |

### Registration Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `register_tool(tool, owner)` | `RegistrationHandle` | Register a tool (owner: `Option<&str>`) |
| `register_command(cmd, owner)` | `RegistrationHandle` | Register a command |
| `register_view(view, owner)` | `RegistrationHandle` | Register a view |
| `register_handler(handler, owner)` | `RegistrationHandle` | Register a handler |
| `unregister(handle)` | `bool` | Remove by handle |
| `unregister_by_owner(owner)` | `usize` | Remove all with owner |

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
