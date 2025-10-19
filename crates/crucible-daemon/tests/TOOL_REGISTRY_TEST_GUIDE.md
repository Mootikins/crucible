# Tool Registry Test Implementation Guide

## Overview

This guide documents the 13 comprehensive tests for the Rune tool registry and script execution system. These tests follow TDD principles - they define expected behavior before implementation.

**Test File**: `/home/moot/crucible/crates/crucible-daemon/tests/tool_registry.rs`

**Run Tests**: `cargo test -p crucible-daemon --test tool_registry`

**Current Status**: All tests compile and fail with descriptive `todo!()` messages

---

## Test Organization

### Tool Discovery Tests (3 tests)

#### 1. `test_discover_tools_in_directory`
**Purpose**: Verify registry can scan directory and find all .rn files

**Setup**:
- Creates 3 Rune scripts: `hello.rn`, `count_notes.rn`, `search_tag.rn`
- Each has different complexity (simple, async, parameterized)

**Expected Behavior**:
- `ToolRegistry::discover_tools()` scans tool directory
- Returns `Vec<String>` of tool names (without .rn extension)
- Names sorted alphabetically
- All 3 tools discovered

**Implementation Hints**:
```rust
impl ToolRegistry {
    async fn discover_tools(&mut self) -> Result<Vec<String>> {
        // 1. Read directory entries
        // 2. Filter for .rn extension
        // 3. Extract filenames without extension
        // 4. Sort and return
    }
}
```

---

#### 2. `test_ignore_non_rune_files`
**Purpose**: Ensure only .rn files loaded, not README.md, .txt, etc.

**Setup**:
- 2 valid .rn files
- 4 non-.rn files (README.md, notes.txt, config.yaml, data.json)

**Expected Behavior**:
- Only .rn files discovered
- Other extensions completely ignored
- No errors from non-.rn files

**Implementation Hints**:
```rust
// In discover_tools():
if let Some(ext) = path.extension() {
    if ext != "rn" {
        continue; // Skip non-Rune files
    }
}
```

---

#### 3. `test_hot_reload_on_file_change`
**Purpose**: File watcher detects new tools and auto-reloads

**Setup**:
- Start with 2 tools
- Add 3rd tool after registry initialized

**Expected Behavior**:
- Initial discovery finds 2 tools
- File watcher detects new .rn file
- `registry.reload()` makes new tool available
- No manual refresh needed

**Implementation Hints**:
```rust
// Use notify crate or watch system
impl ToolRegistry {
    async fn watch_directory(&mut self) -> Result<()> {
        // Set up file watcher on tool_dir
        // On .rn file created/modified: reload()
    }

    async fn reload(&mut self) -> Result<()> {
        self.discover_tools().await?;
        // Re-compile changed tools
    }
}
```

---

### Tool Loading Tests (3 tests)

#### 4. `test_load_valid_rune_script`
**Purpose**: Load and compile valid Rune script, extract metadata

**Setup**:
- Script with doc comments (`//!`)
- Async function with database parameter

**Expected Behavior**:
- Read .rn file from disk
- Parse with `rune::prepare()`
- Extract metadata (name, description, params)
- Store compiled tool in registry

**Implementation Hints**:
```rust
use rune::{Sources, prepare, Diagnostics};

async fn load_tool(&mut self, name: &str) -> Result<()> {
    let path = self.tool_dir.join(format!("{}.rn", name));
    let source = tokio::fs::read_to_string(&path).await?;

    let mut sources = Sources::new();
    sources.insert(rune::Source::new(name, source)?)?;

    let mut diagnostics = Diagnostics::new();
    let unit = prepare(&mut sources)
        .with_diagnostics(&mut diagnostics)
        .build()?;

    // Store compiled unit
    self.loaded_tools.insert(name.to_string(), unit);
    Ok(())
}
```

---

#### 5. `test_handle_invalid_rune_syntax`
**Purpose**: Graceful error handling for syntax errors

**Setup**:
- 1 valid tool
- 1 tool with syntax error (unclosed string)

**Expected Behavior**:
- Loading broken tool returns `Err`
- Error includes filename and line number
- Valid tool still loads successfully
- Registry remains stable

**Implementation Hints**:
```rust
// Use rune::Diagnostics to capture errors
if !diagnostics.is_empty() {
    let errors = diagnostics.errors()
        .map(|e| format!("{}:{} - {}", name, e.span, e.message))
        .collect::<Vec<_>>();
    return Err(anyhow::anyhow!(
        "Compilation failed in {}.rn:\n{}",
        name,
        errors.join("\n")
    ));
}
```

---

#### 6. `test_tool_with_database_access`
**Purpose**: Verify db parameter injection works

**Setup**:
- Tool that executes SurrealDB query
- Mock or test database connection

