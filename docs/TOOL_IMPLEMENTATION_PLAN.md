# Tool System Implementation Plan - Exhaustive

**Date**: 2025-11-20
**Scope**: `crates/crucible-tools` only
**Approach**: Test-Driven Development (TDD)
**Strategy**: Delete legacy code liberally, follow SOLID principles

## Current State Analysis

### Existing Files in `crates/crucible-tools/src/`

| File | Lines | Status | Action |
|------|-------|--------|--------|
| `lib.rs` | 106 | Partial | Update exports, remove legacy |
| `notes.rs` | 428 | Mostly good | Add frontmatter support, line ranges |
| `search.rs` | 314 | Stubs | Implement text_search, property_search |
| `kiln.rs` | 167 | Mostly good | Combine get_roots + get_stats |
| `permission.rs` | 108 | Legacy | **DELETE** - Replace with new design |
| `types.rs` | 310 | Legacy | **DELETE** - Use only rmcp types |
| `system_tools.rs` | 454 | Legacy | **DELETE** - Not part of final design |
| `database_tools.rs` | 317 | Legacy | **DELETE** - Not part of final design |

### Dependencies (from Cargo.toml)

Need to add:
- `serde_yaml` - For frontmatter serialization
- `walkdir` - For directory traversal in property_search
- `ripgrep` or process `rg` - For text_search

## Phase-by-Phase Implementation

---

## Phase 1: Search Tools (Week 1, Days 1-2)

### Goal
Complete `text_search` and `property_search` implementations with full test coverage.

### Files to Create/Modify

#### 1.1. `crates/crucible-tools/Cargo.toml`
**Action**: Add dependencies
```toml
[dependencies]
# Existing...
walkdir = "2"
serde_yaml = "0.9"
```

**Test**: `cargo check` passes

#### 1.2. `crates/crucible-tools/src/search.rs`
**Action**: Implement text_search and property_search

**Current state**: Has semantic_search (working), text_search (stub), metadata_search (stub), tag_search (stub)

**Changes needed**:

1. **Remove stubs**: Delete `metadata_search` and `tag_search` (no longer needed)

2. **Implement `text_search`**:
```rust
#[derive(Deserialize, JsonSchema)]
struct TextSearchParams {
    query: String,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default = "default_true")]
    case_insensitive: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[tool(description = "Fast full-text search using ripgrep")]
async fn text_search(&self, params: Parameters<TextSearchParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    // Use std::process::Command to call 'rg'
    // Parse output into structured results
    // Return matches with line numbers and context
}
```

3. **Implement `property_search`**:
```rust
#[derive(Deserialize, JsonSchema)]
struct PropertySearchParams {
    properties: serde_json::Value,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[tool(description = "Search notes by frontmatter properties (includes tags)")]
async fn property_search(&self, params: Parameters<PropertySearchParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    // Use walkdir to find all .md files
    // For each file, parse frontmatter
    // Check if properties match (AND logic, array OR logic)
    // Return matches with frontmatter + basic stats
}
```

**Tests to write** (`src/search.rs` inline):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // text_search tests
    #[tokio::test]
    async fn test_text_search_basic() { }

    #[tokio::test]
    async fn test_text_search_case_insensitive() { }

    #[tokio::test]
    async fn test_text_search_with_folder() { }

    #[tokio::test]
    async fn test_text_search_limit() { }

    // property_search tests
    #[tokio::test]
    async fn test_property_search_single_property() { }

    #[tokio::test]
    async fn test_property_search_multiple_properties_and() { }

    #[tokio::test]
    async fn test_property_search_tags_or_logic() { }

    #[tokio::test]
    async fn test_property_search_no_frontmatter() { }

    #[tokio::test]
    async fn test_property_search_limit() { }
}
```

**TDD Flow**:
1. Write `test_text_search_basic` (fails)
2. Implement minimal `text_search` to pass
3. Write `test_text_search_case_insensitive` (fails)
4. Extend implementation
5. Repeat for all text_search tests
6. Repeat for property_search tests

**Commit**: "feat(tools): implement text_search with ripgrep integration"
**Commit**: "feat(tools): implement property_search with frontmatter filtering"

---

## Phase 2: Metadata Tools (Week 1, Days 3-4)

### Goal
Add `read_metadata` tool and line range support to `read_note`.

### Files to Modify

#### 2.1. `crates/crucible-tools/src/notes.rs`

**Current tools**:
- `create_note(path, content)` ✅
- `read_note(path)` ⚠️ Needs line range support
- `update_note(path, content)` ⚠️ Needs frontmatter support (Phase 3)
- `delete_note(path)` ✅
- `list_notes(folder)` ⚠️ Needs frontmatter option

**Changes needed**:

1. **Add `read_metadata` tool**:
```rust
#[derive(Deserialize, JsonSchema)]
struct ReadMetadataParams {
    path: String,
}

