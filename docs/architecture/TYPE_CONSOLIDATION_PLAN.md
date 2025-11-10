# Parser Type Duplication Consolidation Plan

## Executive Summary

Massive type duplication exists between `crucible-parser` and `crucible-core/parser` modules. This document provides a comprehensive analysis and phased migration strategy to eliminate the duplication while maintaining architectural integrity.

**Key Metrics:**
- Parser types file: 2,104 lines
- Core types file: 1,054 lines
- Duplication: ~1,050 lines (50% redundancy)
- Affected files: 20+ across 3 crates
- Estimated effort: 8-12 hours (3 phases)

---

## 1. SCOPE: Type Catalog

### 1.1 Duplicated Types (Exist in BOTH locations)

These types are **identical or nearly identical** between parser and core:

| Type | Parser LOC | Core LOC | Divergence | Notes |
|------|-----------|----------|------------|-------|
| `ParsedDocument` | Lines 110-287 | Lines 27-155 | **MAJOR** | Parser has `block_hashes`, `merkle_root` fields |
| `Frontmatter` | Lines 289-406 | Lines 157-233 | **MINOR** | Parser has `get_date()`, `get_object()` methods |
| `FrontmatterFormat` | Lines 408-417 | Lines 235-244 | NONE | Identical enum |
| `Wikilink` | Lines 419-515 | Lines 246-342 | NONE | Identical implementation |
| `Tag` | Lines 517-563 | Lines 344-390 | NONE | Identical implementation |
| `DocumentContent` | Lines 565-662 | Lines 392-473 | **MINOR** | Parser has `blockquotes`, `footnotes`, `tables`, `horizontal_rules` fields |
| `Heading` | Lines 664-704 | Lines 475-515 | NONE | Identical implementation |
| `CodeBlock` | Lines 706-738 | Lines 517-549 | NONE | Identical implementation |
| `Paragraph` | Lines 740-763 | Lines 551-574 | NONE | Identical implementation |
| `ListBlock` | Lines 765-797 | Lines 576-608 | NONE | Identical implementation |
| `ListType` | Lines 799-806 | Lines 610-617 | NONE | Identical enum |
| `ListItem` | Lines 808-843 | Lines 619-654 | NONE | Identical implementation |
| `TaskStatus` | Lines 845-852 | Lines 656-663 | NONE | Identical enum |
| `Callout` | Lines 1331-1417 | Lines 665-735 | **MINOR** | Parser has `start_offset()`, `length()` methods |
| `LatexExpression` | Lines 1419-1459 | Lines 737-772 | **MINOR** | Parser has `start_offset()` method |
| `FootnoteMap` | Lines 1461-1507 | Lines 774-820 | NONE | Identical implementation |
| `FootnoteDefinition` | Lines 1509-1535 | Lines 822-848 | NONE | Identical implementation |
| `FootnoteReference` | Lines 1537-1568 | Lines 850-881 | NONE | Identical implementation |
| `ParsedDocumentBuilder` | Lines 1570-1700 | Lines 883-977 | **MINOR** | Parser has `block_hashes`, `merkle_root` builder methods |

**Total Duplicated Types: 19**

### 1.2 Parser-Only Types (NOT in core)

These types exist **ONLY** in `crucible-parser/src/types.rs`:

| Type | Lines | Purpose | Used By |
|------|-------|---------|---------|
| `BlockHash` | 12-90 | BLAKE3 hash for block-level content addressing | Parser, Core (imported), SurrealDB |
| `Table` | 854-880 | Markdown table representation | Parser tests, content extraction |
| `Blockquote` | 882-911 | Regular blockquote (not callout) | Parser tests, content extraction |
| `HorizontalRule` | 913-953 | Horizontal rule / thematic break | Parser tests, content extraction |
| `ASTBlockType` | 955-1002 | Enum of semantic block types | AST conversion, block extraction |
| `ASTBlock` | 1010-1221 | Semantic unit with hash and metadata | AST conversion, block storage |
| `ASTBlockMetadata` | 1223-1329 | Block-specific metadata | AST blocks, content analysis |

**Total Parser-Only Types: 7**

### 1.3 Core-Only Types

