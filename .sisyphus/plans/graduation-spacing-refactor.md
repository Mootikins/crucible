# Graduation Spacing Refactor

## Context

### Original Request
Refactor the TUI graduation spacing system to be simpler and more maintainable. Each element should be responsible for its own preceding whitespace, using a semantic element kind system instead of scattered `text("")` padding.

### Interview Summary
**Key Discussions**:
- Current system has complex stateful tracking (`pending_newline` across frames)
- Manual `text("")` padding is scattered across chat_app.rs and message_list.rs
- `boundary_lines` adds another layer of complexity
- User wants: "Each element in graduation is responsible for its own preceding whitespace"

**Research Findings**:
- 59 occurrences of spacing-related patterns across 9 files
- `newline: bool` on StaticNode determines if element wants trailing spacing
- `pending_newline` tracks cross-frame state in FramePlanner
- `format_stdout_delta()` uses complex logic to merge spacing decisions

### Metis Review
**Identified Gaps** (addressed in plan):
- Need backward compatibility during transition
- Tests use `scrollback()` extensively - must maintain API
- `newline: bool` is deeply embedded - use ElementKind as source of truth that derives newline

---

## Work Objectives

### Core Objective
Replace scattered `text("")` padding and complex `pending_newline` state tracking with a semantic `ElementKind` enum that declares spacing requirements.

### Concrete Deliverables
- `ElementKind` enum in node.rs with spacing rules
- Updated `scrollback()` to accept ElementKind
- Simplified `format_stdout_delta()` without cross-frame state
- Removed `text("")` padding from chat_app.rs and message_list.rs
- Updated tests to use new API

### Definition of Done
- [ ] All 1410+ tests pass
- [ ] No `text("")` padding in chat_app.rs render methods
- [ ] No `pending_newline` field in FramePlanner
- [ ] `ElementKind` determines all spacing decisions

### Must Have
- Backward compatible `scrollback()` function
- Semantic element kinds: Block, Continuation, ToolCall
- Centralized spacing rules in ElementKind::wants_blank_line_before()

### Must NOT Have (Guardrails)
- Do NOT change the visual output of existing tests
- Do NOT remove `boundary_lines` yet (serves real purpose for stdout/viewport separation)
- Do NOT over-engineer with many element kinds - keep it minimal
- Do NOT break streaming graduation

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (cargo test)
- **User wants tests**: TDD approach
- **Framework**: cargo test / nextest

### Verification Commands
```bash
cargo test -p crucible-cli --lib  # All ~1410 tests should pass
cargo test -p crucible-cli "graduation" --lib  # Graduation-specific tests
cargo test -p crucible-cli "scrollback" --lib  # Scrollback-related tests
```

---

## Task Flow

```
Task 1 (ElementKind enum)
    ↓
Task 2 (Update StaticNode) 
    ↓
Task 3 (scrollback helpers)
    ↓
Task 4 (graduation spacing logic)
    ↓
Task 5 (remove text("") from chat_app)
    ↓
Task 6 (remove text("") from message_list)
    ↓
Task 7 (remove pending_newline)
    ↓
Task 8 (cleanup ensure_block_spacing)
```

## Parallelization

All tasks are sequential - each depends on the previous.

---

## TODOs

- [ ] 1. Add ElementKind enum to node.rs

  **What to do**:
  - Add `ElementKind` enum with variants: `Block`, `Continuation`, `ToolCall`
  - Add `wants_blank_line_before(prev: Option<ElementKind>) -> bool` method
  - Add `wants_newline_after() -> bool` method (maps to current `newline` field)
  - Place after imports, before Node enum

  **Must NOT do**:
  - Do not add too many variants - keep it minimal
  - Do not change any existing code yet

  **Parallelizable**: NO (foundation for all other tasks)

  **References**:
  - `crates/crucible-cli/src/tui/oil/node.rs:60-65` - Current StaticNode with `newline: bool`
  - `crates/crucible-cli/src/tui/oil/graduation.rs:108-114` - How newline is used in spacing logic

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes (no changes to behavior yet)
  - [ ] `ElementKind` enum exists with Block, Continuation, ToolCall variants
  - [ ] `wants_blank_line_before()` and `wants_newline_after()` methods implemented

  **Commit**: YES
  - Message: `feat(oil): add ElementKind enum for semantic spacing`
  - Files: `crates/crucible-cli/src/tui/oil/node.rs`