#[tool(description = "Read note metadata without loading content")]
async fn read_metadata(&self, params: Parameters<ReadMetadataParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    // Read file
    // Parse frontmatter only (don't need full AST)
    // Count basic stats (lines, words, chars)
    // Return frontmatter + stats, NO content
}
```

2. **Update `read_note` with line ranges**:
```rust
#[derive(Deserialize, JsonSchema)]
struct ReadNoteParams {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
}

#[tool(description = "Read note content with optional line range")]
async fn read_note(&self, params: Parameters<ReadNoteParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    // Read file
    // If start_line or end_line specified, slice lines
    // Return content + total_lines + lines_returned
}
```

3. **Update `list_notes` with frontmatter option**:
```rust
#[derive(Deserialize, JsonSchema)]
struct ListNotesParams {
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    include_frontmatter: bool,
    #[serde(default = "default_true")]
    recursive: bool,
}
```

**Tests to write**:

```rust
#[cfg(test)]
mod tests {
    // read_metadata tests
    #[tokio::test]
    async fn test_read_metadata_with_frontmatter() { }

    #[tokio::test]
    async fn test_read_metadata_without_frontmatter() { }

    #[tokio::test]
    async fn test_read_metadata_stats() { }

    // read_note line range tests
    #[tokio::test]
    async fn test_read_note_full() { }

    #[tokio::test]
    async fn test_read_note_first_n_lines() { }

    #[tokio::test]
    async fn test_read_note_last_n_lines() { }

    #[tokio::test]
    async fn test_read_note_line_range() { }

    #[tokio::test]
    async fn test_read_note_out_of_bounds() { }

    // list_notes tests
    #[tokio::test]
    async fn test_list_notes_with_frontmatter() { }

    #[tokio::test]
    async fn test_list_notes_recursive() { }

    #[tokio::test]
    async fn test_list_notes_non_recursive() { }
}
```

**TDD Flow**: Same as Phase 1 - write failing test, implement, repeat.

**Commit**: "feat(tools): add read_metadata tool for efficient metadata access"
**Commit**: "feat(tools): add line range support to read_note"
**Commit**: "feat(tools): add frontmatter option to list_notes"

---

## Phase 3: Frontmatter in CRUD (Week 1-2, Days 5-7)

### Goal
Add frontmatter parameter support to `create_note` and `update_note`.

### Files to Modify

#### 3.1. `crates/crucible-tools/src/notes.rs`

**Changes needed**:

1. **Update `create_note` with frontmatter**:
```rust
#[derive(Deserialize, JsonSchema)]
struct CreateNoteParams {
    path: String,
    content: String,
    #[serde(default)]
    frontmatter: Option<serde_json::Value>,
}

#[tool(description = "Create a new note with optional frontmatter")]
async fn create_note(&self, params: Parameters<CreateNoteParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let full_path = Path::new(&self.kiln_path).join(&params.path);

    // Build content with frontmatter if provided
    let content = if let Some(fm) = params.frontmatter {
        let yaml = serde_yaml::to_string(&fm)?;
        format!("---\n{}\n---\n\n{}", yaml, params.content)
    } else {
        params.content
    };

    // Write file
    tokio::fs::write(&full_path, &content).await?;

    // Return success
}
```

2. **Update `update_note` with frontmatter**:
```rust
#[derive(Deserialize, JsonSchema)]
struct UpdateNoteParams {
    path: String,
    content: Option<String>,
    frontmatter: Option<serde_json::Value>,
}