**NONE** - All types in `crucible-core/src/parser/types.rs` are duplicates of parser types.

### 1.4 Implementation Method Divergence

**Frontmatter additional methods in parser:**
- `get_date(&self, key: &str) -> Option<NaiveDate>` - Parse date from frontmatter (ISO 8601, RFC 3339, YYYYMMDD)
- `get_object(&self, key: &str) -> Option<serde_json::Map<...>>` - Get nested object property

**Callout additional methods in parser:**
- `start_offset(&self) -> usize` - Backward compatibility accessor
- `length(&self) -> usize` - Calculate total callout length including header

**LatexExpression additional methods in parser:**
- `start_offset(&self) -> usize` - Backward compatibility accessor

**ParsedDocument major divergence:**
- Parser has `block_hashes: Vec<BlockHash>` field for Phase 2 block-level change detection
- Parser has `merkle_root: Option<BlockHash>` field for Phase 2 Merkle tree support
- Parser has methods: `has_block_hashes()`, `block_hash_count()`, `has_merkle_root()`, `get_merkle_root()`, `with_block_hashes()`, `with_merkle_root()`, `add_block_hash()`, `clear_hash_data()`

**DocumentContent minor divergence:**
- Parser has `blockquotes: Vec<Blockquote>` field
- Parser has `footnotes: FootnoteMap` field
- Parser has `tables: Vec<Table>` field
- Parser has `horizontal_rules: Vec<HorizontalRule>` field

---

## 2. DEPENDENCY ANALYSIS

### 2.1 Import Patterns

**Files importing from `crucible_parser::types::`** (9 files)
```
crates/crucible-parser/tests/horizontal_rule_tests.rs
crates/crucible-parser/tests/table_tests.rs
crates/crucible-parser/tests/heading_hierarchy.rs
crates/crucible-parser/tests/frontmatter_types.rs
crates/crucible-surrealdb/src/content_addressed_storage.rs
crates/crucible-core/examples/ast_converter_demo.rs
crates/crucible-surrealdb/src/block_storage.rs
crates/crucible-core/src/hashing/block_hasher.rs
crates/crucible-core/src/hashing/ast_converter.rs
```

**Files importing from `crucible_core::parser::types::`** (4 files)
```
crates/crucible-surrealdb/tests/all_block_types_integration_test.rs
crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs
crates/crucible-surrealdb/src/kiln_integration.rs
crates/crucible-surrealdb/src/eav_graph/ingest.rs
```

**Internal imports within crucible-core** (7 files)
```
crates/crucible-core/src/merkle/hybrid.rs
crates/crucible-core/src/processing/mod.rs
crates/crucible-core/src/types/mod.rs
crates/crucible-core/src/traits/parser.rs
crates/crucible-core/src/parser/storage_bridge.rs
crates/crucible-core/src/parser/bridge.rs
crates/crucible-core/src/parser/adapter.rs
```

### 2.2 Dependency Graph

```
crucible-parser (standalone)
    └─> crucible-parser::types (CANONICAL for parser types)
        └─> Contains: BlockHash (duplicate of core), all parser types

crucible-core
    ├─> crucible-parser (dependency)
    │   └─> Uses parser types via re-export
    ├─> crucible-core::types::hashing (CANONICAL for BlockHash)
    │   └─> Contains: BlockHash, FileHash, HashAlgorithm
    └─> crucible-core::parser::types (DUPLICATE MODULE - TO REMOVE)
        └─> Contains: Duplicates of parser types (missing AST types)

crucible-surrealdb
    ├─> crucible-parser::types (9 files)
    └─> crucible-core::parser::types (4 files)
```

### 2.3 Critical Finding: BlockHash Triple Definition

**BlockHash is defined in THREE places:**

1. `/home/moot/crucible/crates/crucible-parser/src/types.rs:20` (local copy)
   - Comment: "This is a local copy of the type from crucible-core to avoid circular dependencies"
   - Returns `Result<Self, String>` for `from_hex()`

2. `/home/moot/crucible/crates/crucible-core/src/types/hashing.rs:82` (canonical)
   - More robust with `HashError` type
   - Returns `Result<Self, HashError>` for `from_hex()`
   - Has `Default` impl via `zero()`