---

- [ ] 2. Update StaticNode to include ElementKind

  **What to do**:
  - Add `kind: ElementKind` field to `StaticNode` struct
  - Keep `newline: bool` field for now (backward compat)
  - Update `StaticNode` to derive `newline` from `kind.wants_newline_after()` in a helper method

  **Must NOT do**:
  - Do not remove `newline` field yet - too many usages
  - Do not change any existing functionality

  **Parallelizable**: NO (depends on task 1)

  **References**:
  - `crates/crucible-cli/src/tui/oil/node.rs:60-65` - StaticNode struct

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] `StaticNode` has `kind: ElementKind` field
  - [ ] Default kind is `ElementKind::Block`

  **Commit**: YES
  - Message: `feat(oil): add ElementKind field to StaticNode`
  - Files: `crates/crucible-cli/src/tui/oil/node.rs`

---

- [ ] 3. Update scrollback() helpers to use ElementKind

  **What to do**:
  - Create `scrollback_with_kind(key, kind, children)` function
  - Update `scrollback()` to use `ElementKind::Block`
  - Update `scrollback_continuation()` to use `ElementKind::Continuation`
  - Add `scrollback_tool(key, children)` using `ElementKind::ToolCall`
  - Ensure `newline` field is set based on `kind.wants_newline_after()`

  **Must NOT do**:
  - Do not break existing scrollback() API
  - Do not change callers yet

  **Parallelizable**: NO (depends on task 2)

  **References**:
  - `crates/crucible-cli/src/tui/oil/node.rs:148-165` - Current scrollback functions

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] `scrollback_with_kind()` function exists
  - [ ] `scrollback_tool()` function exists
  - [ ] Existing tests using `scrollback()` still work

  **Commit**: YES
  - Message: `feat(oil): add scrollback_with_kind and scrollback_tool helpers`
  - Files: `crates/crucible-cli/src/tui/oil/node.rs`

---

