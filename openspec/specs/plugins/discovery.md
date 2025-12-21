# Plugin Discovery

**Status**: Implemented
**System**: plugins
**Related**: [hooks.md](./hooks.md)

## Overview

Crucible uses a unified plugin discovery system. All extensions (tools, hooks) are discovered from `plugins/` directories. Attributes within scripts (`#[tool(...)]`, `#[hook(...)]`) determine what gets registered.

**Plugins can provide**:
- **Tools** - MCP-compatible functions via `#[tool(...)]`
- **Hooks** - Event handlers via `#[hook(...)]`

## Directory Structure

Plugins are discovered from three locations in priority order:

```
~/.config/crucible/
└── plugins/              # 1. Global personal (user-specific)

KILN/
├── .crucible/
│   └── plugins/          # 2. Kiln personal (gitignored)
└── plugins/              # 3. Kiln shared (version-controlled)
```

### Discovery Priority

Later sources override earlier by name:

1. **Global personal**: `~/.config/crucible/plugins/`
   - User-specific tools and hooks
   - Shared across all kilns
   - Platform-specific: Windows uses `%APPDATA%\crucible\plugins\`

2. **Kiln personal**: `KILN/.crucible/plugins/`
   - Kiln-specific customizations
   - Should be gitignored
   - Private to the user

3. **Kiln shared**: `KILN/plugins/`
   - Version-controlled with the kiln
   - Shared with collaborators
   - Project-specific tools

## Plugin Types

### Single-File Plugin

A `.rn` file with tool or hook attributes:

```rune
// plugins/greet.rn

/// A friendly greeting tool
#[tool(name = "greet", description = "Say hello")]
pub fn greet(name) {
    Ok(format!("Hello, {}!", name))
}
```

### Module Plugin (lazy.nvim-style)

A directory with `mod.rn` entry point:

```
plugins/
└── tasks/
    ├── mod.rn           # Entry point
    ├── parser.rn        # Helper module
    └── commands.rn      # More helpers
```

```rune
// tasks/mod.rn
mod parser;
mod commands;

#[tool(name = "tasks_list")]
pub fn list(path) {
    let tasks = parser::parse_tasks(path)?;
    commands::list_tasks(tasks)
}

#[tool(name = "tasks_next")]
pub fn next(path) {
    let tasks = parser::parse_tasks(path)?;
    commands::next_task(tasks)
}
```

### Mixed Tools and Hooks

A single file can provide both:

```rune
// plugins/github.rn

#[tool(name = "gh_issues", description = "List GitHub issues")]
pub fn list_issues(repo) {
    // Implementation
}

#[hook(event = "tool:before", pattern = "gh_*", priority = 5)]
pub fn rate_limit_github(ctx, event) {
    // Rate limiting logic
    event
}

#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
pub fn transform_github_results(ctx, event) {
    // Transform output
    event
}
```

## DiscoveryPaths API

### Creating Discovery Paths

```rust
use crucible_rune::discovery_paths::DiscoveryPaths;

// With kiln path (includes all three tiers)
let paths = DiscoveryPaths::new("plugins", Some(kiln_path));

// Global only (no kiln paths)
let paths = DiscoveryPaths::new("plugins", None);

// Empty (no defaults, explicit paths only)
let paths = DiscoveryPaths::empty("plugins");
```

### Adding Custom Paths

```rust
// Add single path
let paths = DiscoveryPaths::new("plugins", None)
    .with_path("/custom/plugins".into());

// Add multiple paths
let paths = DiscoveryPaths::new("plugins", None)
    .with_additional(vec![
        "/project/plugins".into(),
        "/shared/plugins".into(),
    ]);
```

### Controlling Defaults

```rust
// Disable defaults (explicit paths only)
let paths = DiscoveryPaths::new("plugins", None)
    .without_defaults()
    .with_path("/only/this/path".into());

// Re-enable defaults
let paths = paths.with_defaults();
```

### Querying Paths

```rust
// Get all paths (combined, deduplicated)
let all = paths.all_paths();  // Vec<PathBuf>

// Get only defaults
let defaults = paths.default_paths();  // &[PathBuf]

// Get only additional
let additional = paths.additional_paths();  // &[PathBuf]

// Get only existing directories
let existing = paths.existing_paths();  // Vec<PathBuf>
```

## Configuration

Discovery paths can be configured via TOML:

```toml
# ~/.config/crucible/config.toml

[discovery.plugins]
additional_paths = [
    "/work/shared-plugins",
    "~/personal-plugins"
]
use_defaults = true
```

### Applying Configuration

```rust
use crucible_rune::discovery_paths::{DiscoveryPaths, DiscoveryConfig};

let config = DiscoveryConfig {
    additional_paths: vec!["/custom/path".into()],
    use_defaults: true,
};

let paths = DiscoveryPaths::new("plugins", Some(kiln_path))
    .with_config(&config);
```

## Discovery Process

1. **Scan directories**: Find all `.rn` files in plugin paths (recursive)
2. **Parse attributes**: Extract `#[tool(...)]` and `#[hook(...)]` attributes
3. **Compile scripts**: Compile Rune source to bytecode
4. **Register**: Add tools to registry, hooks to EventBus