**Expected Behavior**:
- Tool execution context includes database
- `db` parameter bound to SurrealDB client
- Queries execute and return results

**Implementation Hints**:
```rust
// In execute_tool:
let mut vm = Vm::new(Arc::new(context), Arc::new(unit));

// Inject database connection
vm.call(["main"], (db_connection,))?;
```

---

### Tool Execution Tests (3 tests)

#### 7. `test_execute_simple_tool`
**Purpose**: Basic execution - run tool, get result

**Setup**:
- Simple tool that returns static string

**Expected Behavior**:
- Create Rune VM
- Execute `main()` function
- Capture return value
- Format output
- Track execution duration

**Implementation Hints**:
```rust
struct ToolResult {
    output: String,
    duration: Duration,
    success: bool,
    error: Option<String>,
}

async fn execute_tool(&self, name: &str, args: &[String])
    -> Result<ToolResult>
{
    let start = Instant::now();

    let unit = self.loaded_tools.get(name)
        .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

    let mut vm = Vm::new(Arc::new(context), Arc::new(unit.clone()));
    let output = vm.call(["main"], ())?;

    Ok(ToolResult {
        output: format!("{:?}", output),
        duration: start.elapsed(),
        success: true,
        error: None,
    })
}
```

---

#### 8. `test_execute_tool_with_arguments`
**Purpose**: Pass CLI args to tool parameters

**Setup**:
- Tool with parameters: `main(name, greeting)`
- Execute with args: `["Alice", "Hello"]`

**Expected Behavior**:
- Parse args from command
- Convert to Rune values
- Pass to `main()` as parameters
- Output includes arg values

**Implementation Hints**:
```rust
// Convert string args to Rune values
let rune_args: Vec<Value> = args.iter()
    .map(|s| Value::from(s.clone()))
    .collect();

// Call with args
let output = vm.call(["main"], (rune_args[0], rune_args[1]))?;
```

---

#### 9. `test_execute_tool_timeout`
**Purpose**: Prevent infinite loops from hanging REPL

**Setup**:
- Tool with infinite loop
- Short timeout (100ms)

**Expected Behavior**:
- Execution wrapped in `tokio::time::timeout()`
- Timeout triggers after duration
- Error returned, VM killed
- Clear timeout error message

**Implementation Hints**:
```rust
use tokio::time::{timeout, Duration};

let timeout_duration = Duration::from_secs(30);
let result = timeout(timeout_duration, async {
    // Execute tool
    vm.call(["main"], ())
}).await;

match result {
    Ok(Ok(output)) => Ok(output),
    Ok(Err(e)) => Err(e), // Runtime error
    Err(_) => Err(anyhow::anyhow!("Tool timed out after {:?}", timeout_duration)),
}
```

---

### Error Handling Tests (4 tests)

#### 10. `test_tool_runtime_error`
**Purpose**: Capture runtime errors (not compilation errors)

**Setup**:
- Tool that compiles but panics at runtime (index out of bounds)

**Expected Behavior**:
- Tool compiles successfully
- Execution throws runtime error
- Error captured in `ToolResult`
- Stack trace available

**Implementation Hints**:
```rust
let result = vm.call(["main"], ());

match result {
    Ok(output) => ToolResult {
        success: true,
        output: format!("{:?}", output),
        error: None,
        ..
    },
    Err(e) => ToolResult {
        success: false,
        output: String::new(),
        error: Some(format!("{:#}", e)), // Pretty error with backtrace
        ..
    }
}
```

---

#### 11. `test_tool_returns_structured_data`
**Purpose**: Tools can return JSON objects/arrays

**Setup**:
- Tool returns Rune object with nested structure

**Expected Behavior**:
- Rune values convert to JSON
- Arrays, maps, primitives supported
- Valid JSON output
- Can be parsed by serde_json

**Implementation Hints**:
```rust
// Convert Rune value to JSON
fn rune_value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::String(s) => json!(s),
        Value::Integer(i) => json!(i),
        Value::Object(obj) => {
            // Recursively convert object fields
        },
        Value::Vec(vec) => {
            // Convert array elements
        },
        // ... other types
    }
}
```

---

#### 12. `test_list_tools_with_metadata`
**Purpose**: Extract metadata for `:tools` command

**Setup**:
- 3 tools with doc comments
- Mix of sync/async, different parameters

**Expected Behavior**:
- Return `Vec<ToolInfo>` with metadata
- Extract doc comments for descriptions
- Detect async vs sync
- Show parameter names

**Implementation Hints**:
```rust
struct ToolInfo {
    name: String,
    description: String,
    is_async: bool,
    params: Vec<String>,
}

fn list_tools_with_info(&self) -> Vec<ToolInfo> {
    // Parse source files for metadata
    // Or store during compilation
}
```

---

