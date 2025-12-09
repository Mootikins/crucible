# Rune Plugin System: MCP Tool Output Filtering

## Overview

A plugin system for Crucible using Rune scripts that can intercept, filter, and transform MCP tool outputs. The short-term goal is filtering test output from Just MCP tools; the long-term goal is a full plugin architecture supporting tools, hooks, and kiln mutations.

## Architecture

### Event Model

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  MCP Tool Call  │────▶│  Event Pipeline  │────▶│  MCP Response   │
│  (just_test)    │     │  (Rune handlers) │     │  (filtered)     │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌──────────────────┐
                        │  Side Effects    │
                        │  (kiln, emit)    │
                        └──────────────────┘
```

### Core Event Type

```rust
// Unified event for all MCP tool executions
pub struct ToolResultEvent {
    pub tool_name: String,           // e.g., "just_test"
    pub arguments: serde_json::Value,
    pub result: ToolResult,          // success/error + content
    pub duration_ms: u64,
    pub metadata: HashMap<String, Value>,
}

pub struct ToolResult {
    pub is_error: bool,
    pub content: Vec<ContentBlock>,  // TextContent, ImageContent, etc.
}
```

### Handler Semantics

- **`None`** = pass through unchanged (event continues to client)
- **`Some(modified_event)`** = use transformed value
- Handlers can also emit new events or mutate kiln via context API

### Plugin Registration Model

Scripts define an `init()` function that returns registration data:

```rune
// runes/plugins/test_filter.rn
pub fn init() {
    #{
        hooks: [
            #{
                event: "tool_result",
                pattern: "just_test*",  // glob pattern
                handler: "filter_test_output",
            }
        ],
        tools: [
            // Can also register new MCP tools
        ]
    }
}

pub async fn filter_test_output(ctx, event) {
    // Filter logic here
    // Return None to pass through, Some(event) to modify
}
```

---

## Phase 1: Short-Term (Test Output Filter)

**Goal**: Working example that filters `just_test` output to show only summary lines.

### Task 1.1: Define Event Types

**File**: `crates/crucible-rune/src/events.rs` (new)

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP tool execution result event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultEvent {
    pub tool_name: String,
    pub arguments: Value,
    pub is_error: bool,
    pub content: Vec<ContentBlock>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String> },
}

impl ToolResultEvent {
    /// Get all text content concatenated
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replace text content with filtered version
    pub fn with_text_content(mut self, text: String) -> Self {
        self.content = vec![ContentBlock::Text { text }];
        self
    }
}
```

### Task 1.2: Create Plugin Loader

**File**: `crates/crucible-rune/src/plugin_loader.rs` (new)

```rust
use crate::{RuneError, RuneExecutor};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use rune::Unit;
use glob::Pattern;

/// A registered hook from a plugin
#[derive(Debug, Clone)]
pub struct RegisteredHook {
    pub event_type: String,
    pub pattern: Pattern,
    pub handler_name: String,
    pub plugin_path: PathBuf,
    pub unit: Arc<Unit>,
}

/// Plugin loader that discovers and loads Rune plugins
pub struct PluginLoader {
    executor: RuneExecutor,
    hooks: Vec<RegisteredHook>,
    plugin_dir: PathBuf,
}

impl PluginLoader {
    pub fn new(plugin_dir: impl AsRef<Path>) -> Result<Self, RuneError> {
        Ok(Self {
            executor: RuneExecutor::new()?,
            hooks: Vec::new(),
            plugin_dir: plugin_dir.as_ref().to_path_buf(),
        })
    }

    /// Load all plugins from the plugin directory
    pub async fn load_plugins(&mut self) -> Result<(), RuneError> {
        let pattern = self.plugin_dir.join("**/*.rn");
        for entry in glob::glob(pattern.to_str().unwrap()).unwrap() {
            if let Ok(path) = entry {
                self.load_plugin(&path).await?;
            }
        }
        Ok(())
    }

    /// Load a single plugin by calling its init() function
    async fn load_plugin(&mut self, path: &Path) -> Result<(), RuneError> {
        // Compile the plugin
        let source = std::fs::read_to_string(path)?;
        let unit = self.executor.compile("plugin", &source)?;

        // Call init() to get registration data
        let init_result = self.executor.call_function(&unit, "init", ()).await?;

        // Parse hooks from the result
        if let Some(hooks) = init_result.get("hooks").and_then(|v| v.as_array()) {
            for hook in hooks {
                let event_type = hook.get("event")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool_result")
                    .to_string();

                let pattern_str = hook.get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*");

                let handler_name = hook.get("handler")
                    .and_then(|v| v.as_str())
                    .unwrap_or("handle")
                    .to_string();

                self.hooks.push(RegisteredHook {
                    event_type,
                    pattern: Pattern::new(pattern_str).unwrap(),
                    handler_name,
                    plugin_path: path.to_path_buf(),
                    unit: unit.clone(),
                });
            }
        }

        Ok(())
    }

    /// Get hooks that match an event
    pub fn get_matching_hooks(&self, event_type: &str, tool_name: &str) -> Vec<&RegisteredHook> {
        self.hooks
            .iter()
            .filter(|h| h.event_type == event_type && h.pattern.matches(tool_name))
            .collect()
    }
}
```