3. Both are essentially the same but parser version predates proper error handling

**This is the root cause of the duplication issue.**

---

## 3. ARCHITECTURAL ANALYSIS

### 3.1 Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      crucible-cli                            │
│                   (Application Layer)                        │
└───────────────────────────────┬─────────────────────────────┘
                                │
                    ┌───────────▼──────────────┐
                    │   crucible-surrealdb     │
                    │   (Storage Layer)        │
                    └───┬────────────────┬─────┘
                        │                │
          ┌─────────────▼──┐      ┌─────▼──────────────┐
          │ crucible-core  │      │  crucible-parser   │
          │ (Business)     │◄─────┤  (Parsing)         │
          │                │      │                    │
          │ parser::types  │      │  types.rs          │
          │ (DUPLICATE)    │      │  (CANONICAL)       │
          └────────────────┘      └────────────────────┘
                  ▲                        ▲
                  │                        │
                  └────── DUPLICATION ─────┘
```

### 3.2 Intended Architecture (Post-Consolidation)

```
┌─────────────────────────────────────────────────────────────┐
│                      crucible-cli                            │
│                   (Application Layer)                        │
└───────────────────────────────┬─────────────────────────────┘
                                │
                    ┌───────────▼──────────────┐
                    │   crucible-surrealdb     │
                    │   (Storage Layer)        │
                    └───┬────────────────┬─────┘
                        │                │
          ┌─────────────▼──┐      ┌─────▼──────────────────┐
          │ crucible-core  │      │  crucible-parser       │
          │ (Business)     │◄─────┤  (Parsing)             │
          │                │      │                        │
          │ Re-exports     │      │  types.rs              │
          │ parser types   │      │  (SINGLE SOURCE)       │
          │                │      │                        │
          │ types/hashing  │      │  Re-export BlockHash   │
          │ (BlockHash)    │─────►│  from core             │
          └────────────────┘      └────────────────────────┘
                                           │
                                  SINGLE SOURCE OF TRUTH
```

### 3.3 Dependency Inversion Compliance

Current setup **violates** dependency inversion:
- Core defines storage traits ✓
- Core should NOT duplicate parser types ✗
- Parser should be the canonical source for parser types ✓
- Core should re-export parser types for convenience ✓

**The duplication in `crucible-core/src/parser/types.rs` was a mistake** that needs to be removed.

---

## 4. CONSOLIDATION STRATEGY

### 4.1 Guiding Principles

1. **Single Source of Truth**: Each type defined in exactly ONE location
2. **Minimize Breaking Changes**: Use re-exports to maintain API compatibility
3. **Preserve Phase 2 Features**: Keep `block_hashes` and `merkle_root` fields in parser
4. **Correct Circular Dependencies**: BlockHash should come from `crucible-core::types::hashing`
5. **Test-Driven Migration**: Verify each phase with full test suite

### 4.2 Canonical Type Locations

| Type Category | Canonical Location | Rationale |
|---------------|-------------------|-----------|
| **Parser Types** | `crucible-parser::types` | Parser owns document structure types |
| **Hash Types** | `crucible-core::types::hashing` | Core owns hashing infrastructure |
| **AST Types** | `crucible-parser::types` | Parser-specific implementation detail |
| **Enhanced Types** | `crucible-parser::types` | Parser owns the Phase 2 enhancements |

### 4.3 Migration Direction

```
MOVE: BlockHash definition
  FROM: crucible-parser/src/types.rs (duplicate)
  TO:   crucible-core/src/types/hashing.rs (already exists)
  ACTION: Remove from parser, import from core

KEEP: All other types
  FROM: crucible-parser/src/types.rs (canonical)
  ACTION: None (already correct)

REMOVE: Duplicate module
  FROM: crucible-core/src/parser/types.rs (entire file)
  TO:   Re-export from crucible-parser in crucible-core/src/parser/mod.rs
  ACTION: Delete file, add re-exports
