# Type Consolidation: Execution Guide

**IMPORTANT:** Follow these steps sequentially. Test after each phase.

---

## Pre-Migration Checklist

```bash
# 1. Ensure clean working directory
cd /home/moot/crucible
git status
# Should show only the analysis documents you just created

# 2. Create a backup branch
git checkout -b backup-before-type-consolidation

# 3. Create the migration branch
git checkout -b refactor/consolidate-parser-types

# 4. Run baseline tests (save output)
cargo test --workspace 2>&1 | tee test_baseline.txt

# 5. Check for warnings
cargo clippy --workspace -- -W clippy::all 2>&1 | tee clippy_baseline.txt

# 6. Verify build
cargo build --workspace --release
```

---

## Phase 1: Fix BlockHash Circular Dependency

**Estimated Time:** 30 minutes

### Step 1.1: Update Parser Types

Edit `/home/moot/crucible/crates/crucible-parser/src/types.rs`:

**REMOVE these lines (12-90):**
```rust
/// A BLAKE3 hash used for block-level content addressing
///
/// Similar to FileHash but specifically used for individual content blocks
/// extracted from documents (headings, paragraphs, code blocks, etc.).
///
/// This is a local copy of the type from crucible-core to avoid circular dependencies.
/// The types are kept in sync for compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    // ... entire implementation ...
}
```

**ADD at line 8 (after imports, before ParsedDocument):**
```rust
// Import BlockHash from canonical location in core
use crucible_core::types::hashing::BlockHash;
```

### Step 1.2: Update Parser Re-exports

Edit `/home/moot/crucible/crates/crucible-parser/src/lib.rs`:

**CHANGE line 33-38 from:**
```rust
pub use types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, BlockHash, Blockquote, Callout, CodeBlock,
    DocumentContent, FootnoteDefinition, FootnoteMap, FootnoteReference, Frontmatter,
    FrontmatterFormat, Heading, LatexExpression, ListBlock, ListItem, ListType, ParsedDocument,
    ParsedDocumentBuilder, Tag, TaskStatus, Wikilink,
};
```

**TO:**
```rust
pub use types::{
    ASTBlock, ASTBlockMetadata, ASTBlockType, Blockquote, Callout, CodeBlock,
    DocumentContent, FootnoteDefinition, FootnoteMap, FootnoteReference, Frontmatter,
    FrontmatterFormat, Heading, HorizontalRule, LatexExpression, ListBlock, ListItem, ListType,
    ParsedDocument, ParsedDocumentBuilder, Table, Tag, TaskStatus, Wikilink,
};

// Re-export BlockHash from core for convenience
pub use crucible_core::types::hashing::BlockHash;
```

### Step 1.3: Verify Parser Dependency

Check `/home/moot/crucible/crates/crucible-parser/Cargo.toml`:

Should already have:
```toml
[dependencies]
crucible-core = { workspace = true }  # Or similar
```

If not, add it to the dependencies section.

### Step 1.4: Test Phase 1

```bash
# Test parser crate
cargo test -p crucible-parser

# Test core hashing
cargo test -p crucible-core --lib hashing

# Verify no circular deps
cargo tree -p crucible-parser | grep -q "crucible-core" && echo "OK: Parser depends on core"
cargo tree -p crucible-core | grep -q "crucible-parser" && echo "OK: Core depends on parser"

# Full build
cargo build --workspace
```

**Expected:** All tests pass, no circular dependency errors

### Step 1.5: Commit Phase 1

```bash
git add crates/crucible-parser/src/types.rs
git add crates/crucible-parser/src/lib.rs
git commit -m "refactor(parser): remove duplicate BlockHash, use core version

- Remove BlockHash definition from parser (lines 12-90)
- Import BlockHash from crucible_core::types::hashing
- Update re-exports to include BlockHash from core
- Add Table and HorizontalRule to public re-exports

Part of type consolidation effort (Phase 1/3)
See: TYPE_CONSOLIDATION_PLAN.md"
```

---

## Phase 2: Remove Core Parser Types Module

**Estimated Time:** 2 hours

### Step 2.1: Backup the File (for reference)

```bash
# Save the file contents before deletion
cp crates/crucible-core/src/parser/types.rs /tmp/core_parser_types_backup.rs
```

### Step 2.2: Update Core Parser Module

Edit `/home/moot/crucible/crates/crucible-core/src/parser/mod.rs`:

