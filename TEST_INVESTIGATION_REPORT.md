# Investigation Report: test_multiple_h1_sections Failure

## Test Summary

**Test**: `crates/crucible-parser/tests/heading_hierarchy.rs::test_multiple_h1_sections`
**Status**: FAILING
**Issue**: Second H1 heading gets `depth = Some(1)` instead of `depth = Some(0)`

## Test Input

```markdown
# Section 1

Content in section 1.

# Section 2

Content in section 2.
```

## Expected Behavior

Both H1 headings should be top-level blocks:
- `depth = Some(0)`
- `parent_block_id = None`

## Actual Behavior

- **First H1**: `depth = Some(0)`, `parent_block_id = None` ✓ (correct)
- **Second H1**: `depth = Some(1)`, `parent_block_id = Some("block_0")` ✗ (incorrect)

## Root Cause Analysis

### The Problem

The bug is in the **order of operations** in `/home/moot/crucible/crates/crucible-parser/src/block_extractor.rs` at **lines 208-223**.

### Current Flow (INCORRECT)

```rust
// Line 208-223 in block_extractor.rs
if let Some(mut block) = block {
    if self.config.preserve_empty_blocks || !block.is_empty() {
        let block_index = blocks.len();

        // 1. Assign hierarchy FIRST (using current stack state)
        block = self.assign_hierarchy(block, &heading_stack, block_index);

        // 2. THEN update heading stack (if this is a heading)
        if let Some(level) = block.heading_level() {
            heading_stack.push(level, block_index);
        }

        blocks.push(block);
        last_end = position.end_offset;
    }
}
```

### What Happens With Two H1s

#### Processing First H1 (Block Index 0)

1. **Before assign_hierarchy()**:
   - `heading_stack.stack = []` (empty)
   - `current_parent() = None`
   - `current_depth() = 0`

2. **assign_hierarchy()** assigns:
   - `parent_block_id = None`
   - `depth = Some(0)` ✓ (correct)

3. **After heading_stack.push(1, 0)**:
   - `heading_stack.stack = [(1, 0)]`

#### Processing Second H1 (Block Index 2)

1. **Before assign_hierarchy()**:
   - `heading_stack.stack = [(1, 0)]` ← **STILL CONTAINS FIRST H1**
   - `current_parent() = Some(0)` ← **WRONG!**
   - `current_depth() = 1` ← **WRONG!**

2. **assign_hierarchy()** assigns:
   - `parent_block_id = Some("block_0")` ✗ (incorrect)
   - `depth = Some(1)` ✗ (incorrect)

3. **After heading_stack.push(1, 2)**:
   - `heading_stack.push()` correctly pops the first H1 (line 42-47 in block_extractor.rs)
   - `heading_stack.stack = [(1, 2)]` ✓ (correct, but too late!)

### The Core Issue

**The hierarchy is assigned BEFORE the stack is updated.**

When `assign_hierarchy()` is called for the second H1:
- The stack still contains the first H1
- `current_parent()` returns `Some(0)` (the first H1)
- `current_depth()` returns `1` (one heading in stack)
- So the second H1 is incorrectly assigned as a child of the first H1

Only AFTER the assignment does `heading_stack.push()` correctly pop the first H1 and push the second H1.

## Code Location Details

### File: `/home/moot/crucible/crates/crucible-parser/src/block_extractor.rs`

#### Lines 208-223: The Buggy Flow

```rust
if let Some(mut block) = block {
    if self.config.preserve_empty_blocks || !block.is_empty() {
        let block_index = blocks.len();

        // Assign hierarchy before adding to blocks
        block = self.assign_hierarchy(block, &heading_stack, block_index);

        // If this is a heading, update the heading stack AFTER assigning its own hierarchy
        if let Some(level) = block.heading_level() {
            heading_stack.push(level, block_index);
        }

        blocks.push(block);
        last_end = position.end_offset;
    }
}
```

#### Lines 36-49: HeadingStack::push() Implementation

```rust
fn push(&mut self, level: u8, block_index: usize) {
    // Pop headings at same or deeper level
    while let Some((top_level, _)) = self.stack.last() {
        if *top_level >= level {
            self.stack.pop();
        } else {
            break;
        }
    }
    self.stack.push((level, block_index));
}
```