#[tool(description = "Update note content and/or frontmatter")]
async fn update_note(&self, params: Parameters<UpdateNoteParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let full_path = Path::new(&self.kiln_path).join(&params.path);

    // Read existing file
    let existing = tokio::fs::read_to_string(&full_path).await?;

    // Parse to separate frontmatter and content
    let (old_fm, old_content) = parse_frontmatter(&existing);

    // Determine new frontmatter
    let new_fm = params.frontmatter.or(old_fm);

    // Determine new content
    let new_content = params.content.unwrap_or(old_content);

    // Rebuild file
    let final_content = if let Some(fm) = new_fm {
        let yaml = serde_yaml::to_string(&fm)?;
        format!("---\n{}\n---\n\n{}", yaml, new_content)
    } else {
        new_content
    };

    // Write file
    tokio::fs::write(&full_path, &final_content).await?;

    // Return which fields were updated
}
```

3. **Add helper function**:
```rust
fn parse_frontmatter(content: &str) -> (Option<serde_json::Value>, String) {
    // Simple frontmatter parser
    // Returns (frontmatter_json, content_without_frontmatter)
}
```

**Tests to write**:

```rust
#[cfg(test)]
mod tests {
    // create_note with frontmatter
    #[tokio::test]
    async fn test_create_note_with_frontmatter() { }

    #[tokio::test]
    async fn test_create_note_without_frontmatter() { }

    #[tokio::test]
    async fn test_create_note_frontmatter_serialization() { }

    // update_note with frontmatter
    #[tokio::test]
    async fn test_update_note_content_only() { }

    #[tokio::test]
    async fn test_update_note_frontmatter_only() { }

    #[tokio::test]
    async fn test_update_note_both() { }

    #[tokio::test]
    async fn test_update_note_preserves_content_when_updating_frontmatter() { }

    #[tokio::test]
    async fn test_update_note_preserves_frontmatter_when_updating_content() { }

    // parse_frontmatter helper
    #[test]
    fn test_parse_frontmatter_yaml() { }

    #[test]
    fn test_parse_frontmatter_none() { }

    #[test]
    fn test_parse_frontmatter_malformed() { }
}
```

**TDD Flow**: Same as previous phases.

**Commit**: "feat(tools): add frontmatter support to create_note"
**Commit**: "feat(tools): add frontmatter support to update_note"
**Commit**: "test(tools): add comprehensive frontmatter tests"

---

## Phase 4: Permission System (Week 2, Days 8-10)

### Goal
Implement user approval system for write operations.

### Files to Create/Modify

#### 4.1. **DELETE** `crates/crucible-tools/src/permission.rs`
**Reason**: Legacy design, doesn't match new requirements.

#### 4.2. **CREATE** `crates/crucible-tools/src/permissions.rs`
**Action**: Implement new permission system

```rust
//! Permission management for tool operations

use std::path::PathBuf;
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;

/// Type of operation being performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Read,
    Create,
    Update,
    Delete,
    Administrative,
}

/// Scope of permission being requested
#[derive(Debug, Clone)]
pub enum PermissionScope {
    ReadInScope(PathBuf),
    ReadOutOfScope(PathBuf),
    WriteCreate { target: PathBuf, size_bytes: u64 },
    WriteUpdate { target: PathBuf, size_bytes: u64 },
    WriteDelete { target: PathBuf, has_backlinks: bool },
    Admin { operation: String, affects_count: usize },
}

/// Description of a tool operation requiring permission
#[derive(Debug, Clone)]
pub struct ToolOperation {
    pub tool_name: String,
    pub operation_type: OperationType,
    pub scope: PermissionScope,
    pub description: String,
    pub details: HashMap<String, String>,
}

/// Manages permission approval for tool operations
pub struct PermissionManager {
    working_directory: PathBuf,
    auto_approve_settings: HashMap<OperationType, bool>,
    approval_fn: Arc<
        dyn Fn(&ToolOperation) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send>>
            + Send
            + Sync,
    >,
}