### Task 1.3: Create Event Pipeline

**File**: `crates/crucible-rune/src/event_pipeline.rs` (new)

```rust
use crate::{PluginLoader, RuneExecutor, ToolResultEvent};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Pipeline for processing events through registered hooks
pub struct EventPipeline {
    loader: Arc<RwLock<PluginLoader>>,
    executor: RuneExecutor,
}

impl EventPipeline {
    pub fn new(loader: Arc<RwLock<PluginLoader>>) -> Result<Self, crate::RuneError> {
        Ok(Self {
            loader,
            executor: RuneExecutor::new()?,
        })
    }

    /// Process a tool result event through all matching hooks
    ///
    /// Returns None if event should pass through unchanged,
    /// Some(event) with the modified event otherwise.
    pub async fn process_tool_result(
        &self,
        event: ToolResultEvent,
    ) -> Result<ToolResultEvent, crate::RuneError> {
        let loader = self.loader.read().await;
        let hooks = loader.get_matching_hooks("tool_result", &event.tool_name);

        if hooks.is_empty() {
            return Ok(event);
        }

        let mut current_event = event;

        for hook in hooks {
            // Convert event to Rune value
            let event_value = serde_json::to_value(&current_event)?;

            // Create context (empty for now, will add kiln/emit later)
            let ctx = serde_json::json!({});

            // Call handler
            let result = self.executor
                .call_function(&hook.unit, &hook.handler_name, (ctx, event_value))
                .await?;

            // Interpret result: None = pass through, Some = modified
            if !result.is_null() {
                current_event = serde_json::from_value(result)?;
            }
        }

        Ok(current_event)
    }
}
```

### Task 1.4: Integrate with ExtendedMcpServer

**File**: `crates/crucible-tools/src/extended_mcp_server.rs`

Add event pipeline integration to Just tool execution:

```rust
// In execute_just_recipe, after getting the result:
let event = ToolResultEvent {
    tool_name: recipe_name.clone(),
    arguments: args.clone(),
    is_error: result.exit_code != 0,
    content: vec![ContentBlock::Text { text: result.stdout.clone() }],
    duration_ms: result.duration_ms,
};

// Process through pipeline
let filtered_event = self.event_pipeline.process_tool_result(event).await?;

// Convert back to MCP response
let content = filtered_event.content.into_iter().map(|c| match c {
    ContentBlock::Text { text } => rmcp::model::Content::text(text),
    // ... other content types
}).collect();
```

### Task 1.5: Create Test Output Filter Example

**File**: `runes/plugins/test_output_filter.rn`

```rune
//! Filter test output to show only summary lines
//! Supports: cargo test, pytest, jest, go test, rspec

/// Register this plugin's hooks
pub fn init() {
    #{
        hooks: [
            #{
                event: "tool_result",
                pattern: "just_test*",
                handler: "filter_test_output",
            }
        ]
    }
}

/// Filter test output to extract summary
pub async fn filter_test_output(ctx, event) {
    let text = event.content[0].text;
    let lines = text.split('\n');

    let summary = extract_summary(lines);

    if summary.len() > 0 {
        // Return modified event with filtered content
        event.content = [#{ type: "text", text: summary }];
        Some(event)
    } else {
        // Pass through unchanged if no summary found
        None
    }
}

/// Extract test summary from output
fn extract_summary(lines) {
    let summary_lines = [];

    for line in lines {
        // Cargo test patterns
        if line.contains("test result:") ||
           line.contains("passed;") ||
           line.contains("failed;") ||
           line.starts_with("error[") ||
           line.starts_with("FAILED") {
            summary_lines.push(line);
        }

        // Pytest patterns
        if line.contains("passed") && line.contains("second") ||
           line.starts_with("PASSED") ||
           line.starts_with("FAILED") ||
           line.starts_with("ERROR") {
            summary_lines.push(line);
        }

        // Jest patterns
        if line.contains("Tests:") ||
           line.contains("Suites:") ||
           line.starts_with("PASS") ||
           line.starts_with("FAIL") {
            summary_lines.push(line);
        }

        // Go test patterns
        if line.starts_with("ok ") ||
           line.starts_with("FAIL") ||
           line.starts_with("---") && line.contains("FAIL") {
            summary_lines.push(line);
        }

        // RSpec patterns
        if line.contains("examples,") ||
           line.contains("failures") ||
           line.starts_with("Finished in") {
            summary_lines.push(line);
        }
    }

    summary_lines.join("\n")
}
```