```

---

## 5. PHASED IMPLEMENTATION PLAN

### Phase 1: Fix BlockHash Circular Dependency (Low Risk)

**Objective**: Remove duplicate BlockHash from parser, use core version

**Changes:**
1. Update `crucible-parser/src/types.rs`:
   - Remove `BlockHash` struct definition (lines 12-90)
   - Add import: `use crucible_core::types::hashing::BlockHash;`
   - Update re-export in parser lib.rs

2. Update error handling in parser to match core's `HashError`:
   - Change `from_hex()` return type from `Result<Self, String>` to `Result<Self, HashError>`
   - Add `use crucible_core::types::HashError;`

3. Verify no circular dependency:
   - Parser already depends on core ✓
   - Core types don't depend on parser ✓

**Files Modified:**
- `crates/crucible-parser/src/types.rs` (remove BlockHash, add import)
- `crates/crucible-parser/src/lib.rs` (update re-export)
- `crates/crucible-parser/Cargo.toml` (verify crucible-core dependency)

**Tests to Run:**
```bash
cargo test -p crucible-parser
cargo test -p crucible-core --test block_hasher
```

**Estimated Time**: 30 minutes

**Risk Level**: LOW
- BlockHash implementations are functionally identical
- Only error type changes (String -> HashError)
- Breaking change is internal to parser crate

---

### Phase 2: Remove Core Parser Types Module (Medium Risk)

**Objective**: Delete `crucible-core/src/parser/types.rs`, re-export from parser

**Changes:**

1. **Delete duplicate file:**
   ```bash
   rm crates/crucible-core/src/parser/types.rs
   ```

2. **Update `crucible-core/src/parser/mod.rs`:**
   ```rust
   // OLD: pub mod types;

   // NEW: Re-export parser types for convenience
   pub use crucible_parser::types::{
       Callout, CodeBlock, DocumentContent, FootnoteDefinition,
       FootnoteMap, FootnoteReference, Frontmatter, FrontmatterFormat,
       Heading, LatexExpression, ListBlock, ListItem, ListType,
       ParsedDocument, ParsedDocumentBuilder, Tag, TaskStatus, Wikilink,

       // AST types (not previously available in core)
       ASTBlock, ASTBlockMetadata, ASTBlockType,
       Table, Blockquote, HorizontalRule,
   };
   ```

3. **Update internal core imports:**

   Change from: `use crate::parser::types::*;`
   To: `use crucible_parser::types::*;`

   Or keep using: `use crate::parser::{ParsedDocument, ...};` (still works via re-export)

   **Files to update:**
   - `crates/crucible-core/src/merkle/hybrid.rs`
   - `crates/crucible-core/src/processing/mod.rs`
   - `crates/crucible-core/src/types/mod.rs`
   - `crates/crucible-core/src/traits/parser.rs`
   - `crates/crucible-core/src/parser/storage_bridge.rs`
   - `crates/crucible-core/src/parser/bridge.rs`
   - `crates/crucible-core/src/parser/adapter.rs`

4. **Update SurrealDB imports:**

   Standardize to use parser directly:
   ```rust
   // Change from:
   use crucible_core::parser::types::ParsedDocument;

   // To:
   use crucible_parser::types::ParsedDocument;

   // OR keep using:
   use crucible_core::parser::ParsedDocument;  // Still works via re-export
   ```

   **Files to update:**
   - `crates/crucible-surrealdb/tests/all_block_types_integration_test.rs`
   - `crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs`
   - `crates/crucible-surrealdb/src/kiln_integration.rs`
   - `crates/crucible-surrealdb/src/eav_graph/ingest.rs`

**Tests to Run:**
```bash
cargo test -p crucible-core
cargo test -p crucible-surrealdb
cargo test --workspace
```

**Estimated Time**: 2 hours

**Risk Level**: MEDIUM
- Many files import from the removed module
- Re-exports maintain API compatibility
- SurrealDB files need manual inspection
- Integration tests will catch issues

---

### Phase 3: Consolidate Enhanced Features (Low Risk)

**Objective**: Ensure parser types have all enhancements, document differences

**Changes:**

1. **Verify parser has all features from core:**
   - ✓ `ParsedDocument` has `block_hashes` and `merkle_root` (already in parser)
   - ✓ `DocumentContent` has `blockquotes`, `footnotes`, `tables`, `horizontal_rules` (already in parser)
   - ✓ `Frontmatter` has `get_date()` and `get_object()` (already in parser)
   - ✓ AST types exist in parser (not in core)

2. **Add missing convenience methods if needed:**
   - Consider if `Callout::start_offset()` and `length()` should be in parser (already there)
   - Consider if `LatexExpression::start_offset()` should be in parser (already there)

3. **Update documentation:**
   - Add doc comments explaining canonical locations
   - Update architecture docs to reflect single source of truth
   - Document the Phase 2 fields in `ParsedDocument`

4. **Update `CLAUDE.md` / `AGENTS.md`:**
   ```markdown
   ## Parser Types

   **Canonical Location**: `crucible-parser/src/types.rs`

   All document parsing types are defined in the parser crate. Core re-exports
   these types for convenience via `crucible_core::parser::*`.

   **Hash Types**: Use `crucible_core::types::hashing::{BlockHash, FileHash}`

   DO NOT duplicate types between parser and core.
   ```

**Files Modified:**
- `crates/crucible-parser/src/types.rs` (add doc comments)
- `CLAUDE.md` or `openspec/specs/architecture.md`
- Parser type documentation

**Tests to Run:**
```bash
cargo doc --no-deps --open
cargo test --workspace
```

**Estimated Time**: 1 hour

**Risk Level**: LOW
- Documentation and verification only
- No code changes needed
- Confirms all features are preserved

---

## 6. RISK ASSESSMENT

### 6.1 Breaking Changes

| Change | Impact | Mitigation |
|--------|--------|------------|
| BlockHash error type | Parser internal | Update error handling in parser |
| Remove core types module | Medium | Re-exports maintain API compatibility |
| Import path changes | Low | Both old and new paths work via re-export |

### 6.2 Potential Issues

**Issue 1: Circular Dependency**
- **Risk**: BlockHash move could create circular dependency
- **Mitigation**: Parser already depends on core, no new dependency
- **Verification**: `cargo build --workspace` will fail if circular

**Issue 2: Missing Types**
- **Risk**: Core uses AST types that weren't in its types module
- **Mitigation**: Add AST types to re-exports in Phase 2
- **Verification**: Compile errors will identify missing types

**Issue 3: Version Skew**
- **Risk**: Parser and core versions of types diverged
- **Mitigation**: Diff analysis shows only additive changes in parser
- **Verification**: Full test suite run after each phase

**Issue 4: Serialization Compatibility**
- **Risk**: Removing fields could break deserialization
- **Mitigation**: Parser version has superset of fields (backward compatible)
- **Verification**: Integration tests with actual SurrealDB storage

### 6.3 Rollback Strategy

Each phase is independent and reversible:

1. **Phase 1 Rollback**: Revert BlockHash import, restore local definition
2. **Phase 2 Rollback**: Restore `crucible-core/src/parser/types.rs` from git
3. **Phase 3 Rollback**: No code changes, just documentation

**Rollback Procedure:**
```bash
git checkout HEAD -- crates/crucible-parser/src/types.rs
git checkout HEAD -- crates/crucible-core/src/parser/types.rs
git checkout HEAD -- crates/crucible-core/src/parser/mod.rs
cargo test --workspace
```

---

## 7. TESTING STRATEGY

### 7.1 Pre-Migration Tests

**Establish Baseline:**
```bash
# Run full test suite before any changes
cargo test --workspace > test_baseline.txt 2>&1