impl PermissionManager {
    pub fn new(
        working_directory: PathBuf,
        approval_fn: impl Fn(&ToolOperation) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            working_directory,
            auto_approve_settings: HashMap::new(),
            approval_fn: Arc::new(approval_fn),
        }
    }

    pub fn requires_approval(&self, operation: &ToolOperation) -> bool {
        match &operation.scope {
            PermissionScope::ReadInScope(_) => false,
            _ => !self.is_auto_approved(operation),
        }
    }

    pub async fn request_approval(&self, operation: &ToolOperation) -> anyhow::Result<bool> {
        if self.is_auto_approved(operation) {
            return Ok(true);
        }

        if !self.requires_approval(operation) {
            return Ok(true);
        }

        (self.approval_fn)(operation).await
    }

    pub fn is_auto_approved(&self, operation: &ToolOperation) -> bool {
        self.auto_approve_settings
            .get(&operation.operation_type)
            .copied()
            .unwrap_or(false)
    }

    pub fn set_auto_approve(&mut self, operation_type: OperationType, enabled: bool) {
        self.auto_approve_settings.insert(operation_type, enabled);
    }
}

// For testing: always-approve implementation
#[cfg(test)]
impl PermissionManager {
    pub fn always_approve(working_directory: PathBuf) -> Self {
        Self::new(working_directory, |_| Box::pin(async { Ok(true) }))
    }
}
```

#### 4.3. Integrate permissions into NoteTools

**Modify** `crates/crucible-tools/src/notes.rs`:

```rust
use crate::permissions::{PermissionManager, ToolOperation, OperationType, PermissionScope};

#[derive(Clone)]
pub struct NoteTools {
    kiln_path: String,
    permissions: Arc<PermissionManager>,
}

impl NoteTools {
    pub fn new(kiln_path: String, permissions: Arc<PermissionManager>) -> Self {
        Self { kiln_path, permissions }
    }
}

// In create_note:
async fn create_note(...) {
    let operation = ToolOperation {
        tool_name: "create_note".to_string(),
        operation_type: OperationType::Create,
        scope: PermissionScope::WriteCreate {
            target: full_path.clone(),
            size_bytes: content.len() as u64,
        },
        description: format!("Create note '{}'", params.path),
        details: HashMap::new(),
    };

    if !self.permissions.request_approval(&operation).await? {
        return Err(rmcp::ErrorData::invalid_params("User denied permission", None));
    }

    // ... rest of implementation
}
```

**Tests to write**:

```rust
#[cfg(test)]
mod permissions_tests {
    #[test]
    fn test_permission_scope_read_in_scope() { }

    #[test]
    fn test_permission_scope_write_create() { }

    #[test]
    fn test_auto_approve_settings() { }

    #[tokio::test]
    async fn test_approval_callback() { }
}

#[cfg(test)]
mod notes_permission_tests {
    #[tokio::test]
    async fn test_create_note_requires_approval() { }

    #[tokio::test]
    async fn test_create_note_denied() { }

    #[tokio::test]
    async fn test_update_note_requires_approval() { }

    #[tokio::test]
    async fn test_delete_note_requires_approval() { }

    #[tokio::test]
    async fn test_read_note_no_approval() { }
}
```

**TDD Flow**: Same as previous phases.

**Commit**: "feat(tools): implement permission system for write operations"
**Commit**: "feat(tools): integrate permissions into NoteTools"
**Commit**: "test(tools): add comprehensive permission tests"

---

## Phase 5: Kiln Tools Consolidation (Week 2, Day 11)

### Goal
Combine `get_roots` and `get_stats` into single `get_kiln_info` tool.

### Files to Modify

#### 5.1. `crates/crucible-tools/src/kiln.rs`

**Current tools**:
- `get_kiln_roots()` ✅
- `get_kiln_stats()` ✅

**Changes needed**:

1. **Remove** `get_kiln_roots` and `get_kiln_stats`

2. **Add** `get_kiln_info`:
```rust
#[derive(Deserialize, JsonSchema)]
struct GetKilnInfoParams {
    #[serde(default)]
    detailed: bool,
}

#[tool(description = "Get kiln root information and statistics")]
async fn get_kiln_info(&self, params: Parameters<GetKilnInfoParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let roots = vec![serde_json::json!({
        "uri": format!("file://{}", self.kiln_path.canonicalize()?.display()),
        "name": "Kiln Root"
    })];

    // Basic stats
    let stats = if params.detailed {
        // Compute detailed stats
    } else {
        // Compute basic stats (count, size)
    };

    Ok(CallToolResult::success(vec![
        rmcp::model::Content::json(serde_json::json!({
            "roots": roots,
            "stats": stats
        }))?
    ]))
}
```

**Tests to write**:

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_get_kiln_info_basic() { }

    #[tokio::test]
    async fn test_get_kiln_info_detailed() { }

    #[tokio::test]
    async fn test_get_kiln_info_roots_format() { }
}
```

