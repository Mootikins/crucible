# Tool System Refactoring Plan

**Status**: Planning Phase
**Branch**: `claude/plan-tool-refactoring-01KN9CNMs5AJSdRqzv676pZZ`
**Date**: 2025-11-20
**Related Spec**: `/openspec/changes/add-tool-system/specs/tool-system/spec.md`

## Executive Summary

Recent refactoring (commit fa1e55f) consolidated 25+ tools into 11 MCP-compatible tools using rmcp 0.9.0. While this achieved significant SOLID improvements and proper dependency injection, several critical pieces remain incomplete:

- 3/4 search tools are stubbed
- Note tools violate kiln-agnostic principle (use filesystem paths directly)
- No permission/approval system implemented
- Missing tools: find_related_notes, get_note_metadata, tag management, wikilink management
- No note name/wikilink resolution system

## Current Implementation Analysis

### What Works ✅

**Architecture**:
- MCP-compatible using rmcp 0.9.0
- `#[tool_router]` and `#[tool]` macros for clean tool definition
- `Parameters<T>` wrapper with `schemars::JsonSchema` for automatic schema generation
- Proper async/await throughout
- Comprehensive unit tests for each module

**Implemented Tools** (5/11 complete):
1. **NoteTools** (5): ✅ create_note, ✅ read_note, ✅ update_note, ✅ delete_note, ✅ list_notes
2. **SearchTools** (1/4): ✅ semantic_search, ⚠️ text_search (stub), ⚠️ metadata_search (stub), ⚠️ tag_search (stub)
3. **KilnTools** (2): ✅ get_roots, ✅ get_stats

### Critical Issues ⚠️

#### 1. **Kiln-Agnostic Violation** (High Priority)
**Location**: `crates/crucible-tools/src/notes.rs:18-45`

**Problem**: All NoteTools use direct filesystem paths:
```rust
#[derive(Deserialize, JsonSchema)]
struct CreateNoteParams {
    path: String,  // ❌ Should be note name/wikilink
    content: String,
}
```

**Spec Requirement** (spec.md:180-182):
> Agents should not directly access filesystem paths as this creates storage dependencies and violates kiln-agnostic principles. All file access tools SHALL use note names and wikilinks.

**Impact**:
- Breaks storage backend abstraction
- Won't work with SurrealDB or future backends
- Violates Dependency Inversion Principle

#### 2. **Incomplete Search Tools** (High Priority)
**Location**: `crates/crucible-tools/src/search.rs:108-162`

**Missing Implementations**:
- `text_search` - Should use ripgrep for full-text search
- `metadata_search` - Should query properties/frontmatter
- `tag_search` - Should find notes by tags

**Current State**: All return "not yet implemented" stubs

#### 3. **No Permission System** (High Priority)
**Spec Requirements** (spec.md:98-117):
- Default read access within scope
- Write permission prompts for modifications
- Directory scope expansion approval
- Session-based auto-approve settings

**Current State**: None of this exists. All operations execute immediately without user approval.

#### 4. **Missing Tools** (Medium Priority)

From spec but not implemented:
- `find_related_notes` - Discover backlinks/forwardlinks
- `get_note_metadata` - Return tags, properties, links
- `add_tag` / `remove_tag` - Tag management
- `create_wikilink` / `remove_wikilink` - Link management
- `list_tags` - Hierarchical tag browsing
- `rebuild_index` - Index management
- `validate_kiln` - Integrity checks

## Proposed Architecture

### Tool Organization

Keep the current 3-router structure but expand tool count:

```rust
// NoteTools - CRUD operations (5 tools)
impl NoteTools {
    create_note(name, content)      // ✅ Exists, needs refactor
    read_note(name)                 // ✅ Exists, needs refactor
    update_note(name, content)      // ✅ Exists, needs refactor
    delete_note(name)               // ✅ Exists, needs refactor
    list_notes(folder?)             // ✅ Exists, needs refactor
}

// SearchTools - Discovery operations (6 tools)
impl SearchTools {
    semantic_search(query, filters) // ✅ Complete
    text_search(query, filters)     // ⚠️ Needs implementation
    metadata_search(properties)     // ⚠️ Needs implementation
    tag_search(tags)                // ⚠️ Needs implementation
    find_related_notes(name)        // ❌ Not implemented
    get_note_metadata(name)         // ❌ Not implemented
}

// KilnTools - Admin & metadata operations (6 tools)
impl KilnTools {
    get_roots()                     // ✅ Complete
    get_stats()                     // ✅ Complete
    list_tags()                     // ❌ Not implemented
    add_tag(name, tag)              // ❌ Not implemented
    remove_tag(name, tag)           // ❌ Not implemented
    validate_kiln()                 // ❌ Not implemented
}

// Total: 17 tools (5 + 6 + 6)
```

