# Visual Timeline of the Bug

## Document Being Parsed

```markdown
# Section 1          ← Block 0 (H1)

Content in section 1. ← Block 1 (Paragraph)

# Section 2          ← Block 2 (H1)

Content in section 2. ← Block 3 (Paragraph)
```

## Timeline of Events

### 📍 Processing Block 0: First H1 "Section 1"

#### Step 1: Before assign_hierarchy()
```
heading_stack.stack = []
current_parent() = None
current_depth() = 0
```

#### Step 2: assign_hierarchy() called
```
Input: Block 0 (H1 "Section 1")
Stack state: []
Assignment:
  ✓ parent_block_id = None  (correct - no parent)
  ✓ depth = Some(0)         (correct - top level)
```

#### Step 3: heading_stack.push(1, 0) called
```
Before: stack = []
After:  stack = [(1, 0)]
```

#### Step 4: Block added to blocks vector
```
blocks = [
  Block 0: H1 "Section 1", depth=0, parent=None
]
```

---

### 📍 Processing Block 1: Paragraph "Content in section 1."

#### Before assign_hierarchy()
```
heading_stack.stack = [(1, 0)]
current_parent() = Some(0)
current_depth() = 1
```

#### assign_hierarchy() called
```
Input: Block 1 (Paragraph)
Stack state: [(1, 0)]
Assignment:
  ✓ parent_block_id = Some("block_0")  (correct - under H1)
  ✓ depth = Some(1)                    (correct - one level deep)
```

#### Block added
```
blocks = [
  Block 0: H1 "Section 1", depth=0, parent=None
  Block 1: Paragraph, depth=1, parent=block_0
]
```

---

### 📍 Processing Block 2: Second H1 "Section 2" ⚠️ **BUG HERE**

#### Step 1: Before assign_hierarchy()
```
heading_stack.stack = [(1, 0)]  ⚠️ STILL HAS FIRST H1!
current_parent() = Some(0)      ⚠️ THINKS PARENT IS FIRST H1!
current_depth() = 1              ⚠️ THINKS DEPTH IS 1!
```

#### Step 2: assign_hierarchy() called
```
Input: Block 2 (H1 "Section 2")
Stack state: [(1, 0)]            ⚠️ WRONG STACK STATE
Assignment:
  ✗ parent_block_id = Some("block_0")  (WRONG - should be None)
  ✗ depth = Some(1)                    (WRONG - should be 0)
```

#### Step 3: heading_stack.push(1, 2) called
```
Before: stack = [(1, 0)]
Process:
  - Check: top_level (1) >= level (1)? YES
  - Pop: [(1, 0)] → []
  - Push: [] → [(1, 2)]
After:  stack = [(1, 2)]         ✓ THIS IS CORRECT!
```

**BUT IT'S TOO LATE!** The block was already assigned wrong parent and depth.

#### Step 4: Block added to blocks vector
```
blocks = [
  Block 0: H1 "Section 1", depth=0, parent=None
  Block 1: Paragraph, depth=1, parent=block_0
  Block 2: H1 "Section 2", depth=1, parent=block_0  ⚠️ WRONG!
]
```

---

## The Problem Visualized

```
     ┌─────────────────────────────────────────────┐
     │ Current Flow (BROKEN)                       │
     └─────────────────────────────────────────────┘

For Block 2 (Second H1):

  Stack State: [(1, 0)]  ← First H1 still in stack
         ↓
    assign_hierarchy()
         ↓
    Reads stack → parent=Some(0), depth=1  ❌ WRONG
         ↓
    heading_stack.push(1, 2)
         ↓
    Stack pops first H1 → stack = [(1, 2)]  ✓ Correct
         ↓
    But block already has wrong values!


     ┌─────────────────────────────────────────────┐
     │ What Should Happen                          │
     └─────────────────────────────────────────────┘

For Block 2 (Second H1):

  Stack State: [(1, 0)]  ← First H1 still in stack
         ↓
    heading_stack.push(1, 2)  ← UPDATE STACK FIRST
         ↓
    Stack pops first H1 → stack = [(1, 2)]  ✓
         ↓
    assign_hierarchy()
         ↓
    Reads stack → parent=None, depth=0  ✓ CORRECT

    OR use special logic that "simulates" the push
```

---

## Code Flow Comparison

### Current (Broken) Order
```rust
// Line 208-223 in block_extractor.rs
if let Some(mut block) = block {
    let block_index = blocks.len();

    // ❌ STEP 1: Assign using OLD stack state
    block = self.assign_hierarchy(block, &heading_stack, block_index);

    // ✓ STEP 2: Update stack correctly (but too late)
    if let Some(level) = block.heading_level() {
        heading_stack.push(level, block_index);
    }

    blocks.push(block);
}
```

### What's Needed (Conceptual)
```rust
if let Some(mut block) = block {
    let block_index = blocks.len();

    // For headings: need to consider POST-update stack state
    if let Some(level) = block.heading_level() {
        // Option A: Update stack first, then assign with special logic
        heading_stack.push(level, block_index);
        block = self.assign_hierarchy_for_heading(block, &heading_stack, block_index);
    } else {
        // For non-headings: use current stack state
        block = self.assign_hierarchy(block, &heading_stack, block_index);
    }

    blocks.push(block);
}
```

---

## Test Expectation vs Reality

```rust
// Test expectation (lines 166-170 in heading_hierarchy.rs)
for h1 in headings {
    assert_eq!(h1.heading_level(), Some(1));
    assert_eq!(h1.depth, Some(0));       // ❌ FAILS for second H1
    assert_eq!(h1.parent_block_id, None); // ❌ FAILS for second H1
}
```

**First H1**: ✓ `depth=Some(0)`, `parent_block_id=None`
**Second H1**: ✗ `depth=Some(1)`, `parent_block_id=Some("block_0")`

---

## Summary

The bug is a **timing issue**:

1. **assign_hierarchy()** is called while the old heading is still in the stack
2. This causes the new heading to think it's a child of the old heading
3. **heading_stack.push()** correctly cleans up the stack afterward
4. But the block was already assigned incorrect values

The fix requires ensuring that when assigning hierarchy to a heading, we consider what the stack will look like AFTER the heading is pushed, not before.
