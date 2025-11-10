# Rust Antipattern Fix Plan

## Overview

This document provides an exhaustive plan to fix the 30 Rust antipatterns identified in the Phase 2 code review.

**Total Issues**: 30 (3 Critical, 8 High, 12 Medium, 7 Low)

---

## Phase 1: Critical Fixes (Must Do Immediately)

### Critical #1: BlockHash Cloning in Hot Path
**File**: `crates/crucible-core/src/merkle/hybrid.rs:68-71`

**Current Code**:
```rust
let section_hashes: Vec<BlockHash> = sections
    .iter()
    .map(|section| section.binary_tree.root_hash.clone())
    .collect();
```

**Fix**:
```rust
let section_hashes: Vec<BlockHash> = sections
    .iter()
    .map(|section| section.binary_tree.root_hash)  // BlockHash is Copy, not Clone
    .collect();
```

**Impact**: Removes unnecessary allocations in document ingestion hot path.

**Implementation Steps**:
1. Search codebase for all `.root_hash.clone()` patterns
2. Replace with `.root_hash` (copy, not clone)
3. Verify with `cargo test`

**Search command**:
```bash
rg "\.root_hash\.clone\(\)" --type rust
```

---

### Critical #2: Vec Capacity Hints Missing
**File**: `crates/crucible-surrealdb/src/eav_graph/ingest.rs:502-558`

**Current Code**:
```rust
fn compute_section_properties(
    entity_id: &RecordId<EntityRecord>,
    doc: &ParsedDocument,
) -> Vec<Property> {
    let mut props = Vec::new();  // No capacity hint
```

**Fix**:
```rust
fn compute_section_properties(
    entity_id: &RecordId<EntityRecord>,
    doc: &ParsedDocument,
) -> Vec<Property> {
    let merkle_tree = HybridMerkleTree::from_document(doc);

    // Pre-calculate capacity: 2 (root + total) + sections * 2 (hash + metadata)
    let capacity = 2 + (merkle_tree.sections.len() * 2);
    let mut props = Vec::with_capacity(capacity);
```

**Impact**: Eliminates Vec reallocations during property storage.

**Implementation Steps**:
1. Calculate exact capacity needed
2. Add `Vec::with_capacity(capacity)` at initialization
3. Add comment explaining capacity calculation

---

### Critical #3: String Allocation in Blockquote Loop
**File**: `crates/crucible-parser/src/blockquotes.rs:89-105`

**Current Code**:
```rust
while i < lines.len() {
    if let Some(next_cap) = re.captures(lines[i]) {
        let next_text = next_cap.get(2).unwrap().as_str();
        if !full_content.is_empty() && !next_text.is_empty() {
            full_content.push(' ');  // May reallocate
        }
        full_content.push_str(next_text);  // May reallocate
```

**Fix**:
```rust
// Pre-calculate total length needed
let total_len: usize = lines[i..]
    .iter()
    .take_while(|line| re.is_match(line))
    .map(|line| line.len() + 1)  // +1 for space
    .sum();

let mut full_content = String::with_capacity(total_len);

while i < lines.len() {
    // ... rest of loop
```

**Impact**: Converts O(n²) to O(n) for large blockquotes.

**Implementation Steps**:
1. Calculate total length before loop
2. Pre-allocate String with capacity
3. Verify correctness with existing tests

---

## Phase 2: High Priority Fixes (Should Do This Sprint)

### High #4: String Instead of &str
**File**: `crates/crucible-surrealdb/src/eav_graph/ingest.rs:855-898`

**Strategy**: Add lifetimes to reduce allocations

**Changes Needed**:
```rust
// Before
fn extract_relations(entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<CoreRelation>

// After
fn extract_relations<'a>(entity_id: &'a RecordId<EntityRecord>, doc: &'a ParsedDocument) -> Vec<CoreRelation>
```

**Implementation Steps**:
1. Add lifetime parameter to function signature
2. Use borrowed data where possible
3. Only allocate when ownership transfer needed
4. Run tests to verify no regressions

---

### High #5: Silent Error Swallowing
**File**: `crates/crucible-parser/src/blockquotes.rs:50-55`

**Current Code**:
```rust
let re = match Regex::new(r"(?m)^(>+)\s*(.*)$") {
    Ok(re) => re,
    Err(_e) => {
        return errors;  // Silently returns empty errors
    }
};
```

**Fix**:
```rust
let re = Regex::new(r"(?m)^(>+)\s*(.*)$")
    .expect("Blockquote regex is valid and should never fail to compile");
```