### Example Discovery

```rust
use crucible_rune::{RuneToolRegistry, RuneDiscoveryConfig, HookRegistry};

// Discover tools
let config = RuneDiscoveryConfig::with_defaults(Some(kiln_path));
let tool_registry = RuneToolRegistry::discover_from(config).await?;

// Discover hooks
let mut hook_registry = HookRegistry::new(Some(kiln_path))?;
let hook_count = hook_registry.discover()?;

// Register hooks on event bus
let mut bus = EventBus::new();
hook_registry.register_all(&mut bus);
```

## File Organization

### Naming Conventions

**Recommended**:
- Descriptive names: `filter_test_output.rn`, `github_tools.rn`
- Lowercase with underscores: `my_plugin.rn` not `MyPlugin.rn`
- Category prefix for related plugins: `task_list.rn`, `task_next.rn`

**Avoid**:
- Generic names: `plugin1.rn`, `script.rn`
- Special characters in filenames
- Names longer than 50 characters

### Directory Organization

**Flat structure** (simple projects):
```
plugins/
├── filter_tests.rn
├── log_tools.rn
└── enrich_recipes.rn
```

**Categorized structure** (complex projects):
```
plugins/
├── github/
│   ├── mod.rn
│   ├── issues.rn
│   └── rate_limit.rn
├── tasks/
│   ├── mod.rn
│   └── parser.rn
└── utils/
    └── logging.rn
```

Discovery is recursive - subdirectories are automatically scanned.

## Examples

### Example 1: Global Plugin

Create `~/.config/crucible/plugins/log_tools.rn`:

```rune
/// Log all tool executions (global)
#[hook(event = "tool:after", pattern = "*", priority = 200)]
pub fn log_all_tools(ctx, event) {
    ctx.emit_custom("audit:tool_executed", #{
        tool: event.identifier,
        timestamp: event.timestamp_ms,
    });
    event
}
```

This hook applies to all kilns.

### Example 2: Kiln Shared Plugin

Create `KILN/plugins/project_tools.rn`:

```rune
/// Project-specific deployment tool
#[tool(
    name = "deploy",
    description = "Deploy to staging"
)]
pub fn deploy(env) {
    // Deployment logic shared with team
    Ok("Deployed!")
}
```

This tool is version-controlled and shared with collaborators.

### Example 3: Kiln Personal Plugin

Create `KILN/.crucible/plugins/my_shortcuts.rn`:

```rune
/// Personal shortcut (not shared)
#[tool(name = "quick_test")]
pub fn quick_test() {
    // My personal testing workflow
    Ok("Done")
}
```

This tool is gitignored and personal to the user.

### Example 4: Override Pattern

**Global**: `~/.config/crucible/plugins/test_filter.rn`
```rune
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn test_filter(ctx, event) {
    // Default filtering
    event
}
```

**Kiln override**: `KILN/plugins/test_filter.rn`
```rune
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn test_filter(ctx, event) {
    // Project-specific filtering
    event
}
```

The kiln plugin shadows the global one (same name = override).

## Troubleshooting

### Plugins Not Found

**Check discovery paths**:
```bash
# Verify directories exist
ls -la ~/.config/crucible/plugins/
ls -la KILN/.crucible/plugins/
ls -la KILN/plugins/

# Check for .rn files
find ~/.config/crucible/plugins/ -name "*.rn"
```

**Check logs**:
```bash
# Look for discovery messages
grep "Discovered.*plugin" crucible.log
```

### Plugins Not Executing

**Verify attributes**:
- Check `#[tool(...)]` or `#[hook(...)]` syntax
- Verify event type matches (for hooks)
- Check pattern matches identifier (for hooks)
- Ensure `enabled = true` (for hooks)

**Check compilation**:
- Look for Rune compilation errors in logs
- Verify syntax is correct

### Name Conflicts

If multiple plugins have the same name:
- Later in discovery order wins
- Kiln shared overrides kiln personal overrides global
- Use unique names to avoid unintended overrides

## Best Practices

### Organization

- **Global plugins**: General utilities, logging, audit
- **Kiln shared plugins**: Project-specific tools for the team
- **Kiln personal plugins**: User-specific customizations
- **Subdirectories**: Group related plugins together

### Performance

- **Combine related items**: Put related tools/hooks in one file
- **Use specific patterns**: Reduce unnecessary hook invocations
- **Cache compiled code**: Discovery caches compiled units

### Maintainability

- **Version control kiln shared**: Put project plugins in `KILN/plugins/`
- **Gitignore personal**: Add `.crucible/` to `.gitignore`
- **Document plugins**: Add comments explaining purpose
- **Test discovery**: Verify plugins load on startup

### Security

- **Restrict permissions**: Make plugin directories user-only
- **Review shared plugins**: Audit plugins from collaborators
- **Separate concerns**: Don't mix sensitive and shared plugins
- **Use .gitignore**: Keep personal plugins out of version control

## See Also

- [hooks.md](./hooks.md) - Writing Rune hooks
- `/examples/plugins/` - Example plugin scripts
- `crates/crucible-rune/src/discovery_paths.rs` - Implementation
- `crates/crucible-rune/src/hook_system.rs` - Hook registration
