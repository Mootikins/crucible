# Migration Strategy: Eliminating Global Static State

This document provides a comprehensive strategy for migrating from global singleton patterns to safer Rust ownership patterns.

## Overview

We need to eliminate two unsafe singleton patterns:
1. `GLOBAL_TOOL_REGISTRY` in `crates/crucible-tools/src/types.rs`
2. `CrucibleToolManager` INSTANCE in `crates/crucible-cli/src/common/tool_manager.rs`

## Phase 1: Preparation (Week 1)

### 1.1 Create New Implementations
- ✅ Create `types_refactored.rs` with dependency injection approach
- ✅ Create `app_state.rs` with application state pattern
- ✅ Create `service_locator.rs` with service locator pattern

### 1.2 Add Compatibility Layer
Create adapters to allow old and new code to coexist:

```rust
// In crates/crucible-tools/src/lib.rs
pub mod compatibility {
    //! Compatibility layer for gradual migration

    use super::types_refactored::*;
    use std::sync::Arc;
    use tokio::sync::OnceCell;

    // Global instance for compatibility during migration
    static GLOBAL_REGISTRY_COMPAT: OnceCell<Arc<ToolRegistry>> = OnceCell::const_new();

    pub async fn get_global_registry_compat() -> Arc<ToolRegistry> {
        GLOBAL_REGISTRY_COMPAT.get_or_init(|| async {
            let registry = Arc::new(ToolRegistry::new());
            registry.load_all_tools().await.unwrap();
            registry
        }).await.clone()
    }

    // Bridge functions for old API
    pub async fn execute_tool_legacy(
        tool_name: String,
        parameters: serde_json::Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<super::ToolResult, super::ToolError> {
        let registry = get_global_registry_compat().await;
        registry.execute_tool(tool_name, parameters, user_id, session_id).await
    }
}
```

### 1.3 Update Build Configuration
Add feature flags to control migration:

```toml
# In Cargo.toml
[features]
default = ["legacy_singletons"]
legacy_singletons = []
dependency_injection = []
service_locator = []
app_state = []
migration_mode = ["legacy_singletons", "dependency_injection"]
```

## Phase 2: Gradual Migration (Weeks 2-3)

### 2.1 Migrate Tool Registry First

**Priority: High** - The tool registry is the foundation

#### Step 1: Update Tool Registration
```rust
// Old code (remove this)
register_tool_function(name, function).await;

// New code (replace with)
let registry = get_tool_registry().await; // Get from context
registry.register_tool(name, function).await?;
```

#### Step 2: Update Tool Execution
```rust
// Old code (remove this)
let result = execute_tool(tool_name, params, user_id, session_id).await?;

// New code (replace with)
let result = tool_manager.execute_tool(&tool_name, params, user_id, session_id).await?;
```

#### Step 3: Update Tool Loading
```rust
// Old code (remove this)
load_all_tools().await?;

// New code (replace with)
let registry = ToolRegistry::new();
registry.load_all_tools().await?;
```

### 2.2 Migrate CLI Tool Manager

#### Step 1: Replace Singleton Access
```rust
// Old code (remove this)
let result = CrucibleToolManager::execute_tool_global(name, params, user_id, session_id).await?;

// New code (inject ToolManager)
pub struct NoteCommandHandler {
    tool_manager: Arc<ToolManager>,
}

impl NoteCommandHandler {
    pub async fn execute_tool(&self, name: &str, params: serde_json::Value) -> Result<ToolResult> {
        self.tool_manager.execute_tool(name, params, None, None).await
    }
}
```

#### Step 2: Update Command Handlers
Update command handlers to accept dependencies:

```rust
// Old pattern
pub async fn execute(config: CliConfig, cmd: NoteCommands) -> Result<()> {
    // Uses global CrucibleToolManager::instance()
}

// New pattern
pub async fn execute(
    app_state: Arc<AppState>,
    cmd: NoteCommands
) -> Result<()> {
    // Uses injected app_state.tool_manager
}
```

### 2.3 Update Integration Points

#### Update CLI Entry Points
```rust
// In src/main.rs
#[tokio::main]
async fn main() -> Result<()> {
    // Build application state
    let app_state = AppState::from_cli_config(cli_config).await?;

    // Pass state to commands
    match cli.command {
        Commands::Note(cmd) => commands::note::execute(app_state.clone(), cmd).await?,
        Commands::Search(cmd) => commands::search::execute(app_state.clone(), cmd).await?,
        // ... other commands
    }

    Ok(())
}
```

#### Update Tauri Commands
```rust
// In Tauri backend
#[tauri::command]
async fn execute_tool(
    app_state: tauri::State<'_, Arc<AppState>>,
    name: String,
    params: serde_json::Value,
) -> Result<ToolResult, String> {
    app_state
        .execute_tool(&name, params)
        .await
        .map_err(|e| e.to_string())
}
```

## Phase 3: Testing & Validation (Week 4)

### 3.1 Comprehensive Testing

