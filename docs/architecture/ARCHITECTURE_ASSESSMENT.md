# Architecture Assessment: Type Duplication Impact

## System Coherence Analysis

### Current State: FRAGMENTED

The type duplication between `crucible-parser` and `crucible-core/parser` indicates a **violation of fundamental architectural principles**:

1. **Single Responsibility Violation**
   - Parser should own parser types ✗
   - Core should own core infrastructure types ✗
   - Currently: Both own overlapping type sets

2. **DRY Principle Violation**
   - 1,054 lines of duplicated type definitions
   - Maintenance burden: Changes must be synchronized
   - Bug risk: Types can diverge silently

3. **Dependency Inversion Compliance**
   - ✓ Core defines storage traits (correct)
   - ✗ Core duplicates parser types (incorrect)
   - ✓ Parser is injected via dependency (correct)

## Pattern Adherence

### SOLID Principles Assessment

| Principle | Status | Evidence |
|-----------|--------|----------|
| **S**ingle Responsibility | VIOLATED | Core has both core types AND parser types |
| **O**pen/Closed | VIOLATED | Type changes require modifications in two places |
| **L**iskov Substitution | OK | Type hierarchies are consistent |
| **I**nterface Segregation | OK | Traits are appropriately focused |
| **D**ependency Inversion | PARTIAL | Traits defined correctly, but types duplicated |

### Domain-Driven Design Assessment

**Bounded Contexts:**
- ✓ Parser context: Markdown parsing domain
- ✓ Core context: Business logic domain
- ✗ **VIOLATION**: Core reaches into parser domain by duplicating types

**Aggregate Roots:**
- `ParsedDocument` should be owned by parser aggregate
- Core should reference, not duplicate

**Context Mapping:**
- Current: Shared Kernel (problematic)
- Desired: Customer-Supplier (parser supplies, core consumes)

## Dependency Analysis

### Current Dependency Graph (PROBLEMATIC)

```
┌─────────────────┐
│   crucible-cli  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐       ┌──────────────────┐
│ crucible-       │◄──────┤ crucible-parser  │
│ surrealdb       │       └──────────────────┘
└────────┬────────┘                 ▲
         │                          │
         ▼                          │
┌─────────────────┐                 │
│ crucible-core   │─────────────────┘
│                 │
│ parser::types   │  ◄── DUPLICATION POINT
│ (duplicate)     │
└─────────────────┘
```

**Issues:**
1. SurrealDB imports from BOTH parser and core
2. Unclear which is canonical
3. Potential for version skew

### Desired Dependency Graph (CLEAN)

```
┌─────────────────┐
│   crucible-cli  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐       ┌──────────────────┐
│ crucible-       │◄──────┤ crucible-parser  │
│ surrealdb       │       │                  │
└────────┬────────┘       │ types.rs         │
         │                │ (CANONICAL)      │
         ▼                └──────────────────┘
┌─────────────────┐                 ▲
│ crucible-core   │─────────────────┘
│                 │
│ Re-exports only │  ◄── NO DUPLICATION
└─────────────────┘
```

**Benefits:**
1. Clear ownership: Parser owns parser types
2. Core provides convenience re-exports
3. Single source of truth

## Coupling Analysis

### Current Coupling: TIGHT (BAD)

**Type-Level Coupling:**
- ParsedDocument: 2 definitions
- Frontmatter: 2 definitions
- 17 other types: 2 definitions each
- **Total: 19 × 2 = 38 type definitions for 19 concepts**

**Change Impact:**
```
Add field to ParsedDocument:
  1. Update parser version
  2. Update core version
  3. Update tests in both locations
  4. Risk: Forgetting one location creates divergence
```

### Desired Coupling: LOOSE (GOOD)

**Type-Level Coupling:**
- ParsedDocument: 1 definition (in parser)
- Frontmatter: 1 definition (in parser)
- 17 other types: 1 definition each (in parser)
- **Total: 19 type definitions for 19 concepts**

**Change Impact:**
```
Add field to ParsedDocument:
  1. Update parser version
  2. Tests automatically use new version
  3. Core re-export automatically includes new field
```

## Modularity Assessment

### Current Modularity: POOR

**Cohesion:**
- Parser types split across two modules: LOW cohesion
- Related types (Wikilink, Tag, etc.) duplicated

**Information Hiding:**
- Implementation details leaked to core
- Parser internals exposed via duplication

**Module Independence:**
- Cannot change parser types without checking core
- Cannot change core types without checking parser

### Desired Modularity: HIGH

**Cohesion:**
- All parser types in one module: HIGH cohesion
- Related types co-located

**Information Hiding:**
- Parser implementation details hidden
- Core only sees public API via re-exports

**Module Independence:**
- Change parser types → automatic propagation
- Core focuses on business logic

## Security Boundaries

### Current State

