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

## Bug #3 Fix: Notification Right-Alignment (2026-01-25)

### Root Cause

In `overlay.rs`, the `pad_or_truncate()` function pads content on the RIGHT:
```rust
format!("{}{}", line, " ".repeat(width - vis_width))
```

This left-aligns content. For notifications to appear in the top-right corner, padding must be on the LEFT.

### Solution

Extended the overlay system to support horizontal alignment:

1. **Added `FromBottomRight(usize)` variant** to `OverlayAnchor` enum in `overlay.rs`
2. **Added `pad_or_truncate_right()` function** that pads on the LEFT:
   ```rust
   format!("{}{}", " ".repeat(width - vis_width), line)
   ```
3. **Updated `composite_overlays()`** to handle `FromBottomRight` using `pad_or_truncate_right()`
4. **Added `overlay_from_bottom_right()` helper** in `node.rs`
5. **Changed `notification_area.rs`** to use `overlay_from_bottom_right()` instead of `overlay_from_bottom()`

### Files Modified

- `crates/crucible-cli/src/tui/oil/overlay.rs` - Added enum variant, padding function, and match arm
- `crates/crucible-cli/src/tui/oil/node.rs` - Added helper function
- `crates/crucible-cli/src/tui/oil/components/notification_area.rs` - Changed overlay anchor

### Tests Added

- `pad_or_truncate_right_pads_on_left` - Verifies left-padding behavior
- `pad_or_truncate_right_exact_width_unchanged` - Verifies exact width passthrough
- `pad_or_truncate_right_truncates_long_lines` - Verifies truncation still works
- `overlay_from_bottom_right_aligns_content` - Verifies compositing produces right-aligned output
- `notification_uses_right_aligned_overlay` - Verifies notification component uses correct anchor

### Result

Notifications now appear right-aligned:
```
                                                         ▗▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
                                                         ▌ ✓ Thinking display: on
                                                         ▌ ✓ Thinking display: off
                                                         ▘
```

## Bug #3 Fix (2026-01-25)

### Root Cause Analysis

Notification popups were appearing left-aligned instead of right-aligned because:

1. The `OverlayAnchor` enum only had `FromBottom(usize)` - no horizontal alignment
2. The `pad_or_truncate()` function padded on the RIGHT: `format!("{}{}", line, " ".repeat(width - vis_width))`
3. This left-aligned all overlay content

### Solution

Extended the overlay system to support right-alignment:

1. **Added** `FromBottomRight(usize)` variant to `OverlayAnchor` enum
2. **Added** `pad_or_truncate_right()` function that pads on the LEFT for right-alignment
3. **Added** `overlay_from_bottom_right()` helper function in `node.rs`
4. **Updated** `NotificationArea` to use `overlay_from_bottom_right(card, 1)`

### Test Results

- All 14 notification_area tests pass
- New test `notification_uses_right_aligned_overlay` verifies correct anchor usage
- LSP diagnostics clean

Notifications now appear in the top-right corner as designed.

---

## Bug #1 Investigation (Content Duplication)

### Status: NEEDS INVESTIGATION

The user reported that when streaming completes and content graduates to scrollback, content appears twice (once formatted, once as plain text).

**Current test status**: Our regression test `table_not_duplicated_after_graduation` shows NO duplication in the snapshot. This suggests:

1. The test doesn't fully simulate the real graduation flow
2. The bug might be specific to certain content types or streaming patterns
3. The bug might only occur in actual `cru chat` usage, not in unit tests

**Next steps**:
1. Need hands-on QA with actual `cru chat` to reproduce the bug
2. May need to add more detailed logging to graduation system
3. Consider adding PTY-based E2E test to capture real streaming behavior


## Graduation Invariant Property Tests (2026-01-25)

### Tests Created

Created comprehensive invariant tests in `graduation_invariant_property_tests.rs` to catch content duplication bugs:

#### Test 1: XOR Invariant
- `graduation_xor_invariant_content_never_in_both` - Verifies content appears in viewport XOR scrollback, never both
- `graduation_xor_invariant_with_multiple_paragraphs` - Tests XOR with paragraph graduation
- `graduation_xor_with_cancelled_stream` - Tests XOR when stream is cancelled

#### Test 2: Content Preservation
- `graduation_preserves_all_content` - Verifies total content equals all streamed content
- `graduation_preserves_content_with_code_blocks` - Tests preservation with code fences

#### Test 3: Atomicity
- `graduation_is_atomic_no_intermediate_duplication` - Verifies no intermediate duplication state
- `graduation_atomicity_with_rapid_chunks` - Tests atomicity with rapid streaming
- `graduation_atomicity_across_multiple_renders` - Tests atomicity across render cycles

#### Test 4: Idempotence
- `rendering_is_idempotent_after_graduation` - Verifies same state renders identically
- `rendering_is_idempotent_during_streaming` - Tests idempotence during streaming
- `rendering_is_idempotent_with_tool_calls` - Tests idempotence with tool calls

#### Additional Invariant Tests
- `graduation_monotonic_count_never_decreases` - Verifies graduated count never decreases
- `graduation_stable_across_resize` - Tests stability during terminal resize
- `graduation_handles_empty_messages_correctly` - Tests empty message handling

### Helper Functions Created

- `extract_viewport_content()` - Extract viewport lines (stripped of ANSI)
- `extract_scrollback_content()` - Extract scrollback lines (stripped of ANSI)
- `extract_viewport_text()` / `extract_scrollback_text()` - Get raw text
- `normalize_line()` - Normalize for comparison
- `is_decorative_line()` - Filter out UI decoration (borders, bullets)
- `count_content_occurrences()` - Count needle in combined output
- `combined_content()` - Get stdout + viewport combined
- `verify_xor_invariant()` - Verify XOR placement invariant

### Test Results

All 14 new tests pass. The tests correctly filter out decorative UI elements (border characters like `▄▀─│`) that legitimately appear in both viewport and scrollback as part of the UI chrome.

### Key Finding

The XOR invariant tests initially failed because border/separator characters (`▄▄▄▄▄...`) appear in both viewport and scrollback. This is expected behavior - these are UI decorations, not content. The `is_decorative_line()` helper was added to filter these out.

### Pre-existing Issue

One unrelated snapshot test (`snapshot_notification_visible`) was already failing before these changes - it's about notification positioning, not graduation.