**Rationale**: The regex is a compile-time constant. If it fails, it's a bug in the code, not user input. Using `expect` documents this invariant.

**Implementation Steps**:
1. Replace `match` with `expect()` and clear message
2. Verify with tests that regex compiles
3. Consider moving regex to lazy_static if used frequently

---

### High #6: Missing Copy Usage
**Status**: BlockHash is already `Copy`, but code uses `.clone()`

**Implementation**: Already covered by Critical #1

---

### High #7: Collecting Unnecessarily in aggregate_hashes
**File**: `crates/crucible-core/src/merkle/hybrid.rs:237-257`

**Current Code**:
```rust
fn aggregate_hashes(hashes: &[BlockHash]) -> BlockHash {
    let mut level: Vec<BlockHash> = hashes.to_vec();  // Full clone
    while level.len() > 1 {
        let mut next = Vec::new();
        // ...
        level = next;  // Move entire Vec
    }
```

**Fix**:
```rust
fn aggregate_hashes(hashes: &[BlockHash]) -> BlockHash {
    if hashes.is_empty() {
        return BlockHash::zero();
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut level = Vec::with_capacity((hashes.len() + 1) / 2);
    level.extend_from_slice(hashes);

    while level.len() > 1 {
        let len = (level.len() + 1) / 2;
        let mut next = Vec::with_capacity(len);

        for chunk in level.chunks(2) {
            let left = &chunk[0];
            let right = chunk.get(1).unwrap_or(&chunk[0]);
            next.push(combine_pair(left, right));
        }
        level = next;
    }

    level[0]
}
```

**Implementation Steps**:
1. Add early returns for empty/single-element cases
2. Pre-allocate Vec capacity for each level
3. Use `extend_from_slice` instead of `to_vec`
4. Verify correctness with existing Merkle tree tests

---

### High #8: Clone-Heavy SectionBuilder
**File**: `crates/crucible-core/src/merkle/hybrid.rs:311-326`

**Current Code**:
```rust
fn into_section(mut self) -> SectionNode {
    let hashed_blocks: Vec<(usize, BlockHash)> = self
        .blocks
        .drain(..)
        .map(|(idx, content)| (idx, hash_block_content(&content)))
        .collect();
```

**Fix**:
```rust
fn into_section(self) -> SectionNode {
    let hashed_blocks: Vec<(usize, BlockHash)> = self
        .blocks
        .into_iter()
        .map(|(idx, content)| (idx, hash_block_content(&content)))
        .collect();
```

**Implementation Steps**:
1. Replace `drain(..)` with `into_iter()`
2. Remove `mut` from `self` parameter
3. Run tests

---

### High #9: Missing Builder Pattern
**File**: `crates/crucible-parser/src/block_extractor.rs:165-210`

**New API**:
```rust
impl BlockExtractor {
    pub fn builder() -> BlockExtractorBuilder {
        BlockExtractorBuilder::default()
    }
}

#[derive(Default)]
pub struct BlockExtractorBuilder {
    config: ExtractionConfig,
}

impl BlockExtractorBuilder {
    pub fn min_paragraph_length(mut self, length: usize) -> Self {
        self.config.min_paragraph_length = length;
        self
    }

    pub fn preserve_empty_blocks(mut self, preserve: bool) -> Self {
        self.config.preserve_empty_blocks = preserve;
        self
    }

    pub fn merge_consecutive_paragraphs(mut self, merge: bool) -> Self {
        self.config.merge_consecutive_paragraphs = merge;
        self
    }

    pub fn build(self) -> BlockExtractor {
        BlockExtractor { config: self.config }
    }
}
```

**Usage**:
```rust
let extractor = BlockExtractor::builder()
    .min_paragraph_length(20)
    .preserve_empty_blocks(true)
    .build();
```

**Implementation Steps**:
1. Create `BlockExtractorBuilder` struct
2. Add fluent methods for each config field
3. Update existing code to use builder (optional, keep old API for compatibility)
4. Add tests for builder pattern

---

### High #10: Missing #[inline] Annotations
**Files**: Multiple small hot-path functions

**Functions to annotate**:
1. `crates/crucible-core/src/merkle/hybrid.rs:171-178` - `MerkleNode::hash()`
2. `crates/crucible-parser/src/types.rs:29-31` - `BlockHash::as_bytes()`
3. `crates/crucible-parser/src/types.rs:55-57` - `BlockHash::is_zero()`
4. `crates/crucible-parser/src/types.rs:1148-1150` - `ASTBlock::is_empty()`

**Implementation**:
```rust
#[inline]
pub fn hash(&self) -> &BlockHash {
    // ... implementation
}
```