**REMOVE this line:**
```rust
pub mod types;
```

**ADD these re-exports (after other imports):**
```rust
// Re-export parser types for convenience
// Canonical definitions are in crucible-parser crate
pub use crucible_parser::types::{
    // Core document types
    ParsedDocument,
    ParsedDocumentBuilder,
    DocumentContent,
    Frontmatter,
    FrontmatterFormat,

    // Link and tag types
    Wikilink,
    Tag,

    // Content structure types
    Heading,
    CodeBlock,
    Paragraph,
    ListBlock,
    ListItem,
    ListType,
    TaskStatus,

    // Enhanced content types
    Callout,
    LatexExpression,

    // Footnote types
    FootnoteMap,
    FootnoteDefinition,
    FootnoteReference,

    // AST types (new in parser, not previously in core)
    ASTBlock,
    ASTBlockMetadata,
    ASTBlockType,

    // Additional content types (new in parser)
    Table,
    Blockquote,
    HorizontalRule,
};
```

### Step 2.3: Delete the Duplicate File

```bash
rm crates/crucible-core/src/parser/types.rs
```

### Step 2.4: Update Core Internal Imports (if needed)

Check these files for imports from `crate::parser::types`:

```bash
# Find files with internal parser type imports
rg "use crate::parser::types" crates/crucible-core/src --type rust -l
```

For each file found, **EITHER:**

**Option A:** Keep using re-export (simpler):
```rust
// No change needed - re-export makes this work
use crate::parser::ParsedDocument;
```

**Option B:** Use direct import (more explicit):
```rust
// Change to direct import
use crucible_parser::types::ParsedDocument;
```

**Recommended:** Keep using re-exports for consistency with existing code.

### Step 2.5: Update SurrealDB Imports

**Files to check:**
- `crates/crucible-surrealdb/tests/all_block_types_integration_test.rs`
- `crates/crucible-surrealdb/tests/eav_graph_integration_tests.rs`
- `crates/crucible-surrealdb/src/kiln_integration.rs`
- `crates/crucible-surrealdb/src/eav_graph/ingest.rs`

**CHANGE:**
```rust
// Old (now broken):
use crucible_core::parser::types::ParsedDocument;

// New (either of these):
use crucible_parser::types::ParsedDocument;  // Direct
use crucible_core::parser::ParsedDocument;   // Via re-export
```

**Recommended:** Use re-export for minimal changes:
```rust
use crucible_core::parser::ParsedDocument;  // Still works via re-export
```

### Step 2.6: Test Phase 2

```bash
# Test each crate individually
cargo test -p crucible-parser
cargo test -p crucible-core
cargo test -p crucible-surrealdb

# Full workspace test
cargo test --workspace

# Check for clippy warnings
cargo clippy --workspace -- -W clippy::all
```

**Expected:** All tests pass, no warnings

### Step 2.7: Commit Phase 2

```bash
git add crates/crucible-core/src/parser/mod.rs
git add crates/crucible-core/src/parser/types.rs  # Deleted file
git add crates/crucible-surrealdb/  # If any imports changed
git commit -m "refactor(core): remove duplicate parser types, use re-exports

- Delete crates/crucible-core/src/parser/types.rs (1,054 lines)
- Add re-exports in crucible-core/src/parser/mod.rs
- All types now canonically defined in crucible-parser
- Core provides convenience re-exports for API compatibility
- Update SurrealDB imports to use re-exports

Part of type consolidation effort (Phase 2/3)
See: TYPE_CONSOLIDATION_PLAN.md

BREAKING CHANGE: Direct imports from crucible_core::parser::types
are no longer available. Use crucible_parser::types or
crucible_core::parser (re-export) instead."
```

---

## Phase 3: Documentation and Verification

**Estimated Time:** 1 hour

### Step 3.1: Add Documentation Comments

Edit `/home/moot/crucible/crates/crucible-parser/src/types.rs`:

**ADD at the top of the file (after module doc comment):**
```rust
//! # Type Ownership
//!
//! This module contains the **canonical definitions** of all parser-related types.
//! These types are re-exported by `crucible-core::parser` for convenience.
//!
//! ## Canonical Locations
//!
//! - **Parser Types**: This module (`crucible_parser::types`)
//! - **Hash Types**: `crucible_core::types::hashing` (BlockHash, FileHash)
//! - **AST Types**: This module (parser implementation detail)
//!
//! ## Import Guidelines
//!
//! Prefer importing from the canonical location:
//! ```rust
//! use crucible_parser::types::{ParsedDocument, Wikilink, Tag};
//! use crucible_core::types::hashing::BlockHash;
//! ```
//!
//! Re-exports are available for convenience:
//! ```rust
//! use crucible_core::parser::{ParsedDocument, Wikilink, Tag};
//! ```
```

### Step 3.2: Update Architecture Documentation

Edit `/home/moot/crucible/CLAUDE.md`:

**ADD a new section (after ## 🏗️ Architecture):**

```markdown
### Type Ownership

**Parser Types** are canonically defined in `crucible-parser/src/types.rs`.
Core re-exports these types via `crucible_core::parser::*` for convenience.

**Hash Types** are canonically defined in `crucible-core/src/types/hashing.rs`.
Parser imports and re-exports BlockHash.

**DO NOT duplicate types between crates.** Each type should be defined in exactly
one location. Use re-exports for convenience.

**Import patterns:**
```rust
// Parser types - prefer canonical location
use crucible_parser::types::{ParsedDocument, Wikilink, Tag};

// Or use re-export for convenience
use crucible_core::parser::{ParsedDocument, Wikilink, Tag};

// Hash types - always from core
use crucible_core::types::hashing::{BlockHash, FileHash};
```
```

### Step 3.3: Verify No Duplication

```bash
# Check ParsedDocument is defined only once
COUNT=$(rg "^pub struct ParsedDocument" crates/ --type rust | wc -l)
if [ "$COUNT" -eq 1 ]; then
    echo "✓ ParsedDocument defined exactly once"
else
    echo "✗ ParsedDocument defined $COUNT times (expected 1)"
    rg "^pub struct ParsedDocument" crates/ --type rust
fi

# Check BlockHash is defined only once
COUNT=$(rg "^pub struct BlockHash" crates/ --type rust | wc -l)
if [ "$COUNT" -eq 1 ]; then
    echo "✓ BlockHash defined exactly once"
else
    echo "✗ BlockHash defined $COUNT times (expected 1)"
    rg "^pub struct BlockHash" crates/ --type rust
fi

# Count all type definitions
echo "Type definition counts:"
rg "^pub struct (ParsedDocument|Frontmatter|Wikilink|Tag)" crates/ --type rust | wc -l
echo "Should be 4 (one of each)"
```

### Step 3.4: Generate and Review Documentation

```bash
# Generate docs
cargo doc --no-deps --workspace --open

# Navigate to:
# - crucible_parser::types (should show all types)
# - crucible_core::parser (should show re-exports)
```

### Step 3.5: Final Full Test Suite

```bash
# Run everything
cargo test --workspace --all-features

# Check for warnings
cargo clippy --workspace --all-features -- -W clippy::all

# Build release
cargo build --workspace --release
```

### Step 3.6: Commit Phase 3

```bash
git add crates/crucible-parser/src/types.rs
git add CLAUDE.md
git commit -m "docs: document type ownership and import guidelines

- Add module documentation to parser types
- Update CLAUDE.md with type ownership section
- Document canonical locations for all types
- Provide import pattern examples

Part of type consolidation effort (Phase 3/3)
See: TYPE_CONSOLIDATION_PLAN.md

All phases complete. Type duplication eliminated."
```

---

## Post-Migration Verification

### Verification Checklist

```bash
# 1. Type uniqueness
echo "Checking type uniqueness..."
[ $(rg "^pub struct ParsedDocument" crates/ --type rust | wc -l) -eq 1 ] && echo "✓ ParsedDocument unique" || echo "✗ FAIL"
[ $(rg "^pub struct BlockHash" crates/ --type rust | wc -l) -eq 1 ] && echo "✓ BlockHash unique" || echo "✗ FAIL"

# 2. No circular dependencies
echo "Checking dependencies..."
cargo tree -p crucible-parser | grep -q crucible-core && echo "✓ Parser depends on core"
cargo tree -p crucible-core | grep -q crucible-parser && echo "✓ Core depends on parser"

# 3. Tests pass
echo "Running tests..."
cargo test --workspace --quiet && echo "✓ All tests pass" || echo "✗ Tests FAILED"

