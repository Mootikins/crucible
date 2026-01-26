# Fix TUI Graduation Bugs: Duplication and Newline Padding

## Context

### Original Request
Fix two TUI rendering bugs:
1. Duplicated graduated nodes when streaming completes (especially with tool calls)
2. Incorrect newline padding - `<p>` following tool call lacks padding during streaming but fixes upon completion

### Interview Summary
**Key Discussions**:
- Bug 1: `text_segments_graduated_count` pre-graduates FIRST N segments, but `finalize_segments()` APPENDS graduated content at END
- Bug 2: `render_streaming()` uses `col()` without `ElementKind::wants_blank_line_before()` rules; graduation applies them

**Research Findings**:
- Key files identified: `viewport_cache.rs`, `chat_app.rs`, `graduation.rs`, `node.rs`
- Existing test coverage: `streaming_to_final_no_stdout_duplication`, `graduation_doesnt_cause_duplication`
- Test infrastructure: `TestRuntime`, `cargo nextest run`, snapshot tests with insta

### Metis Review
**Identified Gaps** (addressed):
- Thinking segments interleaved: tracked as edge case
- Multiple graduated blocks: index tracking handles this
- Subagent flush behavior: same as tool call, tested
- Edge cases: tool call as first/last, multiple tool calls, sequential tool calls

---

## Work Objectives

### Core Objective
Fix content duplication after streaming completion and ensure consistent newline spacing between streaming and graduated states.

### Concrete Deliverables
- Modified `StreamingBuffer` struct with index-based graduation tracking
- Modified `finalize_segments()` to record graduated indices
- Modified `complete_streaming()` to use indices for `pre_graduate_keys`
- Modified `render_streaming()` with `ElementKind`-aware spacing
- Regression tests for both bugs
- All existing tests pass

### Definition of Done
- [ ] `cargo nextest run -p crucible-cli --profile ci` passes (100%)
- [ ] New tests verify bug fixes with specific scenarios
- [ ] No content duplication in text→tool→text scenarios
- [ ] Streaming view spacing matches post-graduation spacing

### Must Have
- Index-based tracking replaces count-based tracking
- Spacing rules applied during streaming render
- Failing tests written BEFORE fixes
- XOR invariant preserved (content in stdout OR viewport, never both)

### Must NOT Have (Guardrails)
- NO changes to `ElementKind` variants or spacing rules
- NO modifications to graduation system beyond index tracking
- NO new message types or segment types
- NO changes to `render_items()` or non-streaming paths
- NO changes to `StreamingCompleteResult` struct definition
- NO "optimizations" or refactoring beyond bug fixes

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (cargo nextest, insta snapshots, proptest)
- **User wants tests**: TDD - write failing tests FIRST
- **Framework**: `cargo nextest run` with `TestRuntime` harness

### TDD Workflow
Each bug follows RED-GREEN-REFACTOR:

**RED**: Write failing test that reproduces the bug
- Test file: `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs`
- Test command: `cargo nextest run -p crucible-cli test_name`
- Expected: FAIL (bug exists)

**GREEN**: Implement minimum fix to pass
- Command: `cargo nextest run -p crucible-cli test_name`
- Expected: PASS

**REFACTOR**: Clean up while keeping green
- Command: `cargo nextest run -p crucible-cli --profile ci`
- Expected: PASS (all tests)

### Manual Verification
For visual confirmation after implementation:
- Build: `cargo build -p crucible-cli`
- Run: `./target/debug/cru chat`
- Test scenario: Ask a question that triggers tool call followed by text response
- Verify: No duplication, consistent spacing during and after streaming

---

## Task Flow