**Steps**:
1. Search for small getter methods in hot paths
2. Add `#[inline]` attribute
3. Consider `#[inline(always)]` for truly critical paths (benchmark first)
4. Run benchmarks to verify improvement

---

### High #11: Incorrect Lifetime
**Status**: Already correct, no fix needed

---

## Phase 3: Medium Priority Fixes (Next Sprint)

### Medium #12-13: Deep Nesting & Large Functions
**File**: `crates/crucible-parser/src/block_extractor.rs:238-294`

**Strategy**: Extract helper methods to reduce complexity

**New Structure**:
```rust
impl BlockExtractor {
    pub fn extract_blocks(&self, document: &ParsedDocument) -> Result<Vec<ASTBlock>> {
        let mut blocks = Vec::new();
        let positions = self.collect_positions(document)?;
        let content_map = self.build_content_map(document);
        let heading_tree = self.build_heading_tree(&positions, &blocks);

        let mut last_end = 0;
        for position in positions {
            self.process_gap(last_end, position.start_offset, &heading_tree, &mut blocks);
            self.process_position(&position, &content_map, &heading_tree, &mut blocks)?;
            last_end = position.end_offset;
        }

        Ok(blocks)
    }

    fn process_gap(&self, last_end: usize, position_start: usize,
                   heading_tree: &HeadingTree, blocks: &mut Vec<ASTBlock>) {
        // Extracted gap processing logic
    }

    fn process_position(&self, position: &ContentPosition,
                        content_map: &ContentMap,
                        heading_tree: &HeadingTree,
                        blocks: &mut Vec<ASTBlock>) -> Result<()> {
        // Extracted position processing logic
    }

    fn should_include_block(&self, block: &ASTBlock) -> bool {
        self.config.preserve_empty_blocks || !block.is_empty()
    }
}
```

**Implementation Steps**:
1. Extract `process_gap` helper
2. Extract `process_position` helper
3. Extract `should_include_block` helper
4. Reduce nesting to max 3 levels
5. Verify all tests still pass

---

### Medium #14: Magic Numbers
**File**: `crates/crucible-parser/src/block_extractor.rs:184-190`

**Implementation**:
```rust
const DEFAULT_MIN_PARAGRAPH_LENGTH: usize = 10;
const DEFAULT_PRESERVE_EMPTY_BLOCKS: bool = false;
const DEFAULT_MERGE_CONSECUTIVE_PARAGRAPHS: bool = false;

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            min_paragraph_length: DEFAULT_MIN_PARAGRAPH_LENGTH,
            preserve_empty_blocks: DEFAULT_PRESERVE_EMPTY_BLOCKS,
            merge_consecutive_paragraphs: DEFAULT_MERGE_CONSECUTIVE_PARAGRAPHS,
        }
    }
}
```

---

### Medium #15: Missing Bounds Check
**File**: `crates/crucible-parser/src/block_extractor.rs:569-573`

**Implementation**:
```rust
let callout = if position.index < document.content.callouts.len() {
    document.content.callouts.get(position.index)
} else {
    document.callouts.get(position.index - document.content.callouts.len())
}.ok_or_else(|| ParseError::new(
    format!("Invalid callout index: {}", position.index),
    position.start_offset,
))?;
```

---

### Medium #16: Inefficient String Building
**File**: `crates/crucible-parser/src/block_extractor.rs:536-547`

**Implementation**: Pre-allocate String capacity based on list size

---

### Medium #17: Dead Code
**File**: `crates/crucible-parser/src/block_extractor.rs:36-44`

**Options**:
1. Remove unused fields if truly not needed
2. Document why they're kept for future features
3. Use them or create a tracking issue to use them

**Decision**: Add documentation explaining they're for future tree traversal

---

### Medium #18: Redundant Computation
**File**: `crates/crucible-surrealdb/src/eav_graph/ingest.rs:702-827`

**Fix**: Use `code_block.line_count` instead of recalculating

---

### Medium #19: Vec drain vs into_iter
**File**: `crates/crucible-core/src/merkle/hybrid.rs:312-316`

**Status**: Already covered by High #8

---

### Medium #20: HashMap for Small Collections
**File**: `crates/crucible-parser/src/block_extractor.rs:29`

**Decision**: Requires benchmarking

**Implementation**:
1. Create benchmark comparing HashMap vs Vec
2. Test with documents of varying sizes (10, 50, 100, 500 headings)
3. Switch to Vec if consistently faster for <100 headings
4. Keep HashMap if performance is similar (better for large documents)