# Check for warnings
cargo clippy --workspace -- -W clippy::all > clippy_baseline.txt 2>&1

# Verify builds
cargo build --workspace --release
```

### 7.2 Per-Phase Testing

**After Phase 1:**
```bash
cargo test -p crucible-parser --lib -- --test-threads=1
cargo test -p crucible-core --test block_hasher
cargo build --workspace  # Verify no circular deps
```

**After Phase 2:**
```bash
cargo test -p crucible-core
cargo test -p crucible-surrealdb
cargo test --workspace --exclude crucible-cli  # Faster iteration
```

**After Phase 3:**
```bash
cargo test --workspace
cargo clippy --workspace -- -W clippy::all
cargo doc --no-deps --workspace
```

### 7.3 Integration Testing

**Critical Test Cases:**

1. **Parser -> Core -> SurrealDB flow:**
   ```bash
   cargo test -p crucible-surrealdb eav_graph_integration
   ```

2. **BlockHash serialization round-trip:**
   ```bash
   cargo test -p crucible-core block_hash_roundtrip
   cargo test -p crucible-parser block_hash
   ```

3. **ParsedDocument with Phase 2 fields:**
   ```bash
   cargo test -p crucible-parser parsed_document_with_block_hashes
   ```

4. **Type re-export verification:**
   ```bash
   # Verify these imports all work:
   use crucible_parser::types::ParsedDocument;
   use crucible_core::parser::ParsedDocument;  # Via re-export
   ```

### 7.4 Regression Testing

**High-Risk Areas:**

1. **EAV Graph Ingestion** (uses core types):
   - Test file: `crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs`
   - Verify: Document ingestion, relation extraction, tag storage

2. **AST Conversion** (uses parser-only types):
   - Test file: `crates/crucible-core/src/hashing/ast_converter.rs`
   - Verify: AST block creation, metadata handling

3. **Block Storage** (uses BlockHash):
   - Test file: `crates/crucible-surrealdb/src/block_storage.rs`
   - Verify: Block hashing, storage, retrieval

---

## 8. ESTIMATED EFFORT

| Phase | Duration | Complexity | Risk |
|-------|----------|------------|------|
| **Phase 1: BlockHash** | 30 min | Low | Low |
| **Phase 2: Remove Dups** | 2 hours | Medium | Medium |
| **Phase 3: Documentation** | 1 hour | Low | Low |
| **Testing** | 2 hours | Medium | - |
| **Verification** | 1 hour | Low | - |
| **Buffer** | 2 hours | - | - |
| **TOTAL** | **8-12 hours** | - | - |

**Assumptions:**
- Developer familiar with Rust module system
- Test suite passes before starting
- No major merge conflicts
- Serial execution (one phase at a time)

**Parallel Execution:**
If multiple developers work in parallel, total time can be reduced to **4-6 hours**.

---

## 9. SUCCESS CRITERIA

### 9.1 Functional Requirements

✅ **All tests pass** across all workspace crates
✅ **No circular dependencies** detected by cargo
✅ **API compatibility maintained** for downstream consumers
✅ **Phase 2 features preserved** (block_hashes, merkle_root)
✅ **AST types available** via re-exports

### 9.2 Quality Requirements

✅ **Zero duplication** - Each type defined once
✅ **Clear ownership** - Documented canonical locations
✅ **Consistent imports** - Standardized import patterns
✅ **Updated docs** - Architecture reflects single source of truth
✅ **Clean compile** - No warnings or clippy issues

### 9.3 Performance Requirements

✅ **No performance degradation** - Re-exports have zero runtime cost
✅ **Build time unchanged** - No additional compilation overhead
✅ **Binary size unchanged** - No code duplication in artifacts

---

## 10. POST-CONSOLIDATION VERIFICATION

### 10.1 Type Uniqueness Check

After all phases complete, verify no duplication:

```bash
# Search for duplicate type definitions
rg "^pub struct ParsedDocument" crates/ --type rust
# Should return exactly 1 match in crucible-parser

