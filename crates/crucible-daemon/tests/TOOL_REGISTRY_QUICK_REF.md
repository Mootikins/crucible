# Tool Registry Test Quick Reference

## Test Summary

**Location**: `/home/moot/crucible/crates/crucible-daemon/tests/tool_registry.rs`
**Total Tests**: 13
**Current Status**: All compile, all fail with `todo!()`

## Run Commands

```bash
# All tests
cargo test -p crucible-daemon --test tool_registry

# Single test
cargo test -p crucible-daemon --test tool_registry test_discover_tools_in_directory

# With output
cargo test -p crucible-daemon --test tool_registry -- --nocapture

# Watch mode (requires cargo-watch)
cargo watch -x "test -p crucible-daemon --test tool_registry"
```

## Test Checklist

### Discovery (3 tests)
- [ ] `test_discover_tools_in_directory` - Find all .rn files
- [ ] `test_ignore_non_rune_files` - Filter non-.rn files
- [ ] `test_hot_reload_on_file_change` - Auto-reload new tools

### Loading (3 tests)
- [ ] `test_load_valid_rune_script` - Compile with rune::prepare()
- [ ] `test_handle_invalid_rune_syntax` - Syntax error handling
- [ ] `test_tool_with_database_access` - DB injection

### Execution (3 tests)
- [ ] `test_execute_simple_tool` - Basic run, capture output
- [ ] `test_execute_tool_with_arguments` - Pass CLI args
- [ ] `test_execute_tool_timeout` - Prevent infinite loops

### Error Handling (4 tests)
- [ ] `test_tool_runtime_error` - Capture panics/exceptions
- [ ] `test_tool_returns_structured_data` - JSON serialization
- [ ] `test_list_tools_with_metadata` - Extract doc comments
- [ ] `test_execute_nonexistent_tool` - Missing tool error

## Implementation Skeleton

```rust
// In /home/moot/crucible/crates/crucible-daemon/src/tools/mod.rs

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use rune::{Unit, Vm, Sources, prepare, Diagnostics};

pub struct ToolRegistry {
    tool_dir: PathBuf,
    loaded_tools: HashMap<String, Arc<Unit>>,
    timeout: Duration,
}

pub struct ToolResult {
    pub output: String,
    pub duration: Duration,
    pub success: bool,
    pub error: Option<String>,
}

pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub is_async: bool,
    pub params: Vec<String>,
}

impl ToolRegistry {
    pub fn new(tool_dir: PathBuf) -> Result<Self> {
        todo!("Initialize registry")
    }

    pub async fn discover_tools(&mut self) -> Result<Vec<String>> {
        todo!("Scan directory for .rn files")
    }

    pub async fn load_tool(&mut self, name: &str) -> Result<()> {
        todo!("Compile Rune script")
    }

    pub async fn execute_tool(&self, name: &str, args: &[String]) -> Result<ToolResult> {
        todo!("Run tool with args")
    }

    pub fn list_tools(&self) -> Vec<String> {
        todo!("Return tool names")
    }

    pub fn list_tools_with_info(&self) -> Vec<ToolInfo> {
        todo!("Extract metadata")
    }

    pub async fn reload(&mut self) -> Result<()> {
        todo!("Re-discover and load tools")
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;
        Ok(())
    }
}
```

## Key Rune APIs

```rust
use rune::{Sources, prepare, Diagnostics, Unit, Vm};

// Compilation
let mut sources = Sources::new();
sources.insert(Source::new("tool_name", source_code)?)?;

let mut diagnostics = Diagnostics::new();
let unit = prepare(&mut sources)
    .with_diagnostics(&mut diagnostics)
    .build()?;

// Execution
let mut vm = Vm::new(Arc::new(context), Arc::new(unit));
let output = vm.call(["main"], ())?;

// With args
let output = vm.call(["main"], (arg1, arg2))?;
```

## File Structure