**Commit**: "refactor(tools): combine get_roots and get_stats into get_kiln_info"

---

## Phase 6: Cleanup and Legacy Deletion (Week 2, Day 12)

### Goal
Remove all legacy code and update exports.

### Files to DELETE

1. **DELETE** `crates/crucible-tools/src/permission.rs` (old design)
2. **DELETE** `crates/crucible-tools/src/types.rs` (legacy ToolFunction types)
3. **DELETE** `crates/crucible-tools/src/system_tools.rs` (not part of final design)
4. **DELETE** `crates/crucible-tools/src/database_tools.rs` (legacy)

### Files to MODIFY

#### 6.1. `crates/crucible-tools/src/lib.rs`

**Action**: Clean up exports

```rust
//! Crucible Tools - MCP-compatible tools for knowledge management
//!
//! This crate provides 10 focused tools for the Crucible knowledge management system.
//!
//! ## Tool Categories
//!
//! - **NoteTools** (6): create_note, read_note, read_metadata, update_note, delete_note, list_notes
//! - **SearchTools** (3): text_search, property_search, semantic_search
//! - **KilnTools** (1): get_kiln_info

pub mod notes;
pub mod search;
pub mod kiln;
pub mod permissions;

pub use notes::NoteTools;
pub use search::SearchTools;
pub use kiln::KilnTools;
pub use permissions::{PermissionManager, ToolOperation, OperationType, PermissionScope};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn init() {
    tracing::info!("Initializing crucible-tools v{}", VERSION);
    tracing::info!("10 tools available: 6 NoteTools, 3 SearchTools, 1 KilnTools");
}
```

**Commit**: "refactor(tools): remove legacy code and clean up exports"
**Commit**: "docs(tools): update lib.rs with final tool documentation"

---

## Phase 7: Integration Tests (Week 2, Day 13)

### Goal
Create end-to-end integration tests.

### Files to CREATE

#### 7.1. `crates/crucible-tools/tests/integration_tests.rs`

```rust
//! Integration tests for crucible-tools

use crucible_tools::{NoteTools, SearchTools, KilnTools, PermissionManager};
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_full_workflow_create_search_read() {
    // Create temp kiln
    // Create note with frontmatter
    // Search by property
    // Read metadata
    // Verify results
}

#[tokio::test]
async fn test_full_workflow_update_frontmatter() {
    // Create note
    // Update frontmatter only
    // Read metadata
    // Verify frontmatter changed, content unchanged
}

#[tokio::test]
async fn test_full_workflow_text_search() {
    // Create multiple notes
    // Text search
    // Verify results
}

#[tokio::test]
async fn test_permission_approval_workflow() {
    // Create note (with approval)
    // Verify permission callback called
    // Update note (denied)
    // Verify error
}
```

**Commit**: "test(tools): add integration tests for full workflows"

---

## Summary: File Changes

### Files to DELETE (4 files)
- [x] `src/permission.rs` (108 lines) - Legacy design
- [x] `src/types.rs` (310 lines) - Legacy ToolFunction types
- [x] `src/system_tools.rs` (454 lines) - Not in final design
- [x] `src/database_tools.rs` (317 lines) - Legacy

**Lines deleted**: 1,189

### Files to MODIFY (4 files)
- [x] `src/lib.rs` - Clean up exports
- [x] `src/notes.rs` - Add frontmatter, line ranges, permissions
- [x] `src/search.rs` - Implement text_search, property_search
- [x] `src/kiln.rs` - Combine into get_kiln_info

### Files to CREATE (2 files)
- [x] `src/permissions.rs` - New permission system
- [x] `tests/integration_tests.rs` - Integration tests

### Expected Final State

```
crates/crucible-tools/
├── Cargo.toml (updated dependencies)
├── src/
│   ├── lib.rs (cleaned up, ~50 lines)
│   ├── notes.rs (~600 lines with tests)
│   ├── search.rs (~500 lines with tests)
│   ├── kiln.rs (~150 lines with tests)
│   └── permissions.rs (~200 lines with tests)
└── tests/
    └── integration_tests.rs (~200 lines)
```