rg "^pub struct BlockHash" crates/ --type rust
# Should return exactly 1 match in crucible-core/types/hashing.rs
```

### 10.2 Import Pattern Audit

Verify consistent import patterns:

```bash
# Find all ParsedDocument imports
rg "use.*ParsedDocument" crates/ --type rust | sort | uniq

# Should see mix of:
# use crucible_parser::types::ParsedDocument;
# use crucible_core::parser::ParsedDocument;  # (via re-export)
```

### 10.3 Dependency Graph Validation

```bash
# Verify no circular dependencies
cargo tree -p crucible-parser | grep crucible-core
# Should show crucible-core as dependency (not dependent)

cargo tree -p crucible-core | grep crucible-parser
# Should show crucible-parser as dependency
```

### 10.4 Documentation Completeness

```bash
# Generate docs and verify cross-linking
cargo doc --no-deps --workspace --open

# Verify re-exports are documented
# Navigate to crucible_core::parser and verify types are listed
```

---

## 11. FUTURE MAINTENANCE

### 11.1 Type Addition Guidelines

**When adding new parser types:**

1. Define in `crucible-parser/src/types.rs` (canonical)
2. Add to public re-export in `crucible-parser/src/lib.rs`
3. If needed in core, add to re-export in `crucible-core/src/parser/mod.rs`
4. Update documentation

**Never:**
- Duplicate types between crates
- Define parser types in core
- Create parallel type hierarchies

### 11.2 Monitoring for Drift

Add to CI pipeline:

```bash
# Fail build if duplicate type definitions detected
scripts/check_type_duplication.sh
```

Script contents:
```bash
#!/bin/bash
# Check for duplicate parser type definitions