- [ ] 4. Update graduation to use ElementKind for spacing

  **What to do**:
  - Modify `GraduatedContent` to include `kind: ElementKind` (copy from StaticNode)
  - Update `collect_static_nodes_readonly()` to copy `kind` from StaticNode
  - Update `format_stdout_delta()` to use `kind.wants_blank_line_before(prev_kind)` 
  - Keep `pending_newline` parameter for now but deprecate its usage internally

  **Must NOT do**:
  - Do not remove `pending_newline` parameter yet (breaks API)
  - Do not change `boundary_lines` behavior

  **Parallelizable**: NO (depends on task 3)

  **References**:
  - `crates/crucible-cli/src/tui/oil/graduation.rs:96-126` - format_stdout_delta
  - `crates/crucible-cli/src/tui/oil/graduation.rs:136-150` - collect_static_nodes_readonly
  - `crates/crucible-cli/src/tui/oil/graduation.rs:176-182` - GraduatedContent struct

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli "graduation" --lib` passes
  - [ ] `GraduatedContent` has `kind` field
  - [ ] Spacing is computed from ElementKind, not just newline bool

  **Commit**: YES
  - Message: `refactor(oil): use ElementKind for graduation spacing logic`
  - Files: `crates/crucible-cli/src/tui/oil/graduation.rs`

---

- [ ] 5. Remove text("") padding from chat_app.rs

  **What to do**:
  - Find all `col([text(""), X, text("")])` patterns in render methods
  - Remove manual padding - spacing is now handled by ElementKind
  - Update `render_streaming()` to use `scrollback_with_kind()` appropriately
  - Update `render_message_with_continuation()` similarly

  **Must NOT do**:
  - Do not remove `text("")` used for actual empty content
  - Do not break visual output

  **Parallelizable**: NO (depends on task 4)

  **References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2835-2841` - render_message_with_continuation
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2874-2922` - render_streaming graduated blocks
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:2943-2966` - render_streaming in-progress

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] No `col([text(""), md_node, text("")])` patterns in chat_app.rs
  - [ ] Manual verification: `cargo run -p crucible-cli -- chat` shows correct spacing

  **Commit**: YES
  - Message: `refactor(oil): remove text("") padding from chat_app, use ElementKind`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`

---

- [ ] 6. Remove text("") padding from message_list.rs

  **What to do**:
  - Find all `col([text(""), X, text("")])` patterns
  - Update `render_message()`, `render_tool_call()`, `render_shell_execution()`, `render_subagent()`
  - Use appropriate `scrollback_with_kind()` or `scrollback_tool()` calls

  **Must NOT do**:
  - Do not remove structural text("") nodes
  - Do not change tool call compact rendering

  **Parallelizable**: NO (depends on task 5)

  **References**:
  - `crates/crucible-cli/src/tui/oil/components/message_list.rs:76-86` - render_message
  - `crates/crucible-cli/src/tui/oil/components/message_list.rs:225-282` - render_tool_call
  - `crates/crucible-cli/src/tui/oil/components/message_list.rs:320-387` - render_shell_execution
  - `crates/crucible-cli/src/tui/oil/components/message_list.rs:432` - render_subagent

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] Reduced `text("")` usage in message_list.rs

  **Commit**: YES
  - Message: `refactor(oil): remove text("") padding from message_list, use ElementKind`
  - Files: `crates/crucible-cli/src/tui/oil/components/message_list.rs`

---

- [ ] 7. Remove pending_newline from FramePlanner

  **What to do**:
  - Remove `pending_newline` field from FramePlanner
  - Update `plan()` method to not track cross-frame state
  - Update `format_stdout_delta()` to not need `pending_newline` parameter
  - Each graduation batch is now self-contained

  **Must NOT do**:
  - Do not change visual output
  - Do not remove boundary_lines (still needed for stdout/viewport separation)

  **Parallelizable**: NO (depends on tasks 5, 6)

  **References**:
  - `crates/crucible-cli/src/tui/oil/planning.rs:94-106` - FramePlanner with pending_newline
  - `crates/crucible-cli/src/tui/oil/planning.rs:123-125` - pending_newline usage
  - `crates/crucible-cli/src/tui/oil/graduation.rs:96-126` - format_stdout_delta signature

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] No `pending_newline` field in FramePlanner
  - [ ] `format_stdout_delta()` has simpler signature

  **Commit**: YES
  - Message: `refactor(oil): remove pending_newline state tracking from graduation`
  - Files: `crates/crucible-cli/src/tui/oil/planning.rs`, `crates/crucible-cli/src/tui/oil/graduation.rs`

---

- [ ] 8. Cleanup markdown ensure_block_spacing (OPTIONAL)

  **What to do**:
  - Evaluate if `ensure_block_spacing()` and `needs_blank_line` flag can be simplified
  - If markdown nodes use scrollback, they could benefit from ElementKind
  - This may be deferred if it's too invasive

  **Must NOT do**:
  - Do not break markdown rendering
  - Do not force this if tests start failing

  **Parallelizable**: NO (depends on task 7)

  **References**:
  - `crates/crucible-cli/src/tui/oil/markdown.rs:302-311` - ensure_block_spacing

  **Acceptance Criteria**:
  - [ ] `cargo test -p crucible-cli --lib` passes
  - [ ] Either simplified or documented why kept

  **Commit**: YES (if changes made)
  - Message: `refactor(oil): simplify markdown block spacing`
  - Files: `crates/crucible-cli/src/tui/oil/markdown.rs`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `feat(oil): add ElementKind enum` | node.rs | cargo test |
| 2 | `feat(oil): add ElementKind to StaticNode` | node.rs | cargo test |
| 3 | `feat(oil): add scrollback helpers` | node.rs | cargo test |
| 4 | `refactor(oil): use ElementKind in graduation` | graduation.rs | cargo test graduation |
| 5 | `refactor(oil): remove text("") from chat_app` | chat_app.rs | cargo test + manual |
| 6 | `refactor(oil): remove text("") from message_list` | message_list.rs | cargo test |
| 7 | `refactor(oil): remove pending_newline` | planning.rs, graduation.rs | cargo test |
| 8 | `refactor(oil): simplify markdown spacing` | markdown.rs | cargo test |

---

## Success Criteria

### Verification Commands
```bash
cargo test -p crucible-cli --lib  # Expected: 1410+ tests pass
cargo test -p crucible-cli "graduation" --lib  # Expected: all graduation tests pass
cargo test -p crucible-cli "spacing" --lib  # Expected: any spacing tests pass
```

### Final Checklist
- [ ] All tests pass
- [ ] No `col([text(""), X, text("")])` patterns in chat_app.rs
- [ ] No `pending_newline` field in FramePlanner
- [ ] ElementKind enum exists and is used for spacing decisions
- [ ] Manual verification: chat TUI shows correct visual spacing