```
Task 0 (Baseline) ────────────────────────────────────────┐
                                                          │
Task 1 (Bug 1 Test) ──────────────────────────────────────┤
                                                          ▼
Task 2 (Bug 1 Fix) ───────────────────────────────────────┐
                                                          │
Task 3 (Bug 2 Test) ──────────────────────────────────────┤
                                                          ▼
Task 4 (Bug 2 Fix) ───────────────────────────────────────┐
                                                          ▼
Task 5 (Regression Suite)
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| - | None | Sequential dependencies |

| Task | Depends On | Reason |
|------|------------|--------|
| 1 | 0 | Baseline must pass first |
| 2 | 1 | Test must exist and fail |
| 3 | 2 | Bug 1 fix may affect Bug 2 test |
| 4 | 3 | Test must exist and fail |
| 5 | 4 | All fixes complete |

---

## TODOs

- [ ] 0. Baseline Verification

  **What to do**:
  - Run existing test suite to confirm current state
  - Document any pre-existing failures

  **Must NOT do**:
  - Make any code changes
  - Skip this step

  **Parallelizable**: NO (must run first)

  **References**:
  - `crates/crucible-cli/src/tui/oil/tests/` - existing test modules

  **Acceptance Criteria**:

  **Commands to run**:
  - [ ] `cargo nextest run -p crucible-cli --profile ci 2>&1 | tail -20`
  - [ ] Expected: All tests pass (or document known failures)
  - [ ] Capture baseline test count for regression comparison

  **Commit**: NO (no changes)

---

- [ ] 1. Write Failing Test for Bug 1 (Duplication)

  **What to do**:
  - Add test `pregrad_keys_target_graduated_content_not_first_segment` to `tool_ordering_tests.rs`
  - Test scenario: Text("BEFORE\n\n") → ToolCall → Text("AFTER\n\n") → StreamComplete
  - Assert: "BEFORE" NOT in `pre_graduate_keys`, "AFTER" IS in `pre_graduate_keys`
  - Also assert each content appears exactly once in final stdout

  **Must NOT do**:
  - Fix the bug in this task
  - Modify any non-test files

  **Parallelizable**: NO (depends on 0)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs:994-1059` - `streaming_to_final_no_stdout_duplication` test structure
  - `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs:875-913` - `graduation_doesnt_cause_duplication` scenario setup

  **API References**:
  - `crates/crucible-cli/src/tui/oil/viewport_cache.rs:14` - `StreamingCompleteResult.pre_graduate_keys`
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:927-928` - `take_pending_pre_graduate_keys()`

  **Test References**:
  - `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs:1030-1035` - How to capture and assert on pre_graduate_keys

  **Acceptance Criteria**:

  **Test must verify**:
  - [ ] Create test that captures `pre_graduate_keys` after StreamComplete
  - [ ] Assert "BEFORE" segment key is NOT in pre_graduate_keys
  - [ ] Assert "AFTER" segment key IS in pre_graduate_keys
  - [ ] Assert no duplication in final stdout

  **Execution verification**:
  - [ ] `cargo nextest run -p crucible-cli pregrad_keys_target_graduated`
  - [ ] Expected: FAIL (test exists, reproduces bug)
  - [ ] Failure message should indicate wrong content was pre-graduated

  **Commit**: YES
  - Message: `test(tui): add failing test for graduation index tracking bug`
  - Files: `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli pregrad_keys_target_graduated` (expect FAIL)

---

- [ ] 2. Fix Bug 1: Index-Based Graduation Tracking

  **What to do**:
  - Add `graduated_segment_indices: Vec<usize>` field to `StreamingBuffer` struct
  - Initialize to empty vec in `StreamingBuffer::new()`
  - In `finalize_segments()`: record `self.segments.len()` BEFORE pushing graduated content
  - In `complete_streaming()`: check if segment index is in `graduated_segment_indices` instead of `msg_counter < graduated_count`
  - Remove `text_segments_graduated_count` field (use `lsp_find_references` first)

  **Must NOT do**:
  - Change `StreamingCompleteResult` struct definition
  - Modify graduation system beyond this tracking
  - Change any other segment handling logic

  **Parallelizable**: NO (depends on 1)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/viewport_cache.rs:715-725` - `StreamingBuffer` struct definition
  - `crates/crucible-cli/src/tui/oil/viewport_cache.rs:790-819` - `finalize_segments()` implementation

  **API References**:
  - `crates/crucible-cli/src/tui/oil/viewport_cache.rs:579-635` - `complete_streaming()` where pre_graduate_keys are built
  - `crates/crucible-cli/src/tui/oil/viewport_cache.rs:584` - Current `text_segments_graduated_count` usage

  **Documentation References**:
  - `crates/crucible-cli/AGENTS.md` - CLI architecture rules

  **Acceptance Criteria**:

  **Implementation verification**:
  - [ ] `StreamingBuffer` has `graduated_segment_indices: Vec<usize>` field
  - [ ] `finalize_segments()` pushes index before appending graduated content
  - [ ] `complete_streaming()` checks `graduated_segment_indices.contains(&text_seg_idx)`
  - [ ] `text_segments_graduated_count` field removed (no references remain)

  **Test verification**:
  - [ ] `cargo nextest run -p crucible-cli pregrad_keys_target_graduated`
  - [ ] Expected: PASS
  - [ ] `cargo nextest run -p crucible-cli streaming_to_final`
  - [ ] Expected: PASS (existing test still works)
  - [ ] `cargo nextest run -p crucible-cli graduation`
  - [ ] Expected: PASS (all graduation tests)

  **Commit**: YES
  - Message: `fix(tui): track graduated segment indices instead of count`
  - Files: `crates/crucible-cli/src/tui/oil/viewport_cache.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli graduation`