#### Unit Tests
```rust
#[cfg(test)]
mod migration_tests {
    use super::*;

    #[tokio::test]
    async fn test_new_tool_registry() {
        let registry = ToolRegistry::new();

        // Test registration
        registry.register_tool("test".to_string(), test_tool).await.unwrap();
        assert!(registry.has_tool("test").await);

        // Test execution
        let result = registry.execute_tool(
            "test".to_string(),
            json!({}),
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_app_state_independence() {
        let state1 = AppState::for_test().await.unwrap();
        let state2 = AppState::for_test().await.unwrap();

        // Verify independence
        let tools1 = state1.list_tools().await.unwrap();
        let tools2 = state2.list_tools().await.unwrap();

        assert_eq!(tools1.len(), tools2.len());

        // Execute tools independently
        let result1 = state1.execute_tool("system_info", json!({})).await.unwrap();
        let result2 = state2.execute_tool("system_info", json!({})).await.unwrap();

        assert!(result1.success && result2.success);
    }
}
```

#### Integration Tests
```rust
#[tokio::test]
async fn test_cli_command_migration() {
    let app_state = AppState::for_test().await.unwrap();

    // Test note command with injected state
    let cmd = NoteCommands::List { format: "json".to_string() };
    commands::note::execute(app_state.clone(), cmd).await.unwrap();

    // Verify no global state was used
    // (This would require additional test infrastructure)
}
```

### 3.2 Performance Validation

```rust
#[tokio::test]
async fn benchmark_new_vs_old() {
    let iterations = 1000;

    // Old global approach
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let _ = crucible_tools::execute_tool(
            "system_info".to_string(),
            json!({}),
            None,
            None,
        ).await;
    }
    let old_duration = start.elapsed();

    // New dependency injection approach
    let registry = ToolRegistry::new();
    registry.load_all_tools().await.unwrap();

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let _ = registry.execute_tool(
            "system_info".to_string(),
            json!({}),
            None,
            None,
        ).await;
    }
    let new_duration = start.elapsed();

    println!("Old: {:?}, New: {:?}", old_duration, new_duration);
    // New should be equal or better performance
}
```

## Phase 4: Cleanup (Week 5)

### 4.1 Remove Legacy Code
Once migration is complete and tested:

```bash
# Remove old singleton implementations
rm crates/crucible-tools/src/types.rs  # After moving types
rm crates/crucible-cli/src/common/tool_manager.rs  # Replace with new version

# Remove compatibility layer
rm crates/crucible-tools/src/compatibility.rs
```

### 4.2 Update Documentation
- Update API documentation
- Update examples and tutorials
- Update README with new patterns

### 4.3 Remove Feature Flags
```toml
# Clean up Cargo.toml
[features]
default = []  # No legacy features
```

## Step-by-Step Migration Checklist

### For Each Singleton Pattern:

1. **Identify Dependencies**
   - [ ] List all files using the singleton
   - [ ] Note the specific methods being called
   - [ ] Identify initialization requirements

2. **Create Replacement**
   - [ ] Implement dependency injection version
   - [ ] Add comprehensive tests
   - [ ] Document the new API

3. **Update Call Sites**
   - [ ] Replace global access with parameter injection
   - [ ] Update function signatures as needed
   - [ ] Handle error propagation

4. **Validate Changes**
   - [ ] Run existing test suite
   - [ ] Add new integration tests
   - [ ] Performance benchmarking

5. **Clean Up**
   - [ ] Remove old singleton code
   - [ ] Remove any compatibility shims
   - [ ] Update documentation

## Migration Timeline

| Week | Tasks | Deliverables |
|------|-------|--------------|
| 1 | Preparation | New implementations, compatibility layer |
| 2-3 | Migration | Updated call sites, working new code |
| 4 | Testing | Comprehensive test coverage, validation |
| 5 | Cleanup | Removed legacy code, updated docs |

## Risk Mitigation

### High Risks
1. **Breaking Changes**: Mitigate with compatibility layer
2. **Performance Regression**: Mitigate with benchmarking
3. **Complex Migration**: Mitigate with incremental approach

### Rollback Plan
- Keep legacy code in separate branch
- Feature flags allow quick rollback
- Comprehensive test suite ensures safety

## Success Criteria

1. **No Global State**: All singletons eliminated
2. **Test Coverage**: 95%+ coverage for new code
3. **Performance**: No regression in tool execution
4. **Maintainability**: Clear dependency chains, no hidden state
5. **Documentation**: Updated examples and API docs

## Benefits of Migration

1. **Thread Safety**: No more `unsafe` static mut patterns
2. **Testability**: Easy to inject test doubles and mocks
3. **Flexibility**: Multiple independent instances possible
4. **Debugging**: Clear data flow and ownership
5. **Rust Idioms**: Follows Rust best practices

## Files to Create/Modify

### New Files
- `crates/crucible-tools/src/types_refactored.rs`
- `crates/crucible-cli/src/common/app_state.rs`
- `crates/crucible-cli/src/common/service_locator.rs`
- `crates/crucible-tools/src/compatibility.rs` (temporary)

### Files to Modify
- `crates/crucible-tools/src/lib.rs`
- `crates/crucible-cli/src/commands/*.rs`
- `crates/crucible-cli/src/main.rs`
- `crates/crucible-tauri/src/main.rs`
- All integration test files

### Files to Remove (after migration)
- `crates/crucible-tools/src/types.rs` (replaced by types_refactored.rs)
- `crates/crucible-cli/src/common/tool_manager.rs` (replaced by app_state.rs)
- `crates/crucible-tools/src/compatibility.rs` (temporary)

This migration strategy provides a safe, incremental path to eliminate global state while maintaining functionality and improving code quality.