### Note Reference System

**Key Design Decision**: How should agents refer to notes?

**Proposed Approach** (from spec):
1. **Primary**: Note name (e.g., "My Project Notes")
2. **Alternative**: Wikilink format (e.g., "[[My Project Notes]]")
3. **Optional**: Path context for disambiguation (e.g., "projects/My Project Notes")

**Implementation**:
```rust
pub trait NoteResolver {
    /// Resolve a note reference to concrete storage location
    async fn resolve_note(&self, reference: &str) -> Result<ResolvedNote>;

    /// Handle ambiguous references (multiple matches)
    async fn disambiguate(&self, reference: &str) -> Result<Vec<NoteCandidate>>;
}

pub struct ResolvedNote {
    pub canonical_name: String,
    pub storage_location: StorageLocation, // Opaque to tools
    pub metadata: NoteMetadata,
}

// NoteTools use NoteResolver, never touch paths directly
impl NoteTools {
    fn new(
        kiln_path: String,
        resolver: Arc<dyn NoteResolver>,  // 👈 New dependency
        repo: Arc<dyn KnowledgeRepository>,
    ) -> Self { ... }
}
```

### Permission System Architecture

**Three-Layer Model**:

```rust
/// Permission layer wraps tool execution
pub struct PermissionManager {
    /// Check if operation requires approval
    fn requires_approval(&self, operation: &ToolOperation) -> bool;

    /// Request user approval for operation
    async fn request_approval(&self, operation: &ToolOperation) -> Result<bool>;

    /// Check session-level auto-approve settings
    fn is_auto_approved(&self, operation: &ToolOperation) -> bool;
}

pub enum PermissionScope {
    Read(PathBuf),          // Read within directory - auto-approve
    WriteCreate(PathBuf),   // Create/modify - needs approval
    WriteDelete(PathBuf),   // Delete - needs approval + warning
    ScopeExpansion(PathBuf), // Access outside CWD - needs approval
}

pub struct ToolOperation {
    pub tool_name: String,
    pub scope: PermissionScope,
    pub description: String,  // Human-readable operation description
}
```

**Integration Point**:
```rust
#[tool_router]
impl NoteTools {
    #[tool(description = "Create a new note")]
    async fn create_note(&self, params: Parameters<CreateNoteParams>)
        -> Result<CallToolResult, rmcp::ErrorData>
    {
        // 1. Resolve note name to location
        let resolved = self.resolver.resolve_note(&params.name).await?;

        // 2. Check permissions
        let operation = ToolOperation {
            tool_name: "create_note".into(),
            scope: PermissionScope::WriteCreate(resolved.path()),
            description: format!("Create note '{}'", params.name),
        };

        if !self.permissions.request_approval(&operation).await? {
            return Err(rmcp::ErrorData::invalid_params("User denied permission", None));
        }

        // 3. Execute via storage abstraction
        self.repo.create_note(&resolved, &params.content).await?;

        // 4. Return success
        Ok(CallToolResult::success(vec![...]))
    }
}
```

## Implementation Phases

### Phase 1: Foundation (Week 1)
**Goal**: Fix kiln-agnostic violations, establish architecture

**Tasks**:
1. ✅ Design NoteResolver trait and interface
2. ✅ Implement file-based NoteResolver
3. ✅ Design PermissionManager interface
4. ✅ Refactor NoteTools to use note names instead of paths
5. ✅ Update all NoteTools parameter structs
6. ✅ Update NoteTools tests to use new interface

**Deliverable**: NoteTools fully kiln-agnostic with note name resolution

### Phase 2: Permission System (Week 1-2)
**Goal**: Implement user approval and permission controls

**Tasks**:
1. ✅ Implement PermissionManager core
2. ✅ Create approval prompt mechanism (CLI integration point)
3. ✅ Add session-based auto-approve settings
4. ✅ Integrate permission checks into NoteTools
5. ✅ Add audit logging for operations
6. ✅ Write permission system tests

**Deliverable**: Working permission system for all write operations

### Phase 3: Complete Search Tools (Week 2)
**Goal**: Finish stubbed search implementations

