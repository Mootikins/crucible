# Interaction Modal Snapshot Tests

## Context

### Original Request
Add snapshot tests for interaction modal rendering (AskRequest and PermRequest) to complete the deferred acceptance criteria from the interaction-primitives plan.

### Interview Summary
**Key Findings**:
- Snapshot test infrastructure exists at `crates/crucible-cli/src/tui/oil/tests/chat_app_snapshot_tests.rs`
- Pattern: Create `InkChatApp`, call `open_interaction()`, use `render_app(&app)` with `assert_snapshot!`
- Unit tests exist for state transitions but visual regression tests are missing
- PTY tests not needed - snapshot tests are appropriate for modal rendering

---

## Work Objectives

### Core Objective
Add snapshot tests for `InteractionModal` visual rendering to catch regressions in Ask and Permission modal displays.

### Concrete Deliverables
- 10 new snapshot tests in `chat_app_snapshot_tests.rs`
- Corresponding `.snap` files in `snapshots/` directory

### Definition of Done
- [x] `cargo nextest run -p crucible-cli snapshot` passes
- [x] Snapshot files generated and reviewed with `cargo insta review`
- [x] No visual regressions in existing snapshots

### Must Have
- AskRequest with choices (first/second/last selected)
- AskRequest with allow_other option
- AskRequest free-text only
- PermRequest for bash, read, write, tool

### Must NOT Have (Guardrails)
- No PTY tests (too complex for modal rendering)
- No new test files (add to existing snapshot file)
- No changes to interaction modal implementation

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES
- **User wants tests**: Snapshot tests with `insta`
- **Framework**: `cargo nextest` + `insta`

---

## TODOs

- [x] 1. Add interaction modal snapshot tests to chat_app_snapshot_tests.rs

  **What to do**:
  - Add new `mod interaction_modal_snapshots` at end of file
  - Import: `crucible_core::interaction::{AskRequest, InteractionRequest, PermRequest}`
  - Add 10 snapshot tests:
    1. `snapshot_ask_modal_with_choices_first_selected` - Default state
    2. `snapshot_ask_modal_with_choices_second_selected` - After Down key
    3. `snapshot_ask_modal_with_allow_other` - Shows "Other..." option
    4. `snapshot_ask_modal_free_text_only` - No choices, just text input
    5. `snapshot_perm_modal_bash_command` - `PermRequest::bash(["npm", "install", "lodash"])`
    6. `snapshot_perm_modal_file_write` - `PermRequest::write(["home", "user", "src", "main.rs"])`
    7. `snapshot_perm_modal_file_read` - `PermRequest::read(["etc", "hosts"])`
    8. `snapshot_perm_modal_tool` - `PermRequest::tool("semantic_search", ...)`
    9. `snapshot_ask_modal_many_choices` - 8 choices to test scrolling
    10. `snapshot_ask_modal_last_selected` - Cursor at last item

  **Must NOT do**:
  - Don't modify interaction modal implementation
  - Don't add raw ANSI tests (use strip_ansi pattern)

  **Parallelizable**: NO (single task)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/tests/chat_app_snapshot_tests.rs:13-20` - `render_app()` helper function
  - `crates/crucible-cli/src/tui/oil/tests/chat_app_snapshot_tests.rs:31-35` - Basic snapshot test pattern

  **Type References**:
  - `crates/crucible-core/src/interaction.rs:42-88` - `AskRequest` with `choices()`, `allow_other()`
  - `crates/crucible-core/src/interaction.rs:717-841` - `PermRequest::bash()`, `read()`, `write()`, `tool()`
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:823-835` - `open_interaction()` method

  **Test References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:3755-3780` - `test_perm_request_bash_renders` pattern

  **Acceptance Criteria**:
  - [x] `cargo nextest run -p crucible-cli interaction_modal_snapshots` runs 10 tests
  - [x] `cargo insta review` shows 10 new snapshots to accept
  - [x] All snapshots show correct modal content (question text, choices, keybindings)

  **Commit**: YES
  - Message: `test(cli): add snapshot tests for interaction modals`
  - Files: `crates/crucible-cli/src/tui/oil/tests/chat_app_snapshot_tests.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli`

---

## Success Criteria

### Verification Commands
```bash
cargo nextest run -p crucible-cli interaction_modal_snapshots  # All 10 tests pass
cargo insta review                                              # Review and accept snapshots
```

### Final Checklist
- [x] AskRequest snapshots show question, choices, selection indicator
- [x] PermRequest snapshots show permission type, details, y/n/Esc hints
- [x] Snapshot files committed to version control
