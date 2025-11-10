# Type Consolidation Quick Reference

## At a Glance

**Problem:** 1,054 lines of duplicated parser types between `crucible-parser` and `crucible-core/parser`

**Solution:** 3-phase migration to establish single source of truth

**Timeline:** 8-12 hours total effort

**Risk:** LOW (phased approach with comprehensive testing)

---

## Key Findings

### 📊 Duplication Metrics

- **19 types duplicated** across both crates
- **7 types unique** to parser (AST types, Table, Blockquote, etc.)
- **0 types unique** to core (all are duplicates)
- **Triple definition** of BlockHash (parser, core hashing, local copy)

### 🎯 Impact Areas

**Files Affected:**
- 9 files import from `crucible_parser::types`
- 4 files import from `crucible_core::parser::types`
- 7 files use internal core imports

**Critical Dependencies:**
- SurrealDB uses BOTH import paths (inconsistent)
- Core examples use parser types
- Tests scattered across both crates

---

## Migration Phases

### Phase 1: Fix BlockHash (30 min, LOW RISK)

**Action:** Remove duplicate BlockHash from parser, use core version

```rust
// REMOVE from parser/src/types.rs
pub struct BlockHash([u8; 32]);

// ADD to parser/src/types.rs
use crucible_core::types::hashing::BlockHash;
```

**Impact:** Internal parser change only

---

### Phase 2: Remove Core Duplicates (2 hrs, MEDIUM RISK)

**Action:** Delete `crucible-core/src/parser/types.rs`, add re-exports

```rust
// DELETE entire file
rm crates/crucible-core/src/parser/types.rs

// ADD to crucible-core/src/parser/mod.rs
pub use crucible_parser::types::{
    ParsedDocument, Frontmatter, Wikilink, Tag,
    // ... all 19 types + 7 parser-only types
};
```

**Impact:**
- 4 SurrealDB files need import updates
- 7 core internal files continue working via re-exports

---

### Phase 3: Documentation (1 hr, LOW RISK)

**Action:** Document canonical locations, update architecture guides

**Deliverables:**
- Updated `CLAUDE.md` with type ownership rules
- Architecture documentation reflecting single source of truth
- CI checks to prevent future duplication

---

## Canonical Locations (Post-Migration)

| Type Category | Location | Rationale |
|---------------|----------|-----------|
| Parser types | `crucible-parser::types` | Parser owns document structure |
| Hash types | `crucible-core::types::hashing` | Core owns infrastructure |
| AST types | `crucible-parser::types` | Parser implementation detail |

---

## Testing Checklist

### Before Migration
- [ ] Run full test suite: `cargo test --workspace`
- [ ] Save baseline: `cargo test --workspace > baseline.txt`
- [ ] Check clippy: `cargo clippy --workspace`

### After Each Phase
- [ ] Parser tests: `cargo test -p crucible-parser`
- [ ] Core tests: `cargo test -p crucible-core`
- [ ] SurrealDB tests: `cargo test -p crucible-surrealdb`
- [ ] Full workspace: `cargo test --workspace`
- [ ] No warnings: `cargo clippy --workspace`

### Final Verification
- [ ] No duplication: `rg "^pub struct ParsedDocument" crates/`
- [ ] Single BlockHash: `rg "^pub struct BlockHash" crates/`
- [ ] Imports work: Test both parser and core import paths
- [ ] Docs build: `cargo doc --no-deps --workspace`

---

## Quick Commands

### Find Duplicates
```bash
# List all ParsedDocument definitions
rg "^pub struct ParsedDocument" crates/ --type rust

# Should return exactly 1 match after consolidation
```

### Check Dependencies
```bash
# No circular deps
cargo tree -p crucible-parser | grep crucible-core
cargo tree -p crucible-core | grep crucible-parser
```

### Run Tests
```bash
# Full suite
cargo test --workspace

# Per-phase
cargo test -p crucible-parser  # Phase 1
cargo test -p crucible-core    # Phase 2
cargo test --workspace         # Phase 3
```

---

## Rollback Procedure

If issues arise, rollback is simple:

```bash
# Restore original files
git checkout HEAD -- crates/crucible-parser/src/types.rs
git checkout HEAD -- crates/crucible-core/src/parser/types.rs
git checkout HEAD -- crates/crucible-core/src/parser/mod.rs

# Verify
cargo test --workspace
```

---

## Success Criteria

✅ All 19 types defined in EXACTLY ONE location (parser)
✅ BlockHash defined in EXACTLY ONE location (core)
✅ All tests pass across all crates
✅ No clippy warnings
✅ Documentation updated
✅ 1,054 lines of duplicated code removed

---

## Next Steps

1. **Review** this summary and full plan
2. **Approve** the migration (or request changes)
3. **Execute** Phase 1 (30 min)
4. **Verify** Phase 1 (run tests)
5. **Execute** Phase 2 (2 hrs)
6. **Verify** Phase 2 (run tests)
7. **Execute** Phase 3 (1 hr)
8. **Final verification** (full test suite)
9. **Commit** with detailed message
10. **Document** in architecture records

---

## Files Reference

**Full Plan:** `/home/moot/crucible/TYPE_CONSOLIDATION_PLAN.md` (detailed 13-section analysis)

**Architecture Assessment:** `/home/moot/crucible/ARCHITECTURE_ASSESSMENT.md` (SOLID principles, DDD, coupling analysis)

**This Summary:** `/home/moot/crucible/CONSOLIDATION_SUMMARY.md` (quick reference)

---

## Questions?

See full plan for:
- Detailed type catalog (Section 1)
- Dependency analysis (Section 2)
- Architectural implications (Section 3)
- Risk mitigation strategies (Section 6)
- Post-migration verification (Section 10)

**Estimated Reading Time:**
- This summary: 5 minutes
- Full plan: 30 minutes
- Architecture assessment: 15 minutes
