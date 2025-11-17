# Comprehensive Cleanup Plan for CLI Rework

## Executive Summary

**Found: ~5,000 lines of redundant code + 12 disabled files**

The codebase has significant architectural debt from the old polling-based embedding system. The new `crucible-pipeline` architecture (5-phase NotePipeline) supersedes these modules, but they haven't been removed yet.

## Major Redundant Modules (4,686 lines)

### 1. `kiln_integration.rs` (2,500 lines) - **PARTIALLY REDUNDANT**
**Location**: `crates/crucible-surrealdb/src/kiln_integration.rs`

**Redundant Functions** (old processing logic):
- `process_kiln_files()` - Replaced by `NotePipeline::process()`
- `process_kiln_delta()` - Replaced by pipeline's Merkle diff phase
- Old embedding queue logic - Replaced by event-driven processing

**KEEP These Functions** (still needed):
- `semantic_search()` (line 1652) - **USED by semantic.rs.disabled**
- `semantic_search_with_reranking()` (line 1765) - **USED by semantic.rs.disabled**
- `store_parsed_document()` - Storage layer, not redundant
- `initialize_kiln_schema()` - Schema management, not redundant
- `get_embedding_index_metadata()` - Metadata queries, not redundant

**Action**: Refactor to split into:
- `kiln_storage.rs` - Keep storage/schema functions
- `kiln_semantic_search.rs` - Keep semantic search functions
- DELETE: Old processing functions

### 2. `kiln_scanner.rs` (1,429 lines) - **FULLY REDUNDANT**
**Location**: `crates/crucible-surrealdb/src/kiln_scanner.rs`

**Has TODO (line 18-21)**:
```rust
// TODO: Update to use new enrichment architecture (NoteEnricher)
// This module was part of the old embedding_pool polling architecture.
// The new architecture uses file watchers + NoteEnricher for event-driven processing.
```

**Replaced By**:
- File discovery: `NotePipeline` with file filters
- Change detection: Merkle tree diff (Phase 3)
- Processing: `NotePipeline::process()`

**Action**: DELETE entire module

### 3. `kiln_pipeline_connector.rs` (757 lines) - **FULLY REDUNDANT**
**Location**: `crates/crucible-surrealdb/src/kiln_pipeline_connector.rs`

**Replaced By**: `crucible-pipeline` orchestrator

**Action**: DELETE entire module

## Disabled Files (12 files)

### Critical - DO NOT DELETE
1. **`semantic.rs.disabled`** (831 lines)
   - **ACTION: RE-ENABLE** as `semantic.rs`
   - Contains full semantic search command implementation
   - Needed for `cru search` command and ACP context enrichment
   - Already production-ready with tests, progress bars, JSON output

### Safe to Delete (11 files)

#### CLI - Old Architecture (7 files)
1. `common/tool_manager.rs.disabled` - Old service locator pattern
2. `common/service_locator.rs.disabled` - Replaced by facade pattern
3. `common/file_scanner.rs.disabled` - Replaced by NotePipeline
4. `common/change_detection_service.rs.disabled` - Replaced by Merkle diff
5. `common/kiln_processor.rs.disabled` - Replaced by NotePipeline
6. `common/app_state.rs.disabled` - Old state management
7. `commands/process.rs.disabled` - Replaced by new process command

#### Other Disabled Files (4 files)
8. `commands/test_tools.rs.disabled` - Unknown purpose
9. `commands/note.rs.disabled` - Old note commands
10. `error_recovery.rs.disabled` - Old error handling
11. `tests/block_storage_integration_tests.rs.disabled` - Old tests

## Duplicate Types Analysis

### In `kiln_integration.rs`:
```rust
pub struct EmbeddingIndexMetadata { ... }  // line 244
pub struct EmbedMetadata { ... }           // line 1247
pub struct LinkRelation { ... }            // line 1257
pub struct EmbedRelation { ... }           // line 1264
```

**Check for duplicates in**:
- `crates/crucible-core/src/enrichment/`
- `crates/crucible-pipeline/`
- `crates/crucible-config/src/enrichment.rs`

## lib.rs Cleanup

**File**: `crates/crucible-surrealdb/src/lib.rs`

**Current Issues**:
- Lines 40-46: Comment says `kiln_scanner` and `kiln_pipeline_connector` are disabled
- Lines 112-125: But then exports them anyway!

**Action**: Clean up after module deletion

## Cleanup Phases

