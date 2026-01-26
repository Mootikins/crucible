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

## Test Results (2026-01-25)

### Regression Tests Created ✅

Created 5 regression tests in `rendering_regression_tests.rs`:
1. `table_not_duplicated_after_graduation` - Verifies no duplication
2. `table_cell_wrapping_preserves_spacing` - **CAPTURES BUG #2**
3. `no_duplication_during_graduation_transition` - Verifies atomic graduation
4. `spacing_preserved_between_graduated_elements` - Verifies spacing
5. `complex_markdown_with_table` - **CAPTURES BUG #2 with real user example**

### Bug #2 Confirmed in Snapshots

The `complex_markdown_table` snapshot shows the exact bug:

```
│ Core ideas          │ • Markdown sessions – every chat is a file      │   
├─────────────────────┼─────────────────────────────────────────────────┤   
│ • Your notes =      │                                                 │   
│ memory – embed      │                                                 │   
│ every block         │                                                 │   
```

**Expected**: "• Your notes = memory – embed every block" should be in the RIGHT column
**Actual**: It's split across multiple rows in the LEFT column

### Root Cause Hypothesis

The table parser is treating `<br>` tags or newlines in cell content incorrectly:
- Original markdown: `| Core ideas | • Markdown sessions – every chat is a file<br>• Your notes = memory – embed every block |`
- The `<br>` should keep both bullets in the same cell
- Instead, the second bullet is being parsed as a new row

**Next Steps**:
1. Investigate `markdown.rs` table rendering (lines 671-940)
2. Check how `<br>` tags are handled in table cells
3. Fix the cell content parsing to preserve inline breaks
4. Re-run tests to verify fix

## Bug #2 Fix (2026-01-25)

### Root Cause Analysis

The bug was caused by `normalize_br_tags()` being called BEFORE markdown parsing:

1. Input: `| Core ideas | • Markdown sessions – every chat is a file<br>• Your notes = memory |`
2. After `normalize_br_tags()`: `| Core ideas | • Markdown sessions – every chat is a file  \n• Your notes = memory |`
3. The newline (`\n`) in the middle of the table row breaks the table structure
4. The markdown parser interprets the newline as ending the row
5. The second bullet point becomes a new row with content in the wrong column

### Solution

Instead of normalizing `<br>` tags before parsing (which breaks table structure), handle them during rendering:

1. **Removed** `normalize_br_tags()` calls from `markdown_to_node_styled()` and `markdown_to_node_with_widths()`
2. **Added** `<br>` handling in `render_node()` for `Text` nodes - splits on `<br>` and flushes lines
3. **Added** `<br>` handling in `extract_all_text()` for table cells - converts `<br>` to `\n`
4. **Updated** `wrap_text()` to handle newlines by splitting first, then wrapping each segment

### Key Changes

- `markdown.rs`: Added module-level `BR_TAG_REGEX` for shared use
- `markdown.rs`: `render_node()` Text handling now splits on `<br>` and flushes lines
- `markdown.rs`: `extract_all_text()` converts `<br>` to `\n` for table cells
- `markdown.rs`: `wrap_text()` splits on newlines before wrapping

### Test Results

- All 5 regression tests pass
- All 1592 CLI tests pass
- Snapshot updated to show correct behavior:
  ```
  │ Core     │ • Markdown sessions – every chat is a file                 │   
  │ ideas    │ • Your notes = memory – embed every block                  │   
  ```

Both bullet points now stay in the same cell, with the `<br>` creating a line break within the cell.