This correctly pops equal or deeper level headings, but it happens AFTER assign_hierarchy().

#### Lines 254-276: assign_hierarchy() Implementation

```rust
fn assign_hierarchy(
    &self,
    mut block: ASTBlock,
    heading_stack: &HeadingStack,
    _block_index: usize,
) -> ASTBlock {
    // Get current depth from the stack
    let depth = heading_stack.current_depth();

    // Get parent block index from the stack (if any)
    if let Some(parent_idx) = heading_stack.current_parent() {
        // Generate a block ID for the parent
        // Format: block_{index}
        block.parent_block_id = Some(format!("block_{}", parent_idx));
        block.depth = Some(depth);
    } else {
        // Top-level block (no parent heading)
        block.parent_block_id = None;
        block.depth = Some(0);
    }

    block
}
```

This queries the stack state to determine parent and depth. For headings, it should query AFTER the stack is updated.

## Conceptual Fix

For **heading blocks specifically**, the stack needs to be updated BEFORE calling assign_hierarchy():

```rust
if let Some(mut block) = block {
    if self.config.preserve_empty_blocks || !block.is_empty() {
        let block_index = blocks.len();

        // For headings: Update stack FIRST, THEN assign hierarchy
        if let Some(level) = block.heading_level() {
            heading_stack.push(level, block_index);
        }

        // Now assign hierarchy (stack is in correct state)
        block = self.assign_hierarchy(block, &heading_stack, block_index);

        blocks.push(block);
        last_end = position.end_offset;
    }
}
```

**However**, this creates a new problem: when assigning hierarchy to a heading, the heading itself is already in the stack, so `current_parent()` would return the heading itself (or its parent).

### Better Conceptual Fix

The assign_hierarchy() method needs to know if it's assigning hierarchy to a heading, and if so, it should:
1. Look at what the parent WOULD BE after the stack is updated
2. Calculate depth based on the post-update stack state

OR:

The flow should be:
1. For headings: Update stack first, then use a different logic to assign hierarchy
2. For non-headings: Keep current flow

OR:

Modify HeadingStack to have a method that "simulates" what the stack would look like after pushing a heading, without actually modifying it yet.

## Why HeadingStack::push() Works Correctly

The `push()` method at lines 36-49 correctly handles same-level headings:

```rust
// Pop headings at same or deeper level
while let Some((top_level, _)) = self.stack.last() {
    if *top_level >= level {  // Note: >= means same or deeper
        self.stack.pop();
    } else {
        break;
    }
}
```

When pushing H1 (level=1) when H1 (level=1) is on top:
- `top_level (1) >= level (1)` is true
- Pops the first H1
- Pushes the second H1
- Result: `stack = [(1, 2)]` ✓

The problem is this happens TOO LATE.

## Summary

### Bug Location
- **File**: `/home/moot/crucible/crates/crucible-parser/src/block_extractor.rs`
- **Lines**: 208-223 (main processing flow)
- **Function**: `extract_blocks()` method

### What the Current Logic Does
1. Calls `assign_hierarchy()` which reads the current stack state
2. Calls `heading_stack.push()` which updates the stack (correctly popping same-level headings)
3. The hierarchy was already assigned using the old stack state

### Why It's Wrong
For the second H1:
- When `assign_hierarchy()` is called, the stack still contains `[(1, 0)]` (first H1)
- So it assigns `depth=1` and `parent_block_id="block_0"`
- Then `push()` correctly updates the stack to `[(1, 2)]`, but the damage is done

### What the Correct Logic Should Be
When assigning hierarchy to a **heading block**, the depth and parent should be calculated based on what the stack will look like AFTER the heading is pushed, not before.

For a heading:
- The heading's parent should be the heading that will remain on the stack after pushing this heading
- The heading's depth should be the stack depth after pushing this heading
- The heading itself should NOT be considered when calculating its own parent/depth

### Specific Issue with Multiple Same-Level Headings
When encountering H1 after H1:
- The second H1 should have the same depth (0) and parent (None) as the first
- But currently it gets depth=1 and parent=first_H1
- This is because the stack cleanup (popping the first H1) happens AFTER hierarchy assignment