---

### Medium #21: Verbose Error Construction
**Implementation**: Create test helper module

---

### Medium #22: Unnecessary Option Unwrapping
**File**: `crates/crucible-core/src/merkle/hybrid.rs:232`

**Fix**: Use `expect` with clear message explaining invariant

---

### Medium #23: Clone on Option<String>
**File**: `crates/crucible-core/src/merkle/hybrid.rs:88`

**Decision**: Requires API change analysis

**Implementation**:
1. Analyze if `SectionChange` can use references
2. If consumers need owned data, keep clone
3. If consumers can work with references, change to `&HeadingSummary`

---

## Phase 4: Low Priority (Technical Debt Backlog)

### Low #24: Missing Documentation
**Implementation**: Add rustdoc comments to all public APIs

**Template**:
```rust
/// Brief one-line description.
///
/// More detailed explanation if needed.
///
/// # Arguments
///
/// * `arg1` - Description
/// * `arg2` - Description
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// When this function returns an error
///
/// # Examples
///
/// ```
/// use crate::example;
/// let result = example();
/// assert_eq!(result, expected);
/// ```
pub fn example() -> Result<()> {
    // ...
}
```

---

### Low #25: Non-Idiomatic Names
**File**: `crates/crucible-parser/src/block_extractor.rs:780-789`

**Fix**: Rename `ContentMap` to `DocumentContentIndex`

---

### Low #26: Test Coverage
**Implementation**: Add property-based tests with `proptest`

---

### Low #27: Consistency in Error Handling
**Decision**: Establish style guide

**Standard**: Use `Result<Option<T>>` for "may fail or return nothing"

---

### Low #28: Verbose Debug Output
**Implementation**: Custom Debug implementations for large structs

---

### Low #29: Const Functions
**Implementation**: Add `const` to simple constructors

---

### Low #30: Clippy Lints
**Implementation**: Enable at crate level

```rust
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::module_name_repetitions)]
```

---

## Implementation Priorities

### Sprint 1 (Immediate - This Week)
- ✅ Critical #1: BlockHash cloning (30 min)
- ✅ Critical #2: Vec capacity hints (1 hour)
- ✅ Critical #3: Blockquote string allocation (2 hours)
- ✅ High #5: Silent error swallowing (30 min)

**Total: 4 hours**

### Sprint 2 (Next Week)
- High #4: String vs &str (3 hours)
- High #7: aggregate_hashes optimization (2 hours)
- High #8: SectionBuilder drain pattern (30 min)
- High #10: Inline annotations (1 hour)

**Total: 6.5 hours**

### Sprint 3 (Following Week)
- High #9: Builder pattern (4 hours)
- Medium #12-13: Refactor extract_blocks (6 hours)
- Medium #14-18: Quick wins (3 hours)

**Total: 13 hours**

### Backlog (Future)
- Medium #19-23: Case-by-case analysis
- Low #24-30: Polish and best practices

---

## Testing Strategy

### For Each Fix:
1. **Before**: Run existing tests to establish baseline
2. **After**: Re-run tests to verify no regressions
3. **Benchmark**: For performance fixes, measure improvement

### Benchmark Commands:
```bash
# Before fix
cargo bench --bench merkle_tree > before.txt

# After fix
cargo bench --bench merkle_tree > after.txt

# Compare
diff before.txt after.txt
```

### Test Commands:
```bash
# Run all tests
cargo test --workspace

# Run specific package tests
cargo test -p crucible-parser
cargo test -p crucible-core
cargo test -p crucible-surrealdb

# Run with verbose output
cargo test -- --nocapture
```

---

## Success Metrics

### Sprint 1 (Critical Fixes)
- [ ] All 3 critical issues fixed
- [ ] No test regressions
- [ ] Measurable performance improvement in benchmarks
- [ ] Code review approved

### Sprint 2-3 (High/Medium Priority)
- [ ] 80% of high-priority issues fixed
- [ ] 50% of medium-priority issues fixed
- [ ] Documentation updated
- [ ] Clippy warnings reduced by 50%

### Long-term (3 months)
- [ ] All critical and high-priority issues resolved
- [ ] 80% of medium-priority issues resolved
- [ ] Clippy enabled in CI
- [ ] Property-based tests added
- [ ] Performance benchmarks passing

---

## References

- **Original Analysis**: See Rust expert agent output
- **OpenSpec Change Proposal**: `openspec/changes/2025-11-08-enhance-markdown-parser-eav-mapping/`
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/
- **API Guidelines**: https://rust-lang.github.io/api-guidelines/