PARSER_TYPES="ParsedDocument|Frontmatter|Wikilink|Tag|DocumentContent"

MATCHES=$(rg "^pub struct ($PARSER_TYPES)" crates/ --type rust | wc -l)

if [ "$MATCHES" -gt 5 ]; then
  echo "ERROR: Duplicate parser types detected"
  rg "^pub struct ($PARSER_TYPES)" crates/ --type rust
  exit 1
fi
```

### 11.3 Version Compatibility

**Parser types are versioned with the parser crate:**
- Changes to parser types require parser version bump
- Core re-exports track parser version
- Breaking changes follow semver

**Hash types are versioned with the core crate:**
- Changes to BlockHash/FileHash require core version bump
- Parser imports from core types, tracks core version

---

## 12. OPEN QUESTIONS

1. **Should we add `get_date()` and `get_object()` to core's Frontmatter?**
   - Current: Only in parser version
   - Impact: Low - can add later if needed
   - Decision: Keep in parser for now

2. **Should AST types be public API or internal to parser?**
   - Current: Public in parser, not in core
   - Impact: Affects API surface area
   - Decision: Make public via re-export for flexibility

3. **Should we version the type schemas separately?**
   - Current: Types version with their crate
   - Impact: Coordination between parser and core versions
   - Decision: Current approach is sufficient

---

## 13. APPENDICES

### Appendix A: File Line Count Analysis

```
Parser types file:      2,104 lines
Core types file:        1,054 lines
Overlap (estimated):    ~1,050 lines (50% duplication)

Parser-only content:
- BlockHash: 78 lines (to be removed)
- Table: 27 lines
- Blockquote: 30 lines
- HorizontalRule: 41 lines
- ASTBlockType: 48 lines
- ASTBlock: 212 lines
- ASTBlockMetadata: 107 lines
- Enhanced methods: ~100 lines
- Total: ~640 lines unique to parser

After consolidation:
- Parser types: ~2,104 lines (canonical)
- Core types: 0 lines (removed, re-exported)
- Reduction: 1,054 lines deleted
```

### Appendix B: Import Statement Templates

**For new code using parser types:**
```rust
// Preferred: Import from canonical location
use crucible_parser::types::{ParsedDocument, Wikilink, Tag};

// Acceptable: Import from core re-export (for convenience)
use crucible_core::parser::{ParsedDocument, Wikilink, Tag};
```

**For new code using hash types:**
```rust
// Always use core for hash types
use crucible_core::types::hashing::{BlockHash, FileHash, HashAlgorithm};
```

### Appendix C: Related Issues

- **BlockHash Triple Definition**: Parser has local copy, core has canonical
- **Missing AST Types in Core**: Core never had Table, Blockquote, HorizontalRule, AST*
- **Phase 2 Fields**: Parser has `block_hashes` and `merkle_root`, core doesn't
- **Enhanced Methods**: Parser has additional Frontmatter methods

All resolved by making parser canonical and removing core duplicates.

---

## CONCLUSION

This consolidation eliminates 1,054 lines of duplicated code, establishes clear ownership boundaries, and maintains full backward compatibility through re-exports. The phased approach minimizes risk while ensuring all enhancements and features are preserved in the canonical location.

**Recommended Action**: Proceed with Phase 1 immediately to fix the BlockHash circular dependency issue.