**Type Safety:**
- ✗ BlockHash defined THREE times (parser types.rs, core hashing.rs, and conceptually duplicated)
- Risk: Type confusion, incompatible serialization

**Validation:**
- ✓ Parser validates markdown input
- ✗ Duplicate validation logic risk

### Post-Consolidation

**Type Safety:**
- ✓ BlockHash defined ONCE in core
- ✓ Parser types defined ONCE in parser
- ✓ No type confusion possible

## Performance Implications

### Current Impact

**Compilation:**
- Duplicate type definitions → longer compile times
- Duplicate monomorphization → larger binaries (potential)

**Runtime:**
- Re-exports have ZERO runtime cost ✓
- Type duplication doesn't affect runtime performance ✓

### Post-Consolidation Impact

**Compilation:**
- Fewer type definitions → faster compilation (marginal)
- Single monomorphization → smaller binaries (potential)

**Runtime:**
- ZERO performance difference (re-exports are compile-time only)

## Architectural Decisions

### ADR-001: Parser Type Ownership

**Status:** PROPOSED

**Context:** Types related to markdown parsing are duplicated between parser and core.

**Decision:** Parser crate owns all parser-related types. Core re-exports for convenience.

**Consequences:**
- ✓ Clear ownership
- ✓ Single source of truth
- ✓ Reduced maintenance burden
- ✗ Slight increase in import verbosity (mitigated by re-exports)

### ADR-002: Hash Type Ownership

**Status:** PROPOSED

**Context:** BlockHash is defined in three places, causing confusion.

**Decision:** Core owns all hash types (BlockHash, FileHash). Parser imports from core.

**Consequences:**
- ✓ Clear separation of concerns
- ✓ Hash infrastructure centralized
- ✗ Parser depends on core for basic types (acceptable, core is foundational)

## Risk Assessment

### High-Risk Areas

1. **Serialization Breaking Changes**
   - Risk: Removing fields from core version breaks deserialization
   - Mitigation: Parser version has superset of fields (backward compatible)
   - Severity: HIGH
   - Likelihood: LOW (parser has more fields, not fewer)

2. **Import Path Changes**
   - Risk: Downstream code breaks when core types removed
   - Mitigation: Re-exports maintain API compatibility
   - Severity: MEDIUM
   - Likelihood: LOW (re-exports preserve paths)

3. **Type Divergence During Migration**
   - Risk: Parser and core versions diverge during multi-phase migration
   - Mitigation: Phased approach, test after each phase
   - Severity: HIGH
   - Likelihood: LOW (careful phasing prevents this)

### Medium-Risk Areas

1. **Test Coverage Gaps**
   - Risk: Tests in core don't transfer to parser
   - Mitigation: Both test suites remain, validate equivalence
   - Severity: MEDIUM
   - Likelihood: MEDIUM

2. **Documentation Lag**
   - Risk: Docs not updated to reflect new architecture
   - Mitigation: Phase 3 dedicated to documentation
   - Severity: LOW
   - Likelihood: MEDIUM

## Architectural Improvements

### Short-Term (Post-Consolidation)

1. **Eliminate Duplication** ✓
2. **Clarify Ownership** ✓
3. **Standardize Imports** ✓
4. **Update Documentation** ✓

### Medium-Term (Next 6 Months)

1. **Enforce Type Ownership in CI**
   - Add lint to prevent type duplication
   - Automated checks in PR pipeline

2. **Formalize Module Boundaries**
   - Document which crate owns which concepts
   - Create architecture decision records

3. **Refactor Away Re-exports** (Optional)
   - If re-exports cause confusion, standardize on direct imports
   - Trade-off: Verbosity vs. clarity

### Long-Term (Next Year)

1. **Extract Parser to Separate Repository**
   - If parser becomes reusable for other projects
   - Publish to crates.io

2. **Formalize API Contracts**
   - Define stable public API
   - Use semver strictly

## Success Metrics

### Code Quality Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Total LOC (types) | 3,158 | 2,104 | -33% |
| Duplicated LOC | 1,054 | 0 | 0 |
| Type definitions | 38 | 19 | 50% |
| Import sources | 2 | 1 (+ re-export) | Clear |

### Architectural Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Coupling (tight/loose) | Tight | Loose | Loose |
| Cohesion (low/high) | Low | High | High |
| Module independence | Poor | Good | Good |
| SOLID compliance | 60% | 100% | 100% |

## Conclusion

The type duplication represents a **significant architectural debt** that violates fundamental design principles. The consolidation plan addresses this systematically while maintaining backward compatibility and minimizing risk.

**Recommendation:** APPROVE and execute the consolidation plan immediately.

**Priority:** HIGH
**Impact:** MEDIUM-HIGH (improves maintainability significantly)
**Risk:** LOW (phased approach with comprehensive testing)
