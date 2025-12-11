# Worktree: feat/mcp-bridge-completion

**Goal**: Complete the MCP bridge event system (last 15%)
**Status**: ~85% complete

## Current State

Already implemented:
- [x] Unified discovery paths (`DiscoveryPaths`)
- [x] Attribute discovery (`#[hook]`, `#[tool]`)
- [x] Core event system (EventBus, patterns, priorities)
- [x] Hook system (Rune and built-in handlers)
- [x] Tool events (before/after/error)
- [x] Note events (parsed/created/modified)
- [x] MCP Gateway client (stdio + SSE transports)
- [x] Tool selector hook (whitelist/blacklist/prefix)
- [x] Configuration schema

## Key Files

```
crates/crucible-rune/src/
├── event_bus.rs        # Core EventBus implementation
├── tool_events.rs      # ToolEventEmitter, ToolSource
├── note_events.rs      # NoteEventEmitter, NotePayload
├── hooks/
│   ├── mod.rs          # Hook trait and registry
│   ├── rune_handler.rs # Rune script hook wrapper
│   ├── builtin.rs      # Built-in hooks (filter, transform)
│   └── tool_selector.rs # Whitelist/blacklist hook
├── discovery/
│   ├── paths.rs        # DiscoveryPaths struct
│   └── attributes.rs   # FromAttributes trait
└── mcp_gateway.rs      # Upstream MCP client

crates/crucible-tools/src/extended_mcp_server.rs  # Integration point
crates/crucible-config/src/                        # Config structs
```

## Remaining Tasks

### 1. TOML Config Schema (1.4)
Add `[discovery.<type>]` section support:

```toml
[discovery.hooks]
additional_paths = ["~/.config/crucible/hooks"]
use_defaults = true

[discovery.tools]
additional_paths = ["/opt/crucible/tools"]
use_defaults = true
```

Files to modify:
- `crates/crucible-config/src/` - Add discovery config types
- `crates/crucible-rune/src/discovery/paths.rs` - Read from config

### 2. SurrealDB Attribute Caching (2.7) - DEFERRED
Cache discovered `#[tool]` and `#[hook]` attributes in SurrealDB for fast reload.

Currently re-scans files on startup. Caching would speed up cold starts.

Status: Deferred - works without it, optimize later.

### 3. Hook Hot-Reload (4.6) - DEFERRED
Watch hook script files and reload on change.

Requires file watcher integration (`crucible-watch`).

Status: Deferred - restart works for now.

### 4. Integration Test with Mock MCP (8.8)
Test the gateway client with a mock MCP server:

```rust
#[tokio::test]
async fn test_mcp_gateway_tool_discovery() {
    // Start mock MCP server
    let mock = MockMcpServer::new()
        .with_tool("test_tool", json!({"type": "object"}))
        .start();

    // Connect gateway
    let gateway = UpstreamMcpClient::connect(mock.address()).await?;

    // Verify tool discovery
    let tools = gateway.list_tools().await?;
    assert!(tools.iter().any(|t| t.name == "test_tool"));
}
```

Location: `crates/crucible-rune/tests/` or `crates/crucible-tools/tests/`

## Implementation Notes

### Discovery Config Types
```rust
// crates/crucible-config/src/discovery.rs
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryConfig {
    pub hooks: Option<DiscoveryTypeConfig>,
    pub tools: Option<DiscoveryTypeConfig>,
    pub events: Option<DiscoveryTypeConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryTypeConfig {
    pub additional_paths: Vec<String>,
    #[serde(default = "default_true")]
    pub use_defaults: bool,
}
```

### Integrate with DiscoveryPaths
```rust
impl DiscoveryPaths {
    pub fn from_config(type_name: &str, kiln_path: &Path, config: &DiscoveryConfig) -> Self {
        let type_config = match type_name {
            "hooks" => config.hooks.as_ref(),
            "tools" => config.tools.as_ref(),
            _ => None,
        };

        let mut paths = if type_config.map(|c| c.use_defaults).unwrap_or(true) {
            Self::new(type_name, kiln_path)
        } else {
            Self::empty(type_name)
        };

        if let Some(tc) = type_config {
            for p in &tc.additional_paths {
                paths.add_path(shellexpand::tilde(p).into_owned());
            }
        }

        paths
    }
}
```

## Testing

```bash
# Unit tests
cargo test -p crucible-rune

# Integration tests
cargo test -p crucible-tools extended_mcp

# Manual: start MCP server and connect
cargo run -p crucible-cli -- mcp serve
```

## Reference

- Task spec: `openspec/changes/add-mcp-bridge/tasks.md`
- rmcp docs: https://docs.rs/rmcp
- Event system design: `openspec/changes/add-mcp-bridge/proposal.md`

## Success Criteria

- [ ] `[discovery.hooks]` config section works
- [ ] `[discovery.tools]` config section works
- [ ] Integration test passes with mock MCP server
- [ ] Config validation at startup catches invalid paths