---

## Phase 2: Long-Term (Full Plugin System)

### Task 2.1: VM Pooling

Implement VM pooling for better performance:

```rust
pub struct VmPool {
    units: HashMap<PathBuf, Arc<Unit>>,
    runtime: Arc<RuntimeContext>,
    max_vms_per_unit: usize,
}

impl VmPool {
    /// Get or create a VM for a plugin
    /// VMs are cheap - just stack frames + Arc refs
    pub fn get_vm(&self, unit: &Arc<Unit>) -> Vm {
        Vm::new(self.runtime.clone(), unit.clone())
    }

    /// Return VM to pool (just drops it, VMs are cheap)
    pub fn return_vm(&self, vm: Vm) {
        // VMs are cheap, just let it drop
        // Could implement vm.clear() for reuse if needed
    }
}
```

### Task 2.2: Context API for Side Effects

Add kiln access and event emission to handler context:

```rust
// Rust side: expose these as Rune modules
module.function(["kiln", "get_note"], |path: String| async {
    // Read note from kiln
})?;

module.function(["kiln", "update_note"], |path: String, content: String| async {
    // Update note in kiln
})?;

module.function(["emit", "event"], |event_type: String, data: Value| async {
    // Emit a new event into the pipeline
})?;
```

```rune
// Rune side usage
pub async fn handle_test_failure(ctx, event) {
    if event.is_error {
        // Log failure to kiln
        let note = `## Test Failure: ${event.tool_name}\n\n${event.content[0].text}`;
        kiln::update_note("logs/test-failures.md", note).await;

        // Emit notification event
        emit::event("notification", #{
            level: "error",
            message: `Test failed: ${event.tool_name}`,
        }).await;
    }

    None  // Pass through unchanged
}
```

### Task 2.3: Additional Event Types

Extend to support more event types:

```rust
pub enum Event {
    ToolResult(ToolResultEvent),
    NoteChanged(NoteChangedEvent),
    AgentMessage(AgentMessageEvent),
    WorkflowStep(WorkflowStepEvent),
}

pub struct NoteChangedEvent {
    pub path: String,
    pub change_type: ChangeType,  // Created, Modified, Deleted
    pub content: Option<String>,
}

pub struct AgentMessageEvent {
    pub agent_id: String,
    pub role: String,  // user, assistant, system
    pub content: String,
}
```

### Task 2.4: Plugin Tools

Allow plugins to register new MCP tools:

```rune
pub fn init() {
    #{
        hooks: [...],
        tools: [
            #{
                name: "analyze_test_history",
                description: "Analyze test failure patterns over time",
                parameters: #{
                    days: #{ type: "integer", default: 7 },
                },
                handler: "analyze_history",
            }
        ]
    }
}

