# Rendering Bugs Investigation

## Reported Issues

### 1. Content Duplication After Graduation

**Symptom**: When streaming content completes and graduates to scrollback, content appears twice.

**Example**: Table content appears once as a formatted table, then again as plain text/bullets below it.

**Hypothesis**: The graduation system is not properly filtering already-graduated content, causing it to render in both viewport AND scrollback.

**Files to investigate**:
- `crates/crucible-cli/src/tui/oil/graduation.rs` - Graduation logic
- `crates/crucible-cli/src/tui/oil/graduation_invariant_tests.rs` - XOR placement invariant
- `crates/crucible-cli/src/tui/oil/viewport_cache.rs` - Viewport filtering

**Test to write**: Stream a message with a table, complete it, verify content appears exactly once.

---

### 2. Table Cell Spacing Lost

**Symptom**: Multi-line content in table cells gets split incorrectly, with spacing lost.

**Example**:
```
│ • Your notes =      │
│ memory – embed      │
```

The bullet point "• Your notes = memory" is split across two lines.

**Hypothesis**: Text wrapping in table cells doesn't preserve bullet point integrity.

**Files to investigate**:
- `crates/crucible-cli/src/tui/oil/markdown.rs` - `wrap_text()` function (line 913)
- `crates/crucible-cli/src/tui/oil/markdown.rs` - `render_table_data_row()` (line 863)

**Test to write**: Render a table with bullet points in cells, verify bullets stay with their text.

---

### 3. Notification Popup Left-Aligned

**Symptom**: Notification boxes appear on the left side of screen instead of right-aligned.

**Example**:
```
▗▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
▌ ✓ Thinking display: on
▌ ✓ Thinking display: off
▘
```

This should appear in the top-right corner.

**Hypothesis**: Notification rendering doesn't calculate right-alignment offset.

**Files to investigate**:
- Search for notification/toast rendering code
- Check for "Thinking display" message generation
- Look for box drawing characters ▗▄▌▘

**Test to write**: Trigger a notification, verify box appears in right portion of screen (column > 40 for 80-wide terminal).

---

### 4. Spacing Lost Between Graduated Elements

**Symptom**: When multiple elements (paragraphs, lists, tables) graduate together, blank lines between them disappear.

**Hypothesis**: Graduation system strips blank lines or doesn't preserve spacing between elements.

**Files to investigate**:
- `crates/crucible-cli/src/tui/oil/graduation.rs` - `format_stdout_delta()` (line 86)
- `crates/crucible-cli/src/tui/oil/markdown.rs` - Block spacing logic

**Test to write**: Stream content with multiple paragraphs separated by blank lines, verify spacing preserved after graduation.

---

## Next Steps

1. Fix the regression test file to compile
2. Run tests to confirm they reproduce the issues
3. Use failing tests to guide debugging
4. Fix the actual bugs
5. Verify tests pass

## Test File Status

Created: `crates/crucible-cli/src/tui/oil/tests/rendering_regression_tests.rs`

**Issues**:
- Wrong imports (need to understand AppHarness API)
- Type annotation errors
- Need to study existing snapshot tests for patterns

**Reference tests**:
- `chat_app_snapshot_tests.rs` - How to use AppHarness
- `graduation_tests.rs` - How to test graduation
- `graduation_invariant_tests.rs` - XOR placement invariant