**Total lines**: ~1,700 (down from ~2,200)
**Test coverage**: >80% target

---

## Commit Strategy

### Commit Frequency
- After each passing test
- After each feature implementation
- After each deletion

### Commit Message Format
```
<type>(tools): <description>

[optional body]
```

Types: `feat`, `test`, `refactor`, `docs`, `chore`

### Example Commit Sequence

1. `feat(tools): implement text_search with ripgrep integration`
2. `test(tools): add comprehensive text_search tests`
3. `feat(tools): implement property_search with frontmatter filtering`
4. `test(tools): add property_search tests with AND/OR logic`
5. `feat(tools): add read_metadata tool for efficient metadata access`
6. `test(tools): add read_metadata tests`
7. `feat(tools): add line range support to read_note`
8. `test(tools): add line range tests for read_note`
9. `feat(tools): add frontmatter option to list_notes`
10. `feat(tools): add frontmatter support to create_note`
11. `test(tools): add frontmatter creation tests`
12. `feat(tools): add frontmatter support to update_note`
13. `test(tools): add frontmatter update tests`
14. `feat(tools): implement permission system`
15. `test(tools): add permission system tests`
16. `feat(tools): integrate permissions into NoteTools`
17. `test(tools): add permission integration tests`
18. `refactor(tools): combine get_roots and get_stats into get_kiln_info`
19. `test(tools): add get_kiln_info tests`
20. `chore(tools): delete legacy permission.rs`
21. `chore(tools): delete legacy types.rs`
22. `chore(tools): delete legacy system_tools.rs`
23. `chore(tools): delete legacy database_tools.rs`
24. `refactor(tools): clean up lib.rs exports`
25. `docs(tools): update lib.rs documentation`
26. `test(tools): add integration tests`

**Total commits**: ~25-30

---

## Testing Strategy

### Unit Tests
- Every tool function has tests
- Edge cases covered (empty input, malformed data, file not found)
- Error cases tested

### Integration Tests
- Full workflows tested
- Tool interactions verified
- Permission system integration verified

### Test Data
Use `tempfile::TempDir` for all tests to avoid pollution.

Create helper functions:
```rust
fn create_test_note(dir: &Path, name: &str, content: &str, frontmatter: Option<Value>) -> PathBuf
fn create_test_kiln(notes: Vec<(&str, &str, Option<Value>)>) -> TempDir
```

---

## Dependencies to Add

```toml
[dependencies]
walkdir = "2"
serde_yaml = "0.9"

[dev-dependencies]
tempfile = "3"
```

---

## SOLID Compliance Checklist

### Single Responsibility
- [x] NoteTools: Only note CRUD operations
- [x] SearchTools: Only search operations
- [x] KilnTools: Only system info
- [x] PermissionManager: Only permission checking

### Open/Closed
- [x] New search types can be added without modifying existing
- [x] Permission policies can be extended
- [x] New tool categories can be added

### Liskov Substitution
- [x] All tool implementations use rmcp::tool macro consistently
- [x] PermissionManager can be swapped

### Interface Segregation
- [x] Tools don't depend on unused dependencies
- [x] Each tool module is independent

### Dependency Inversion
- [x] NoteTools depends on PermissionManager abstraction (trait-like via callback)
- [x] SearchTools depends on injected KnowledgeRepository and EmbeddingProvider

---

## Risk Mitigation

### Risk: ripgrep not installed
**Mitigation**: Check for `rg` binary at runtime, return clear error if missing

### Risk: Large kilns cause property_search to timeout
**Mitigation**: Add early exit when limit reached, document performance characteristics

### Risk: Frontmatter parsing errors
**Mitigation**: Graceful fallback, return empty object if YAML invalid

### Risk: Permission callback not provided
**Mitigation**: Provide default always-approve for tests, require for production

---

## Next Steps After Implementation

1. **CLI Integration**: Update crucible-cli to use new tools
2. **ACP Integration**: Expose tools via MCP server
3. **Documentation**: Write user guide for each tool
4. **Performance Testing**: Benchmark search operations on large kilns

---

**Ready to implement? Start with Phase 1: Search Tools using TDD approach.**