---

- [ ] 3. Write Failing Test for Bug 2 (Newline Padding)

  **What to do**:
  - Add test `streaming_spacing_matches_graduation_spacing` to `tool_ordering_tests.rs`
  - Test scenario: ToolCall completion followed by text paragraph
  - Assert: Blank line exists between tool call and paragraph DURING streaming
  - Compare streaming output with post-graduation output for consistency

  **Must NOT do**:
  - Fix the bug in this task
  - Modify any non-test files

  **Parallelizable**: NO (depends on 2)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/tests/graduation_tests.rs:96` - `blank_line_between_graduated_content_and_viewport` structure
  - `crates/crucible-cli/src/tui/oil/tests/property_tests.rs:664-680` - `has_blank_line_between()` helper function

  **API References**:
  - `crates/crucible-cli/src/tui/oil/node.rs:14-24` - `ElementKind::wants_blank_line_before()` rules

  **Test References**:
  - `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs:527-610` - `tool_call_position_during_graduation` for rendering during streaming

  **Acceptance Criteria**:

  **Test must verify**:
  - [ ] Render app during streaming (after tool call, during text streaming)
  - [ ] Find tool call output and subsequent paragraph in rendered output
  - [ ] Assert blank line exists between them (not just single newline)

  **Execution verification**:
  - [ ] `cargo nextest run -p crucible-cli streaming_spacing_matches`
  - [ ] Expected: FAIL (test exists, reproduces bug)
  - [ ] Failure message should indicate missing blank line during streaming

  **Commit**: YES
  - Message: `test(tui): add failing test for streaming newline padding`
  - Files: `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli streaming_spacing_matches` (expect FAIL)

---

- [ ] 4. Fix Bug 2: Apply Spacing in render_streaming()

  **What to do**:
  - In `render_streaming()`, add `prev_kind: Option<ElementKind>` tracking variable
  - Before each `nodes.push()`, check `kind.wants_blank_line_before(prev_kind)`
  - If true, insert `text("")` node before the actual node
  - Update `prev_kind` after each segment is processed
  - Handle: segments loop (ToolCall, Text), graduated_blocks loop (Block), in_progress (Block)

  **Must NOT do**:
  - Change `ElementKind` variants or spacing rules
  - Modify graduation spacing (format_stdout_delta)
  - Add spacing to non-streaming render paths

  **Parallelizable**: NO (depends on 3)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/graduation.rs:92-117` - `format_stdout_delta()` spacing application pattern
  - `crates/crucible-cli/src/tui/oil/node.rs:14-24` - `wants_blank_line_before()` implementation

  **API References**:
  - `crates/crucible-cli/src/tui/oil/chat_app.rs:3027-3154` - `render_streaming()` implementation
  - `crates/crucible-cli/src/tui/oil/node.rs:6-11` - `ElementKind` enum

  **Documentation References**:
  - `crates/crucible-cli/src/tui/oil/node.rs:14-24` - Spacing rules documentation in match arms

  **Acceptance Criteria**:

  **Implementation verification**:
  - [ ] `render_streaming()` has `prev_kind: Option<ElementKind>` variable
  - [ ] Spacing check before each segment render
  - [ ] `text("")` inserted when `wants_blank_line_before()` returns true
  - [ ] `prev_kind` updated: ToolCall segments → `Some(ToolCall)`, Text/graduated → `Some(Block)`

  **Test verification**:
  - [ ] `cargo nextest run -p crucible-cli streaming_spacing_matches`
  - [ ] Expected: PASS
  - [ ] `cargo nextest run -p crucible-cli tool_call_position`
  - [ ] Expected: PASS (existing tests still work)

  **Commit**: YES
  - Message: `fix(tui): apply ElementKind spacing rules during streaming render`
  - Files: `crates/crucible-cli/src/tui/oil/chat_app.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli streaming_spacing`