pub async fn analyze_history(ctx, params) {
    let days = params.days ?? 7;
    // Implementation...
}
```

---

## File Structure

```
crates/crucible-rune/
├── src/
│   ├── lib.rs              # Add new module exports
│   ├── executor.rs         # Existing, add call_function method
│   ├── events.rs           # NEW: Event types
│   ├── plugin_loader.rs    # NEW: Plugin discovery and loading
│   ├── event_pipeline.rs   # NEW: Event processing pipeline
│   └── vm_pool.rs          # FUTURE: VM pooling
│
runes/
├── plugins/
│   ├── test_output_filter.rn   # Example filter
│   └── mod.rn                  # Plugin manifest (optional)
```

---

## Phase 1: Granular Implementation Tasks

### Legend
- **Risk**: Low (L), Medium (M), High (H)
- **Parallel**: Tasks that can run concurrently are grouped
- **QA**: Quality checkpoint requiring review before proceeding
- **COMMIT**: Git commit point

---

### Sprint 1: Foundation Types (Parallel)

#### Task 1.1: Event Types [L] ⚡ PARALLEL
**File**: `crates/crucible-rune/src/events.rs`
**Depends on**: Nothing
**Subagent**: Yes

TDD Steps:
1. Write failing tests for `ToolResultEvent` serialization
2. Write failing tests for `ContentBlock` enum variants
3. Write failing tests for `text_content()` helper
4. Write failing tests for `with_text_content()` builder
5. Implement types to pass tests
6. Run `cargo test -p crucible-rune`

```rust
// Tests to write first:
#[test] fn test_tool_result_event_serialize_deserialize()
#[test] fn test_content_block_text_variant()
#[test] fn test_content_block_tagged_serialization()
#[test] fn test_text_content_extracts_text_blocks()
#[test] fn test_text_content_joins_multiple_blocks()
#[test] fn test_with_text_content_replaces_all()
```

**Acceptance**: All tests pass, `cargo clippy` clean

---

#### Task 1.2: Hook Registration Types [L] ⚡ PARALLEL
**File**: `crates/crucible-rune/src/plugin_types.rs`
**Depends on**: Nothing
**Subagent**: Yes

TDD Steps:
1. Write failing tests for `RegisteredHook` struct
2. Write failing tests for `PluginManifest` (parsed init() result)
3. Write failing tests for glob pattern matching
4. Implement types to pass tests

```rust
// Tests to write first:
#[test] fn test_registered_hook_pattern_matches_exact()
#[test] fn test_registered_hook_pattern_matches_glob()
#[test] fn test_registered_hook_pattern_no_match()
#[test] fn test_plugin_manifest_parse_from_json()
#[test] fn test_plugin_manifest_empty_hooks_ok()
#[test] fn test_plugin_manifest_missing_handler_uses_default()
```

**Acceptance**: All tests pass, `cargo clippy` clean

---

#### Task 1.3: Add `call_function` to Executor [M] ⚡ PARALLEL
**File**: `crates/crucible-rune/src/executor.rs`
**Depends on**: Nothing (extends existing)
**Subagent**: Yes

TDD Steps:
1. Write failing test: call function with no args
2. Write failing test: call function with 1 arg
3. Write failing test: call function with 2 args (ctx, event pattern)
4. Write failing test: call async function
5. Write failing test: function returns Option (None vs Some)
6. Implement `call_function(&self, unit: &Arc<Unit>, fn_name: &str, args) -> Result<JsonValue>`
7. Ensure existing tests still pass

```rust
// Tests to write first:
#[tokio::test] async fn test_call_function_no_args()
#[tokio::test] async fn test_call_function_single_arg()
#[tokio::test] async fn test_call_function_two_args()
#[tokio::test] async fn test_call_async_function()
#[tokio::test] async fn test_call_function_returns_none()
#[tokio::test] async fn test_call_function_returns_some_object()
#[tokio::test] async fn test_compile_returns_arc_unit()
```

**Acceptance**: All tests pass, existing executor tests still pass

---

### 🔒 QA Checkpoint 1
- [ ] All Sprint 1 tasks complete
- [ ] `cargo test -p crucible-rune` passes
- [ ] `cargo clippy -p crucible-rune` clean
- [ ] Types are well-documented

### 📦 COMMIT: "feat(rune): add event types and call_function executor method"

---

### Sprint 2: Plugin Loading

#### Task 2.1: Plugin Loader Core [M]
**File**: `crates/crucible-rune/src/plugin_loader.rs`
**Depends on**: Task 1.1, 1.2, 1.3
**Subagent**: Yes

TDD Steps:
1. Write failing test: create loader with empty dir
2. Write failing test: load single plugin with init()
3. Write failing test: init() returns hooks array
4. Write failing test: skip files without init()
5. Write failing test: error handling for invalid Rune
6. Implement `PluginLoader`

```rust
// Tests to write first:
#[tokio::test] async fn test_loader_empty_dir()
#[tokio::test] async fn test_loader_single_plugin()
#[tokio::test] async fn test_loader_parses_hooks_from_init()
#[tokio::test] async fn test_loader_skips_no_init()
#[tokio::test] async fn test_loader_handles_compile_error()
#[tokio::test] async fn test_loader_multiple_plugins()
#[tokio::test] async fn test_get_matching_hooks_filters_correctly()
```

**Acceptance**: All tests pass with temp directories

---

#### Task 2.2: Wire Up lib.rs Exports [L]
**File**: `crates/crucible-rune/src/lib.rs`
**Depends on**: Task 1.1, 1.2, 2.1
**Subagent**: No (trivial)

Steps:
1. Add module declarations
2. Add pub use exports
3. Verify `cargo check -p crucible-rune`

```rust
mod events;
mod plugin_types;
mod plugin_loader;

