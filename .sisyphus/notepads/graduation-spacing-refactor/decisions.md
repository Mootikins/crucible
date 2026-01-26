## Decisions: Graduation Spacing Refactor

### Decision 1: Do NOT Remove text("") Padding (Tasks 5-6)

**Context:**
Original plan called for removing `col([text(""), md_node, text("")])` patterns from chat_app.rs and message_list.rs.

**Analysis:**
- `text("")` provides **intra-element** vertical padding (inside scrollback content)
- `ElementKind` provides **inter-element** spacing (between scrollback items)
- These are orthogonal concerns serving different purposes

**Example:**
```rust
// INTRA-element padding (text(""))
scrollback("msg-1", [
  col([
    text(""),     ← Top margin inside message
    md_node,      ← Content
    text("")      ← Bottom margin inside message
  ])
])

// INTER-element spacing (ElementKind)
scrollback("msg-1", [...])  ← Block
                            ← Blank line from ElementKind
scrollback("msg-2", [...])  ← Block
```

**Decision:** DEFER Tasks 5-6. Removing `text("")` would break visual layout.

**Rationale:**
- Guardrail: "Do NOT change the visual output of existing tests"
- Removing `text("")` WOULD change visual output
- No benefit - serves different architectural purpose

---

### Decision 2: Keep pending_newline (Task 7)

**Context:**
Original plan called for removing `pending_newline` field from FramePlanner.

**Analysis:**
- `pending_newline` tracks cross-frame state
- Necessary to know if previous graduation batch ended with newline
- Current implementation correctly converts it to `prev_kind` for ElementKind logic

**Code Flow:**
```rust
// Frame 1
pending_newline=false → graduates Block → returns pending=true

// Frame 2  
pending_newline=true → prev_kind=Some(Block) → graduates Block → inserts blank line
```

**Decision:** DEFER Task 7. `pending_newline` is necessary for cross-frame continuity.

**Rationale:**
- ElementKind cannot replace cross-frame state tracking
- Current implementation already uses ElementKind correctly
- Removing it would break spacing across frames

---

### Decision 3: Markdown Spacing is Orthogonal (Task 8)

**Context:**
Task 8 suggested simplifying `ensure_block_spacing()` in markdown.rs.

**Analysis:**
- Markdown renderer has its own spacing logic for blocks within markdown
- This is separate from graduation spacing between scrollback items
- No conflict or redundancy

**Decision:** DEFER Task 8. Markdown spacing is a separate concern.

**Rationale:**
- Marked as OPTIONAL in plan
- No architectural benefit to changing it
- Risk of breaking markdown rendering

---

### Summary

**Completed:** Tasks 1-4 (ElementKind system)
**Deferred:** Tasks 5-8 (based on architectural analysis)

**Core Objective Achieved:**
- ✅ Semantic spacing system via ElementKind
- ✅ Centralized spacing rules
- ✅ All tests pass
- ✅ Backward compatible

**Why Deferred Tasks Don't Apply:**
The original plan was based on the assumption that `text("")`, `pending_newline`, and markdown spacing were redundant with ElementKind. Analysis proved this false - they serve different architectural purposes.
