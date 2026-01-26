## Learnings: Graduation Spacing Refactor

### Architecture Insights

**Two Orthogonal Spacing Concerns:**

1. **Inter-element spacing** (between graduated items)
   - Handled by: `ElementKind.wants_blank_line_before(prev)`
   - Location: `graduation.rs::format_stdout_delta()`
   - Purpose: Blank lines between messages, tool calls, etc.

2. **Intra-element spacing** (inside rendered content)
   - Handled by: `text("")` nodes in `col([text(""), content, text("")])`
   - Location: `chat_app.rs`, `message_list.rs`
   - Purpose: Vertical padding around markdown, thinking blocks, etc.

**Key Discovery:** These are NOT redundant. Removing `text("")` would break visual layout.

### Cross-Frame State

**`pending_newline` is necessary:**
- Tracks whether previous frame ended with newline
- Converted to `prev_kind` for ElementKind logic
- Cannot be eliminated - needed for cross-frame spacing continuity

### Implementation Notes

**ElementKind enum:**
```rust
Block        → wants blank line before Block/ToolCall
Continuation → never wants blank line
ToolCall     → wants blank line before Block only
```

**Spacing rules matrix:**
```
prev=None          → no spacing
prev=Continuation  → no spacing
curr=Continuation  → no spacing
Block→Block        → blank line
ToolCall→Block     → blank line
Block→ToolCall     → no blank line
ToolCall→ToolCall  → no blank line
```

### Test Results

All 1410 tests pass after Tasks 1-4.

### Deferred Work

**Tasks 5-6 (remove text("") padding):**
- SHOULD NOT BE DONE
- Reason: Serves different purpose (intra-element vs inter-element)
- Would break visual layout

**Task 7 (remove pending_newline):**
- SHOULD NOT BE DONE
- Reason: Necessary for cross-frame state tracking
- Current implementation correctly uses it with ElementKind

**Task 8 (markdown spacing):**
- OPTIONAL
- Markdown renderer has own spacing logic (`ensure_block_spacing`)
- Orthogonal to graduation spacing