### Phase 1: Analysis & Backup (30 min)
- [x] Identify all redundant code
- [ ] Verify semantic.rs.disabled is safe to re-enable
- [ ] Check for any dependencies on deleted modules
- [ ] Create git branch for cleanup: `cleanup/remove-old-architecture`

### Phase 2: Re-enable Critical Files (1 hour)
- [ ] Re-enable `semantic.rs.disabled` → `semantic.rs`
- [ ] Update imports in CLI
- [ ] Verify semantic search tests pass
- [ ] Test `cru search` command

### Phase 3: Delete Redundant Modules (2 hours)
- [ ] Delete `kiln_scanner.rs` (1,429 lines)
- [ ] Delete `kiln_pipeline_connector.rs` (757 lines)
- [ ] Refactor `kiln_integration.rs`:
  - Extract semantic search functions → `kiln_semantic_search.rs`
  - Extract storage functions → `kiln_storage.rs`
  - Delete old processing functions
  - Reduce from 2,500 lines to ~800 lines
- [ ] Update `lib.rs` exports
- [ ] Fix compilation errors

### Phase 4: Delete Disabled Files (30 min)
- [ ] Delete 11 safe-to-delete `.disabled` files
- [ ] Update imports if needed
- [ ] Verify compilation

### Phase 5: Remove Duplicate Types (1 hour)
- [ ] Search for duplicate type definitions
- [ ] Consolidate into appropriate modules
- [ ] Update all references

### Phase 6: Update OpenSpec (30 min)
- [ ] Update `tasks.md` with cleanup tasks
- [ ] Update `design.md` to note semantic search already exists
- [ ] Document that we're integrating (not building from scratch)

### Phase 7: Testing (1 hour)
- [ ] Run all tests: `cargo test --workspace`
- [ ] Verify pipeline integration tests pass
- [ ] Test semantic search command
- [ ] Test CLI basic operations

### Phase 8: Commit & Push (15 min)
- [ ] Commit cleanup changes
- [ ] Push to branch
- [ ] Update PR description

## Estimated Time Savings

**Lines Deleted**: ~6,000 lines
**Maintenance Burden Reduced**:
- 3 large modules no longer need updates
- 11 disabled files no longer clutter codebase
- Clearer separation between old/new architecture

## Risk Assessment

**Low Risk**:
- Modules marked with TODOs saying they're redundant
- New architecture proven in pipeline integration tests
- Semantic search re-enable is low risk (already tested code)

**Medium Risk**:
- Some parts of `kiln_integration.rs` are still used
- Need careful extraction of semantic search logic

**Mitigation**:
- Work in feature branch
- Test after each phase
- Keep git history for easy rollback

## Dependencies to Check

Before deleting, search for usage:
```bash
# Check kiln_scanner usage
rg "kiln_scanner::" crates/

# Check kiln_pipeline_connector usage
rg "kiln_pipeline_connector::" crates/

# Check kiln_integration usage
rg "kiln_integration::" crates/
```

## OpenSpec Integration

Update `openspec/changes/rework-cli-acp-chat/tasks.md`:

**Add new section**:
```markdown
## 0. Architecture Cleanup (Before Implementation)
- [ ] 0.1 Re-enable semantic.rs.disabled as semantic.rs
- [ ] 0.2 Delete kiln_scanner.rs (1,429 lines - old polling architecture)
- [ ] 0.3 Delete kiln_pipeline_connector.rs (757 lines - old polling architecture)
- [ ] 0.4 Refactor kiln_integration.rs (extract semantic search, delete old processing)
- [ ] 0.5 Delete 11 .disabled files from old architecture
- [ ] 0.6 Remove duplicate types
- [ ] 0.7 Run full test suite to verify cleanup
```

**Update section 4 (Context Enrichment)**:
```markdown
## 4. Context Enrichment
- [ ] 4.1 ~~Create `acp/context.rs` for context assembly~~ ALREADY EXISTS in semantic.rs
- [ ] 4.2 Integrate existing semantic search with CrucibleCore facade
- [ ] 4.3 Add context formatting for agent prompts
- [ ] 4.4 Wire configurable context size into ACP client
- [ ] 4.5 Write integration tests for context enrichment in chat mode
```

## Success Metrics

- [ ] Codebase reduced by ~6,000 lines
- [ ] All tests pass: `cargo test --workspace`
- [ ] `cru search` command works with re-enabled semantic.rs
- [ ] No compilation errors
- [ ] Clear separation: only new pipeline architecture remains