---

- [ ] 5. Full Regression Suite and Edge Cases

  **What to do**:
  - Run complete test suite
  - Add edge case tests if not already covered:
    - Tool call as first segment (no text before)
    - Multiple tool calls: text→tool→text→tool→text
    - Sequential tool calls (tool→tool, should have no blank line)
    - Subagent interleaved with tool calls
  - Verify no performance regression (optional: add timing assertion)

  **Must NOT do**:
  - Make additional code changes unless tests fail
  - Add tests for unrelated functionality

  **Parallelizable**: NO (final verification)

  **References**:

  **Pattern References**:
  - `crates/crucible-cli/src/tui/oil/tests/graduation_invariant_property_tests.rs` - Property test patterns
  - `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs` - Tool ordering test patterns

  **Test References**:
  - `justfile` - `just ci` command for full CI validation

  **Acceptance Criteria**:

  **Full suite verification**:
  - [ ] `cargo nextest run -p crucible-cli --profile ci`
  - [ ] Expected: 100% pass, no regressions
  - [ ] `cargo clippy -p crucible-cli -- -D warnings`
  - [ ] Expected: No warnings

  **Edge case verification** (add tests if missing):
  - [ ] Tool call first: `cargo nextest run -p crucible-cli tool_first`
  - [ ] Multiple tools: `cargo nextest run -p crucible-cli multiple_tools`
  - [ ] Sequential tools: `cargo nextest run -p crucible-cli sequential_tools`

  **Commit**: YES (if edge case tests added)
  - Message: `test(tui): add edge case tests for graduation and spacing`
  - Files: `crates/crucible-cli/src/tui/oil/tests/tool_ordering_tests.rs`
  - Pre-commit: `cargo nextest run -p crucible-cli --profile ci`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `test(tui): add failing test for graduation index tracking bug` | tests/tool_ordering_tests.rs | Test exists, fails |
| 2 | `fix(tui): track graduated segment indices instead of count` | viewport_cache.rs | Test passes |
| 3 | `test(tui): add failing test for streaming newline padding` | tests/tool_ordering_tests.rs | Test exists, fails |
| 4 | `fix(tui): apply ElementKind spacing rules during streaming render` | chat_app.rs | Test passes |
| 5 | `test(tui): add edge case tests for graduation and spacing` | tests/tool_ordering_tests.rs | All pass |

---

## Success Criteria

### Verification Commands
```bash
# Full suite - must pass
cargo nextest run -p crucible-cli --profile ci

# Specific bug 1 verification
cargo nextest run -p crucible-cli pregrad_keys_target_graduated

# Specific bug 2 verification  
cargo nextest run -p crucible-cli streaming_spacing_matches

# No clippy warnings
cargo clippy -p crucible-cli -- -D warnings
```

### Final Checklist
- [ ] All "Must Have" present (index tracking, spacing rules)
- [ ] All "Must NOT Have" absent (no ElementKind changes, no graduation system changes)
- [ ] Bug 1 test: content after tool call correctly pre-graduated
- [ ] Bug 2 test: streaming spacing matches post-graduation spacing
- [ ] All existing tests pass
- [ ] No clippy warnings