**Tasks**:
1. ✅ Implement `text_search` using ripgrep
2. ✅ Implement `metadata_search` via KnowledgeRepository
3. ✅ Implement `tag_search` via KnowledgeRepository
4. ✅ Add proper error handling and validation
5. ✅ Write comprehensive search tool tests

**Deliverable**: All 4 SearchTools fully functional

### Phase 4: Missing Tools (Week 2-3)
**Goal**: Add tools specified in openspec but not yet implemented

**Tasks**:
1. ✅ Implement `find_related_notes` (backlinks/forwardlinks)
2. ✅ Implement `get_note_metadata` (tags, links, properties)
3. ✅ Implement tag management tools (add_tag, remove_tag, list_tags)
4. ✅ Implement `validate_kiln` (integrity checks)
5. ✅ Write tests for all new tools

**Deliverable**: Complete tool set per openspec (17 tools)

### Phase 5: Integration & Polish (Week 3)
**Goal**: CLI integration, documentation, testing

**Tasks**:
1. ✅ Integrate tools with ACP client
2. ✅ Add CLI commands for permission management
3. ✅ Write comprehensive integration tests
4. ✅ Performance testing and optimization
5. ✅ Complete API documentation
6. ✅ Update openspec with implementation status

**Deliverable**: Production-ready tool system

## SOLID Compliance

### Single Responsibility Principle ✅
Each tool module has one clear purpose:
- `NoteTools`: CRUD operations only
- `SearchTools`: Discovery operations only
- `KilnTools`: Admin/metadata operations only

### Open/Closed Principle ✅
- New storage backends can be added via `NoteResolver` trait
- New tools can be added without modifying existing routers
- Permission rules can be extended without changing core logic

### Liskov Substitution Principle ✅
- All `NoteResolver` implementations interchangeable
- All storage backends interchangeable via `KnowledgeRepository`

### Interface Segregation Principle ✅
- Tools depend only on needed traits (`NoteResolver`, `KnowledgeRepository`)
- No fat interfaces with unused methods

### Dependency Inversion Principle ✅
- Tools depend on abstractions (`NoteResolver`, `KnowledgeRepository`)
- Concrete implementations injected at construction time
- No direct filesystem dependencies

## Testing Strategy

### Unit Tests
- Each tool function has independent tests
- Mock implementations of `NoteResolver` and `KnowledgeRepository`
- Permission system tested in isolation

### Integration Tests
- End-to-end flows through tool routers
- Real filesystem operations in temp directories
- Permission approval workflows

### Performance Tests
- Measure tool execution times
- Test with large kilns (1000+ notes)
- Stress test search operations

## Open Questions

1. **Note Name Disambiguation**: When multiple notes have same name, how should we present choices to user?
   - Option A: Return all matches with paths, let agent choose
   - Option B: Use most recently modified
   - Option C: Require path context in ambiguous cases

2. **Permission Persistence**: Where should session-based auto-approve settings be stored?
   - Option A: In-memory only (lost on restart)
   - Option B: Session-specific config file
   - Option C: Global user preferences

3. **Wikilink Resolution**: Should we support full Obsidian-style wikilink features?
   - Headers: `[[Note#Section]]`
   - Aliases: `[[Note|Display Text]]`
   - Embeds: `![[Note]]`

4. **Tag Hierarchy**: How deep should hierarchical tags go?
   - Unlimited depth (like filesystem)
   - Fixed depth (e.g., 3 levels max)
   - Flat tags only

## Success Metrics

- ✅ All 17 tools implemented and tested
- ✅ Zero direct filesystem path references in tool parameters
- ✅ 100% test coverage for tool functions
- ✅ Permission system enforces all write operations
- ✅ Works with both file-based and SurrealDB backends
- ✅ Performance: <100ms for CRUD operations, <500ms for search
- ✅ Integration tests pass with ACP client

## Next Steps

1. **Review this plan** - Discuss open questions and architecture decisions
2. **Prioritize phases** - Confirm phase order and timeline
3. **Start Phase 1** - Begin with NoteResolver implementation
4. **Iterate** - Adjust plan based on implementation learnings

---

**References**:
- Openspec: `/openspec/changes/add-tool-system/specs/tool-system/spec.md`
- Tasks: `/openspec/changes/add-tool-system/tasks.md`
- SOLID Analysis: `/docs/SOLID_REFACTORING_ANALYSIS.md`
- Current Implementation: `/crates/crucible-tools/src/`
