# Discovery Path Conventions

**Status**: Implemented
**System**: plugins
**Related**: [hooks.md](./hooks.md), [event-system.md](./event-system.md)

## Overview

Crucible uses a unified discovery system to locate Rune scripts across multiple directories. This enables separation of global resources (shared across kilns) and kiln-specific customizations.

**Discovery applies to**:
- **Rune tools** - Script-based MCP tools
- **Rune hooks** - Event handlers
- **Event handlers** - Specialized event processors

## Directory Structure

### Global Paths

Located in `~/.crucible/` (user's home directory):

```
~/.crucible/
├── runes/          # Rune tool scripts (global)
├── hooks/          # Hook scripts (global)
└── events/         # Event handler scripts (global)
```

**Purpose**: Shared across all kilns, user-specific customizations

### Kiln Paths

Located in `KILN/.crucible/` (kiln-specific):

```
KILN/
└── .crucible/
    ├── runes/      # Rune tool scripts (kiln-specific)
    ├── hooks/      # Hook scripts (kiln-specific)
    └── events/     # Event handler scripts (kiln-specific)
```

**Purpose**: Kiln-specific tools and hooks, project customizations

## Search Priority

When discovering resources, paths are searched in this order:

1. **Additional paths** (from configuration)
2. **Global user path** (`~/.crucible/<type>/`)
3. **Kiln-specific path** (`KILN/.crucible/<type>/`)

**Name conflicts**: First match wins. Kiln-specific scripts can override global scripts by using the same filename.

## DiscoveryPaths API

### Creating Discovery Paths

```rust
use crucible_rune::discovery_paths::DiscoveryPaths;

// With kiln path
let paths = DiscoveryPaths::new("hooks", Some(kiln_path));

// Without kiln path (global only)
let paths = DiscoveryPaths::new("tools", None);

// Empty (no defaults)
let paths = DiscoveryPaths::empty("hooks");
```

### Adding Custom Paths

```rust
// Add single path
let paths = DiscoveryPaths::new("hooks", None)
    .with_path("/custom/hooks".into());

// Add multiple paths
let paths = DiscoveryPaths::new("hooks", None)
    .with_additional(vec![
        "/project/hooks".into(),
        "/shared/hooks".into(),
    ]);
```

### Controlling Defaults

```rust
// Disable defaults (explicit paths only)
let paths = DiscoveryPaths::new("hooks", None)
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

// Get subdirectories
let subdirs = paths.subdir("recipe_discovered");  // Vec<PathBuf>
let existing_subdirs = paths.existing_subdir("recipe_discovered");
```

## Configuration

Discovery paths can be configured via TOML:

### Format

```toml
[discovery.hooks]
additional_paths = ["/custom/hooks", "/shared/hooks"]
use_defaults = true

[discovery.tools]
additional_paths = ["/project/tools"]
use_defaults = false  # Disable ~/.crucible/tools/ and KILN/.crucible/tools/

[discovery.events]
# Use defaults only
use_defaults = true
```

### Applying Configuration

```rust
use crucible_rune::discovery_paths::{DiscoveryPaths, DiscoveryConfig};

let config = DiscoveryConfig {
    additional_paths: vec!["/custom/path".into()],
    use_defaults: true,
};

let paths = DiscoveryPaths::new("hooks", Some(kiln_path))
    .with_config(&config);
```

## Hook Discovery

### Discovery Process

1. **Scan directories**: Find all `.rn` files in discovery paths
2. **Parse attributes**: Extract `#[hook(...)]` attributes from each file
3. **Compile scripts**: Compile Rune source to bytecode
4. **Create handlers**: Wrap in `RuneHookHandler` instances
5. **Register**: Add to EventBus

### Example Discovery

```rust
use crucible_rune::hook_system::HookRegistry;

// Create registry with discovery paths
let mut registry = HookRegistry::new(Some(kiln_path))?;

// Discover all hooks
let count = registry.discover()?;
println!("Discovered {} hooks", count);

// Register on event bus
let mut bus = EventBus::new();
registry.register_all(&mut bus);
```

### Hot Reload

File watcher detects changes:

```rust
// Watch for file changes
let watcher = setup_file_watcher(registry.paths());

// On file change:
registry.reload_file(&changed_path)?;

// Re-register on bus
registry.register_all(&mut bus);
```

## Tool Discovery

### Rune Tools

Tools are discovered from `runes/` directories:

```rust
use crucible_rune::discovery_paths::DiscoveryPaths;
use crucible_rune::attribute_discovery::AttributeDiscovery;

let paths = DiscoveryPaths::new("runes", Some(kiln_path));
let discovery = AttributeDiscovery::new();

// Discover all tools
let tools: Vec<RuneTool> = discovery.discover_all(&paths)?;
```

### Tool Metadata

Tools use `#[tool(...)]` attributes:

```rune
/// Search for Rust crates on crates.io
#[tool(
    name = "search_crates",
    description = "Search for Rust crates",
    category = "search"
)]
pub fn search_crates(query) {
    // Implementation
}
```

## Event Handler Discovery

Event handlers are specialized hooks for custom event types:

### Directory Structure

```
~/.crucible/events/
├── recipe_discovered/    # Handlers for recipe:discovered
│   └── enrich.rn
├── audit/                # Handlers for audit:* events
│   └── log.rn
└── custom/               # General custom event handlers
    └── process.rn
```

### Subdirectory Matching

```rust
// Get paths for specific event type
let paths = DiscoveryPaths::new("events", Some(kiln_path));
let recipe_paths = paths.existing_subdir("recipe_discovered");

// Discover handlers in subdirectory
for path in recipe_paths {
    let handlers = discover_in_dir(&path)?;
}
```

## File Organization

### Naming Conventions

**Recommended naming**:
- Descriptive names: `filter_test_output.rn`, `github_rate_limiter.rn`
- Lowercase with underscores: `my_hook.rn` not `MyHook.rn`
- Category prefix optional: `tool_validator.rn`, `note_indexer.rn`

**Avoid**:
- Generic names: `hook1.rn`, `script.rn`
- Special characters in filenames
- Names longer than 50 characters

### Single vs Multiple Hooks

**One file, one hook** (recommended):
```rune
// filter_tests.rn
#[hook(event = "tool:after", pattern = "just_test*")]
pub fn filter_tests(ctx, event) {
    // Implementation
}
```

**One file, multiple hooks** (for related functionality):
```rune
// github_hooks.rn

#[hook(event = "tool:before", pattern = "gh_*", priority = 5)]
pub fn rate_limit_github(ctx, event) {
    // Rate limiting logic
}

#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
pub fn transform_github_results(ctx, event) {
    // Transformation logic
}

#[hook(event = "tool:error", pattern = "gh_*", priority = 100)]
pub fn retry_github_errors(ctx, event) {
    // Retry logic
}
```

### Directory Organization

**Flat structure** (simple projects):
```
~/.crucible/hooks/
├── filter_tests.rn
├── log_tools.rn
└── enrich_recipes.rn
```

**Categorized structure** (complex projects):
```
~/.crucible/hooks/
├── tools/
│   ├── filter_tests.rn
│   ├── rate_limiter.rn
│   └── validator.rn
├── notes/
│   ├── index_backlinks.rn
│   └── extract_tags.rn
└── github/
    ├── rate_limit.rn
    ├── transform.rn
    └── retry.rn
```

**Note**: Discovery is recursive, so subdirectories are automatically scanned.

## Examples

### Example 1: Global Hook

Create `~/.crucible/hooks/log_all_tools.rn`:

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

### Example 2: Kiln-Specific Hook

Create `KILN/.crucible/hooks/project_specific.rn`:

```rune
/// Project-specific test filtering
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_project_tests(ctx, event) {
    // Project-specific filtering logic
    event
}
```

This hook only applies to this kiln.

### Example 3: Override Global Hook

**Global**: `~/.crucible/hooks/test_filter.rn`
```rune
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn test_filter(ctx, event) {
    // Default filtering
    event
}
```

**Kiln override**: `KILN/.crucible/hooks/test_filter.rn`
```rune
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn test_filter(ctx, event) {
    // Custom filtering for this project
    event
}
```

The kiln-specific hook shadows the global one (same name = override).

### Example 4: Custom Path Configuration

`~/.config/crucible/config.toml`:

```toml
[discovery.hooks]
additional_paths = [
    "/work/shared-hooks",
    "/home/user/personal-hooks"
]
use_defaults = true

[discovery.tools]
additional_paths = ["/work/company-tools"]
use_defaults = false  # Only use company tools, not defaults
```

This configuration:
- Adds two custom hook directories
- Keeps default hook paths
- Disables default tool paths (uses only company tools)

### Example 5: Event-Specific Handlers

Create event-specific handlers:

```
~/.crucible/events/
└── recipe_discovered/
    └── enrich_rust_recipes.rn
```

```rune
/// Enrich Rust-related recipes
#[hook(event = "custom", pattern = "recipe:discovered", priority = 10)]
pub fn enrich_rust_recipes(ctx, event) {
    let recipe_name = event.payload.name;

    if recipe_name.contains("cargo") || recipe_name.contains("rust") {
        event.payload.tags.push("rust");
        event.payload.category = "rust-development";
    }

    event
}
```

## Troubleshooting

### Hooks Not Found

**Check discovery paths**:
```bash
# Verify directories exist
ls -la ~/.crucible/hooks/
ls -la KILN/.crucible/hooks/

# Check for .rn files
find ~/.crucible/hooks/ -name "*.rn"
```

**Check logs**:
```bash
# Look for discovery messages
grep "Discovered.*hooks" crucible.log
```

### Hooks Not Executing

**Verify registration**:
- Check hook attribute syntax
- Verify event type matches
- Check pattern matches identifier
- Ensure `enabled = true`

**Check priority**:
- Lower priority hooks run first
- Check if earlier hook cancels event

**Check compilation**:
- Look for Rune compilation errors in logs
- Verify syntax is correct

### Name Conflicts

If multiple hooks have the same name:
- First discovered wins
- Use unique names to avoid conflicts
- Use namespacing: `project_filter_tests` vs `global_filter_tests`

### Permission Issues

Ensure directories are readable:
```bash
chmod -R u+r ~/.crucible/
chmod -R u+r KILN/.crucible/
```

## Best Practices

### Organization

- **Global hooks**: General utilities, logging, audit
- **Kiln hooks**: Project-specific behavior, customizations
- **Subdirectories**: Group related hooks together
- **Clear names**: Descriptive function and file names

### Performance

- **Minimize file count**: Combine related hooks
- **Use specific patterns**: Reduce unnecessary invocations
- **Cache compiled code**: Discovery caches compiled units

### Maintainability

- **Document paths**: Comment why custom paths are needed
- **Version control**: Check kiln hooks into project repo
- **Avoid hardcoding**: Use configuration for paths
- **Test discovery**: Verify hooks load on startup

### Security

- **Restrict permissions**: Make hook directories user-only
- **Review scripts**: Audit hooks before deploying
- **Separate concerns**: Don't mix sensitive and public hooks
- **Use .gitignore**: Don't commit sensitive hooks

## See Also

- [hooks.md](./hooks.md) - Writing Rune hooks
- [event-system.md](./event-system.md) - Event types and handling
- `/examples/rune-hooks/` - Example hook scripts
- `crates/crucible-rune/src/discovery_paths.rs` - Implementation
- `crates/crucible-rune/src/hook_system.rs` - Hook registration