```
crucible-daemon/
├── src/
│   ├── tools/
│   │   └── mod.rs           ← Implement ToolRegistry here
│   ├── rune/
│   │   └── mod.rs           ← Rune runtime helpers (optional)
│   └── repl/
│       └── command.rs       ← Already has :tools, :run commands
├── tests/
│   ├── tool_registry.rs     ← Your test file (done!)
│   └── TOOL_REGISTRY_TEST_GUIDE.md  ← Implementation guide
└── Cargo.toml
```

## Example Test Run Output

```
running 13 tests
test test_discover_tools_in_directory ... FAILED
test test_ignore_non_rune_files ... FAILED
test test_hot_reload_on_file_change ... FAILED
test test_load_valid_rune_script ... FAILED
test test_handle_invalid_rune_syntax ... FAILED
test test_tool_with_database_access ... FAILED
test test_execute_simple_tool ... FAILED
test test_execute_tool_with_arguments ... FAILED
test test_execute_tool_timeout ... FAILED
test test_tool_runtime_error ... FAILED
test test_tool_returns_structured_data ... FAILED
test test_list_tools_with_metadata ... FAILED
test test_execute_nonexistent_tool ... FAILED

failures:

---- test_discover_tools_in_directory stdout ----
thread 'test_discover_tools_in_directory' panicked at crates/crucible-daemon/tests/tool_registry.rs:114:5:
not yet implemented: Implement ToolRegistry::discover_tools() - should find all .rn files in directory
```

## Implementation Progress Tracking

Use this checklist to track implementation:

**Phase 1: Basic Structure**
- [ ] Create `ToolRegistry` struct
- [ ] Create `ToolResult` struct
- [ ] Create `ToolInfo` struct
- [ ] Implement `new()`

**Phase 2: Discovery**
- [ ] Implement `discover_tools()` - scan directory
- [ ] Add file extension filtering
- [ ] Sort tool names

**Phase 3: Loading**
- [ ] Implement `load_tool()` - compile with rune
- [ ] Handle compilation errors with diagnostics
- [ ] Store compiled units in HashMap

**Phase 4: Execution**
- [ ] Implement `execute_tool()` - basic execution
- [ ] Add argument passing
- [ ] Add timeout wrapper
- [ ] Capture runtime errors

**Phase 5: Metadata**
- [ ] Extract doc comments
- [ ] Detect async functions
- [ ] Parse parameter names

**Phase 6: Advanced**
- [ ] Database injection
- [ ] JSON serialization
- [ ] Hot-reload with file watching

## Common Patterns

### Async Test
```rust
#[tokio::test]
async fn test_something() -> Result<()> {
    // Setup
    let test_reg = TestToolRegistry::new().await?;
    test_reg.create_tool("name", "source").await?;

    // Test
    // ...

    Ok(())
}
```

### Error Assertion
```rust
let result = registry.execute_tool("broken", &[]).await;
assert!(result.is_err());

let err_msg = result.unwrap_err().to_string();
assert!(err_msg.contains("expected text"));
```

### Timeout Pattern
```rust
use tokio::time::{timeout, Duration};

let result = timeout(Duration::from_millis(100), async {
    registry.execute_tool("infinite", &[]).await
}).await;

assert!(result.is_err()); // Timeout occurred
```

## Tips

1. **Start simple**: Get discovery working first (tests 1-2)
2. **Use diagnostics**: Rune provides great error messages
3. **Test incrementally**: Fix one test at a time
4. **Check examples**: Rune docs have good VM examples
5. **Error messages**: Make them helpful (filename, line, suggestion)
6. **Performance**: Cache compiled units, don't recompile every run

## Resources

- **Rune Docs**: https://rune-rs.github.io/
- **Rune Book**: https://rune-rs.github.io/book/
- **Rune API**: https://docs.rs/rune/latest/rune/

## Need Help?

Check the detailed guide: `/home/moot/crucible/crates/crucible-daemon/tests/TOOL_REGISTRY_TEST_GUIDE.md`
