# Sprint 3: Integrate Existing Managers

**Goal:** Reduce `runner.rs` from 3,381 → ~800 lines by integrating existing managers

**Risk Level:** HIGH (affects entire event loop)

**Managers to Integrate:**
1. ✅ `StreamingManager` (2.6k) - Manage streaming state
2. ✅ `SelectionManager` (2.2k) - Manage selection, clipboard, mouse mode
3. ✅ `HistoryManager` (2.2k) - Manage command history navigation
4. ✅ `InputModeManager` (1.4k) - Manage rapid input, paste detection

---

## Current State Analysis

### RatatuiRunner Fields (needs refactoring):

**Streaming-related** (should use StreamingManager):
- `is_streaming: bool`
- `streaming_parser: Option<StreamingParser>`
- `streaming_task: Option<...>`
- `streaming_rx: Option<...>`

**Selection-related** (should use SelectionManager):
- `mouse_capture_enabled: bool`
- `selection: SelectionState`
- `selection_cache: SelectableContentCache`

**History-related** (should use HistoryManager):
- `history: Vec<String>`
- `history_index: Option<usize>`
- `history_saved_input: String`

**Input mode-related** (should use InputModeManager):
- `pending_pastes: Vec<PastedContent>`
- `rapid_input_buffer: String`
- `last_key_time: Option<Instant>`

---

## Sprint 3 Tasks

### Task 3.1: Add Manager Fields (30 min)
**Goal:** Add manager instances to RatatuiRunner without breaking existing code

**Steps:**
1. Add manager fields at the end of RatatuiRunner struct
2. Initialize managers in `new()` method
3. Keep existing fields for now (gradual migration)
4. Run tests to ensure no breakage

**Expected outcome:**
- Managers initialized and ready
- Zero functional changes
- All tests passing

---

### Task 3.2: Delegate Streaming (45 min)
**Goal:** Move streaming-related logic to StreamingManager

**Steps:**
1. Identify all code using `is_streaming`, `streaming_parser`
2. Create methods in StreamingManager for these operations
3. Replace direct field access with manager method calls
4. Test streaming functionality

**Code patterns to replace:**
```rust
// OLD:
if self.is_streaming {
    self.streaming_parser.as_mut().map(|p| p.parse(&delta));
}

// NEW:
if self.streaming_manager.is_streaming() {
    self.streaming_manager.parse_delta(&delta);
}
```

**Expected outcome:**
- Streaming logic delegated to StreamingManager
- ~100 lines removed from runner.rs

---

### Task 3.3: Delegate Selection (45 min)
**Goal:** Move selection logic to SelectionManager

**Steps:**
1. Identify all code using `mouse_capture_enabled`, `selection`, `selection_cache`
2. Add methods to SelectionManager for mouse/clipboard operations
3. Replace direct field access with manager method calls
4. Test mouse selection and clipboard copy

**Code patterns to replace:**
```rust
// OLD:
self.mouse_capture_enabled = !self.mouse_capture_enabled;
self.selection.start(pos);

// NEW:
self.selection_manager.toggle_mouse_mode();
self.selection_manager.start_selection(pos);
```

**Expected outcome:**
- Selection logic delegated to SelectionManager
- ~50 lines removed from runner.rs

---

### Task 3.4: Delegate History (45 min)
**Goal:** Move history navigation logic to HistoryManager

**Steps:**
1. Identify all code using `history`, `history_index`, `history_saved_input`
2. HistoryManager already has prev() and next() methods
3. Replace direct field access with manager method calls
4. Test history navigation (Ctrl+Up/Down)

**Code patterns to replace:**
```rust
// OLD:
if let Some(idx) = self.history_index {
    if idx > 0 {
        self.history_index = Some(idx - 1);
        // ... update input
    }
}

// NEW:
if let Some(entry) = self.history_manager.prev(&current_input) {
    // ... update input
}
```

**Expected outcome:**
- History logic delegated to HistoryManager
- ~80 lines removed from runner.rs

---

### Task 3.5: Delegate Input Mode (45 min)
**Goal:** Move input mode logic to InputModeManager

**Steps:**
1. Identify all code using `pending_pastes`, `rapid_input_buffer`, `last_key_time`
2. Add methods to InputModeManager for paste detection
3. Replace direct field access with manager method calls
4. Test paste detection and rapid input

**Code patterns to replace:**
```rust
// OLD:
if let Some(last_time) = self.last_key_time {
    if last_time.elapsed() < Duration::from_millis(30) {
        self.rapid_input_buffer.push(c);
    }
}

// NEW:
if self.input_mode_manager.is_rapid_input() {
    self.input_mode_manager.push_char(c);
}
```

**Expected outcome:**
- Input mode logic delegated to InputModeManager
- ~60 lines removed from runner.rs

---

### Task 3.6: Remove Old Fields (30 min)
**Goal:** Clean up old fields after delegation is complete

**Steps:**
1. Remove old fields from RatatuiRunner struct
2. Update constructor to not initialize old fields
3. Search for any remaining direct field access
4. Final test run

**Expected outcome:**
- Old fields removed
- Clean delegation to managers
- ~290 additional lines removed from runner.rs

---

## Success Criteria

**Before stopping:**
- [ ] All 4 managers integrated
- [ ] Old fields removed
- [ ] All tests passing (1291+ tests)
- [ ] runner.rs reduced by ~2,500+ lines
- [ ] No compilation warnings
- [ ] Git commits after each task

**Test Coverage:**
- Streaming: Manual test with agent chat
- Selection: Manual test with mouse selection
- History: Manual test with Ctrl+Up/Down
- Input Mode: Manual test with paste

---

## Rollback Plan

If any task breaks functionality:
1. `git revert HEAD` to undo the commit
2. Analyze what went wrong
3. Create smaller, incremental changes
4. Add more tests before trying again

---

## Estimated Time

- Task 3.1: 30 min (add fields)
- Task 3.2: 45 min (streaming)
- Task 3.3: 45 min (selection)
- Task 3.4: 45 min (history)
- Task 3.5: 45 min (input mode)
- Task 3.6: 30 min (cleanup)

**Total: ~4 hours**

---

## Next Actions

**START HERE:** Task 3.1 - Add manager fields to RatatuiRunner