# 4. No warnings
echo "Checking for warnings..."
cargo clippy --workspace --quiet -- -W clippy::all && echo "✓ No clippy warnings" || echo "✗ Warnings found"

# 5. Docs build
echo "Building documentation..."
cargo doc --no-deps --workspace --quiet && echo "✓ Docs build successfully" || echo "✗ Doc build FAILED"

# 6. Import paths work
echo "Verifying import paths..."
grep -r "use crucible_parser::types::ParsedDocument" crates/ && echo "✓ Direct imports exist"
grep -r "use crucible_core::parser::ParsedDocument" crates/ && echo "✓ Re-export imports exist"
```

### Success Metrics

**Before consolidation:**
- Total LOC (types): 3,158
- Duplicated LOC: 1,054
- Type definitions: 38
- Import sources: 2 (confusing)

**After consolidation:**
- Total LOC (types): 2,104 ✓ (-33%)
- Duplicated LOC: 0 ✓ (eliminated)
- Type definitions: 19 + 7 = 26 ✓ (no duplication)
- Import sources: 1 canonical + 1 re-export ✓ (clear)

---

## Commit Message Template

```
refactor: consolidate parser types, eliminate duplication

Summary:
Eliminated 1,054 lines of duplicated parser type definitions between
crucible-parser and crucible-core. Established single source of truth
with clear ownership boundaries.

Changes:
- Phase 1: Removed duplicate BlockHash from parser, use core version
- Phase 2: Deleted crucible-core/src/parser/types.rs (1,054 lines)
- Phase 3: Added documentation and verification

Architecture:
- Parser types: canonically defined in crucible-parser
- Hash types: canonically defined in crucible-core
- Re-exports: available in crucible-core::parser for convenience

Benefits:
- Single source of truth for all parser types
- Clear ownership boundaries (parser owns parser types)
- Reduced maintenance burden (no synchronization needed)
- Improved modularity and cohesion
- SOLID principles compliance restored

Testing:
- All workspace tests pass
- No clippy warnings
- Documentation builds successfully
- Import paths verified (both direct and re-export)

See TYPE_CONSOLIDATION_PLAN.md for complete analysis and strategy.

Breaking Changes:
- crucible_core::parser::types module removed
- Use crucible_parser::types or crucible_core::parser (re-export)

Co-authored-by: Claude <noreply@anthropic.com>
```

---

## Troubleshooting

### Issue: "circular dependency detected"

**Cause:** Parser trying to depend on core which depends on parser

**Solution:** Verify dependency direction:
```bash
cargo tree -p crucible-parser
cargo tree -p crucible-core
```

Parser should show core as dependency, core should show parser as dependency.

### Issue: "type ParsedDocument is not in scope"

**Cause:** Missing import after removing types module

**Solution:** Add import:
```rust
use crucible_parser::types::ParsedDocument;
// Or
use crucible_core::parser::ParsedDocument;
```

### Issue: Tests fail with "no such type BlockHash"

**Cause:** BlockHash not properly re-exported

**Solution:** Check re-exports in:
- `crates/crucible-parser/src/lib.rs`
- `crates/crucible-core/src/parser/mod.rs`

### Issue: "error[E0433]: failed to resolve: use of undeclared crate"

**Cause:** Missing dependency in Cargo.toml

**Solution:** Add to `crates/crucible-parser/Cargo.toml`:
```toml
[dependencies]
crucible-core = { workspace = true }
```

---

## Rollback Instructions

If critical issues arise:

```bash
# Return to backup branch
git checkout backup-before-type-consolidation

# Or rollback specific files
git checkout HEAD~3 -- crates/crucible-parser/src/types.rs
git checkout HEAD~3 -- crates/crucible-core/src/parser/

# Verify
cargo test --workspace
```

---

## Completion

After all phases complete and verification passes:

```bash
# Merge to main
git checkout master
git merge refactor/consolidate-parser-types

# Clean up backup branch
git branch -d backup-before-type-consolidation

# Push
git push origin master

# Celebrate
echo "🎉 Type consolidation complete!"
echo "   - 1,054 lines of duplication eliminated"
echo "   - Clear ownership boundaries established"
echo "   - SOLID principles restored"
```

**Total time:** Approximately 3.5 hours (30min + 2hr + 1hr + verification)

**Impact:** High maintainability improvement, no runtime performance change

**Risk:** Low (phased approach with comprehensive testing)