pub use events::*;
pub use plugin_types::*;
pub use plugin_loader::*;
```

**Acceptance**: `cargo check` passes

---

### 🔒 QA Checkpoint 2
- [ ] Plugin loading works with test fixtures
- [ ] `cargo test -p crucible-rune` all green
- [ ] Manual test: create temp plugin, verify hooks loaded

### 📦 COMMIT: "feat(rune): add plugin loader with init() discovery"

---

### Sprint 3: Event Pipeline

#### Task 3.1: Event Pipeline Implementation [M]
**File**: `crates/crucible-rune/src/event_pipeline.rs`
**Depends on**: Task 2.1, 1.1
**Subagent**: Yes

TDD Steps:
1. Write failing test: no hooks = passthrough
2. Write failing test: single hook modifies event
3. Write failing test: hook returns None = passthrough
4. Write failing test: multiple hooks chain correctly
5. Write failing test: hook error doesn't crash pipeline
6. Implement `EventPipeline`

```rust
// Tests to write first:
#[tokio::test] async fn test_pipeline_no_hooks_passthrough()
#[tokio::test] async fn test_pipeline_hook_modifies_event()
#[tokio::test] async fn test_pipeline_hook_none_passthrough()
#[tokio::test] async fn test_pipeline_multiple_hooks_chain()
#[tokio::test] async fn test_pipeline_hook_error_handling()
#[tokio::test] async fn test_pipeline_pattern_filtering()
```

**Acceptance**: All tests pass

---

### 🔒 QA Checkpoint 3
- [ ] Event pipeline works end-to-end in tests
- [ ] Error handling is robust
- [ ] No panics on bad plugin code

### 📦 COMMIT: "feat(rune): add event pipeline for hook execution"

---

### Sprint 4: MCP Integration (Higher Risk)

#### Task 4.1: Add glob Dependency [L]
**File**: `crates/crucible-rune/Cargo.toml`
**Depends on**: Nothing
**Subagent**: No

```toml
glob = "0.3"
```

**Acceptance**: `cargo check` passes

---

#### Task 4.2: ExtendedMcpServer Pipeline Integration [H]
**File**: `crates/crucible-tools/src/extended_mcp_server.rs`
**Depends on**: Task 3.1
**Subagent**: No (needs careful integration)

Steps:
1. Add `EventPipeline` field to server struct
2. Add `PluginLoader` initialization in constructor
3. Identify Just tool execution point
4. Wrap result in `ToolResultEvent`
5. Process through pipeline
6. Convert back to MCP response
7. Write integration test

```rust
// Key integration points:
// 1. Server::new() - initialize pipeline
// 2. execute_just_recipe() - wrap result, filter, unwrap
// 3. Handle errors gracefully (don't break MCP on plugin error)
```

**Acceptance**:
- Existing MCP tests still pass
- New integration test with mock plugin passes

---

#### Task 4.3: Add crucible-rune Dependency to crucible-tools [L]
**File**: `crates/crucible-tools/Cargo.toml`
**Depends on**: Nothing
**Subagent**: No

Verify crucible-rune is already a dependency, add if not.

**Acceptance**: `cargo check -p crucible-tools`

---

### 🔒 QA Checkpoint 4 (Critical)
- [ ] All existing MCP tests pass
- [ ] `cargo test -p crucible-tools` green
- [ ] Manual test: run MCP server, call just_test, verify it works
- [ ] Manual test: add a broken plugin, verify graceful degradation

### 📦 COMMIT: "feat(tools): integrate event pipeline into MCP server"

---

### Sprint 5: Example Plugin

#### Task 5.1: Create Test Output Filter Plugin [L]
**File**: `runes/plugins/test_output_filter.rn`
**Depends on**: Task 4.2
**Subagent**: Yes

Steps:
1. Write init() function returning hooks
2. Implement filter_test_output handler
3. Add patterns for: cargo test, pytest, jest, go test, rspec
4. Test with sample output strings

```rune
pub fn init() { #{hooks: [...]} }
pub async fn filter_test_output(ctx, event) { ... }
fn extract_cargo_summary(text) { ... }
fn extract_pytest_summary(text) { ... }
// etc.
```

**Acceptance**: Plugin loads without errors

---

#### Task 5.2: Plugin Integration Test [M]
**File**: `crates/crucible-rune/tests/plugin_integration.rs`
**Depends on**: Task 5.1
**Subagent**: Yes

```rust
#[tokio::test]
async fn test_cargo_test_output_filtered() {
    // Load test_output_filter.rn
    // Create ToolResultEvent with cargo test output
    // Process through pipeline
    // Assert only summary lines remain
}

#[tokio::test]
async fn test_pytest_output_filtered() { ... }

#[tokio::test]
async fn test_no_match_passthrough() { ... }
```

**Acceptance**: All integration tests pass

---

### 🔒 QA Checkpoint 5 (Final)
- [ ] Full test suite green: `cargo test`
- [ ] Clippy clean: `cargo clippy`
- [ ] Manual E2E test:
  1. Start MCP server with plugins enabled
  2. Run `just test` through MCP
  3. Verify output is filtered to summary
- [ ] Documentation updated

### 📦 COMMIT: "feat(rune): add test output filter plugin example"

---

## Task Dependency Graph

```
Sprint 1 (Parallel):
  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
  │ 1.1 Events  │  │ 1.2 Types   │  │ 1.3 Executor│
  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘
         │                │                │
         └────────────────┼────────────────┘
                          │
                          ▼
                    [QA Checkpoint 1]
                    [COMMIT]
                          │
Sprint 2:                 ▼
                 ┌─────────────────┐
                 │ 2.1 PluginLoader│
                 └────────┬────────┘
                          │
                 ┌────────┴────────┐
                 │ 2.2 lib.rs      │
                 └────────┬────────┘
                          │
                    [QA Checkpoint 2]
                    [COMMIT]
                          │
Sprint 3:                 ▼
                 ┌─────────────────┐
                 │ 3.1 Pipeline    │
                 └────────┬────────┘
                          │
                    [QA Checkpoint 3]
                    [COMMIT]
                          │
Sprint 4:                 ▼
  ┌─────────────┐  ┌─────────────────┐
  │ 4.1 glob dep│  │ 4.3 rune dep    │
  └──────┬──────┘  └────────┬────────┘
         │                  │
         └──────────┬───────┘
                    ▼
           ┌─────────────────┐
           │ 4.2 MCP Integr. │ [HIGH RISK]
           └────────┬────────┘
                    │
              [QA Checkpoint 4]
              [COMMIT]
                    │
Sprint 5:           ▼
           ┌─────────────────┐
           │ 5.1 Filter.rn   │
           └────────┬────────┘
                    │
           ┌────────┴────────┐
           │ 5.2 Integ Tests │
           └────────┬────────┘
                    │
              [QA Checkpoint 5]
              [COMMIT]
```

---

## Subagent Assignment Summary

| Task | Risk | Subagent? | Notes |
|------|------|-----------|-------|
| 1.1 Event Types | L | ✅ Yes | Pure types, TDD |
| 1.2 Hook Types | L | ✅ Yes | Pure types, TDD |
| 1.3 call_function | M | ✅ Yes | Extends executor |
| 2.1 PluginLoader | M | ✅ Yes | Uses 1.1-1.3 |
| 2.2 lib.rs | L | ❌ No | Trivial wiring |
| 3.1 Pipeline | M | ✅ Yes | Core logic |
| 4.1 glob dep | L | ❌ No | One line |
| 4.2 MCP Integration | H | ❌ No | Careful integration |
| 4.3 rune dep | L | ❌ No | One line |
| 5.1 Filter Plugin | L | ✅ Yes | Rune script |
| 5.2 Integration Tests | M | ✅ Yes | Tests only |

**Parallel Execution Windows:**
- Sprint 1: Tasks 1.1, 1.2, 1.3 can run simultaneously
- Sprint 4: Tasks 4.1, 4.3 can run before 4.2
- Sprint 5: Tasks 5.1, 5.2 are sequential

---

## Phase 2 (Long-term) - Placeholder

1. [ ] VM pooling for performance
2. [ ] Context API with kiln access
3. [ ] Event emission from handlers
4. [ ] Additional event types
5. [ ] Plugin-defined tools
6. [ ] Hot reloading of plugins