#### 13. `test_execute_nonexistent_tool`
**Purpose**: Clear error for missing tools

**Setup**:
- One valid tool exists
- Attempt to run non-existent tool

**Expected Behavior**:
- Returns `Err`
- Error suggests using `:tools`
- Optional: suggest similar tool names (typo detection)

**Implementation Hints**:
```rust
let tool = self.loaded_tools.get(name)
    .ok_or_else(|| {
        let available = self.list_tools().join(", ");
        anyhow::anyhow!(
            "Tool '{}' not found. Available tools: {}\nUse :tools to see all tools.",
            name,
            available
        )
    })?;
```

---

## Implementation Order

Recommended order to implement features:

1. **Basic Discovery** (Tests 1-2)
   - Scan directory for .rn files
   - Filter by extension
   - Return sorted names

2. **Basic Loading** (Test 4-5)
   - Compile Rune scripts with `rune::prepare()`
   - Handle syntax errors gracefully
   - Store compiled units

3. **Basic Execution** (Test 7)
   - Create Rune VM
   - Execute `main()` function
   - Capture output

4. **Error Handling** (Tests 10, 13)
   - Runtime error capture
   - Missing tool errors
   - Clear error messages

5. **Advanced Execution** (Tests 8-9)
   - Argument passing
   - Timeout handling

6. **Structured Data** (Test 11)
   - Rune value to JSON conversion

7. **Metadata** (Test 12)
   - Doc comment extraction
   - Parameter detection

8. **Hot Reload** (Test 3)
   - File watching
   - Auto-reload on changes

9. **Database Integration** (Test 6)
   - Database connection injection
   - Query execution from tools

---

## API Surface to Implement

```rust
// Core types
pub struct ToolRegistry {
    tool_dir: PathBuf,
    loaded_tools: HashMap<String, Arc<Unit>>,
    // file watcher, etc.
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

// Core methods
impl ToolRegistry {
    pub fn new(tool_dir: PathBuf) -> Result<Self>;
    pub async fn discover_tools(&mut self) -> Result<Vec<String>>;
    pub async fn load_tool(&mut self, name: &str) -> Result<()>;
    pub async fn execute_tool(&self, name: &str, args: &[String]) -> Result<ToolResult>;
    pub fn list_tools(&self) -> Vec<String>;
    pub fn list_tools_with_info(&self) -> Vec<ToolInfo>;
    pub async fn reload(&mut self) -> Result<()>;
    pub fn set_database(&mut self, db: DatabaseHandle) -> Result<()>;
    pub fn set_timeout(&mut self, timeout: Duration) -> Result<()>;
}
```

---

## Dependencies Required

Already in `Cargo.toml`:
- `rune = "0.13"` - Script runtime
- `tokio` - Async runtime
- `anyhow` - Error handling
- `serde_json` - JSON serialization

May need to add:
- `notify` or `notify-debouncer-full` - File watching (if not using crucible-watch)

---

## Testing Workflow

1. **Run all tests**: `cargo test -p crucible-daemon --test tool_registry`
2. **Run single test**: `cargo test -p crucible-daemon --test tool_registry test_discover_tools_in_directory`
3. **Watch mode**: `cargo watch -x "test -p crucible-daemon --test tool_registry"`

---

## Example Rune Scripts (For Testing)

### Simple Tool
```rust
pub fn main() {
    "Hello from Rune!"
}
```

### Database Query Tool
```rust
pub async fn main(db) {
    let result = db.query("SELECT count() FROM notes").await?;
    result
}
```

### Parameterized Tool
```rust
pub async fn main(db, tag) {
    let query = format!("SELECT * FROM notes WHERE tags CONTAINS '{}'", tag);
    db.query(query).await?
}
```

### Documented Tool
```rust
//! Search notes by tag or title
//!
//! This tool performs a fuzzy search across note titles and tags.

pub async fn main(db, query) {
    // Implementation
}
```

---

## Success Criteria

All 13 tests pass:
- 3 discovery tests (file scanning, filtering, hot-reload)
- 3 loading tests (compilation, errors, database)
- 3 execution tests (basic, args, timeout)
- 4 error handling tests (runtime errors, missing tools, structured data, metadata)

When complete, the tool registry will support:
- Automatic discovery of .rn scripts
- Compilation and caching
- Execution with arguments
- Database access from tools
- Hot-reload on file changes
- Comprehensive error handling
- Structured data output
- Metadata extraction for help

---

## Next Steps

1. Implement `ToolRegistry` struct in `/home/moot/crucible/crates/crucible-daemon/src/tools/mod.rs`
2. Create `ToolResult` and `ToolInfo` types
3. Start with discovery tests (easiest)
4. Build up to execution and error handling
5. Integration with REPL commands (`:tools`, `:run`)

Good luck! The tests provide complete specifications for expected behavior.
