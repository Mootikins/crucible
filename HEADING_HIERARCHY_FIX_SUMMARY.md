# Heading Hierarchy Fix - Implementation Summary

## Issue Fixed
Fixed the bug where multiple H1 headings at the same level were incorrectly assigned different depths. The old implementation treated depth as equivalent to heading level, which caused the second H1 to have depth=1 instead of depth=0.

## Root Cause
The `HeadingStack` implementation conflated **heading level** (H1-H6, the markdown syntax) with **depth** (position in the document hierarchy tree). This caused incorrect depth calculations when multiple headings at the same level appeared sequentially.

**Example of the bug:**
```markdown
# First Section    <- depth=0 ✓
Content here

# Second Section   <- depth=1 ✗ (should be 0)
More content
```

## Solution Implemented
Replaced the stack-based `HeadingStack` with a tree-based `HeadingTree` structure that properly separates:
- **Level**: The markdown heading level (H1=1, H2=2, etc.)
- **Depth**: The position in the document hierarchy (0 for roots, 1 for children, etc.)

## Implementation Details

### New Structures (lines 14-139)

#### HeadingTree
A tree structure that maintains:
- `nodes: HashMap<usize, HeadingNode>` - All heading nodes indexed by block_index
- `current_path: Vec<(u8, usize)>` - Active path from root to current heading

#### HeadingNode
Each heading node tracks:
- `level: u8` - The markdown level (1-6)
- `block_index: usize` - Position in blocks vector
- `parent_index: Option<usize>` - Parent heading (if any)
- `children: Vec<usize>` - Child headings

### Key Methods

#### `add_heading(level, block_index) -> (parent_idx, depth)`
1. Finds appropriate parent based on level
2. Calculates depth based on tree position
3. Updates tree structure
4. Returns parent and depth for immediate use

#### `find_parent_for_level(level) -> Option<usize>`
- Walks backward through current_path
- Finds first heading with level < new_level
- That heading becomes the parent
- Returns None if this is a root heading

#### `calculate_depth_for_level(level) -> u32`
- Counts ancestors in current_path
- Depth = number of headings with level < new_level
- Multiple headings at same level can have same depth

#### `update_path(level, block_index)`
- Removes headings at same or deeper level
- Adds new heading to path
- Maintains "active branch" of the tree

### Changes to extract_blocks() (lines 234-317)

**Line 237**: Changed initialization from `HeadingStack` to `HeadingTree`
```rust
let mut heading_tree = HeadingTree::new();
```

**Lines 282-294**: Updated heading processing logic
```rust
// For headings, calculate hierarchy BEFORE updating tree
if let Some(level) = block.heading_level() {
    let (parent_idx, depth) = heading_tree.add_heading(level, block_index);
    if let Some(parent) = parent_idx {
        block.parent_block_id = Some(format!("block_{}", parent));
    } else {
        block.parent_block_id = None;
    }
    block.depth = Some(depth);
} else {
    // For non-headings, use current tree state
    block = self.assign_hierarchy(block, &heading_tree, block_index);
}
```

**Lines 252-257, 308**: Updated all references from `heading_stack` to `heading_tree`

### Changes to assign_hierarchy() (lines 319-352)

Updated signature to use `HeadingTree` instead of `HeadingStack`:
```rust
fn assign_hierarchy(
    &self,
    mut block: ASTBlock,
    heading_tree: &HeadingTree,
    _block_index: usize,
) -> ASTBlock
```

## Test Results

### All 6 heading hierarchy tests passing:
```
test test_blocks_without_headings ... ok
test test_multiple_h1_sections ... ok           <- Previously failing!
test test_nested_headings_h1_h2 ... ok
test test_nested_headings_h1_h2_h3 ... ok
test test_single_heading_with_paragraph ... ok
test test_skipped_heading_level ... ok

test result: ok. 6 passed; 0 failed; 0 ignored
```

### No regressions in parser tests:
```
test result: ok. 103 passed; 0 failed; 0 ignored
```

### Total tests passing: 119

## Key Insight

The critical insight is that **depth is determined by position in the tree, NOT by heading level**. Multiple H1s can exist at different depths if they appear in different contexts:

```markdown
# Root (H1, depth=0)
## Child (H2, depth=1)
### Grandchild (H3, depth=2)
# Another Root (H1, depth=0)  <- Same level, same depth as first H1
## Another Child (H2, depth=1)
```

This is fundamentally different from the stack approach which tried to infer tree structure from level differences alone.

## Files Modified
- `/home/moot/crucible/crates/crucible-parser/src/block_extractor.rs`
  - Added `HashMap` import
  - Replaced `HeadingStack` with `HeadingTree` (lines 14-139)
  - Updated `extract_blocks()` method (lines 234-317)
  - Updated `assign_hierarchy()` method (lines 319-352)

## Success Criteria - All Met ✓
1. ✓ All code compiles without errors
2. ✓ All 6 heading hierarchy tests pass (including `test_multiple_h1_sections`)
3. ✓ No regressions in other parser tests (103 library tests pass)
4. ✓ Code follows the design exactly as specified
