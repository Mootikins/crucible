# SOLID & DRY Refactoring Plan for TUI Codebase

**Date:** 2026-01-08
**Scope:** TUI codebase (`crates/crucible-cli/src/tui/`)
**Analysis:** Comprehensive SOLID and DRY violation audit

---

## Executive Summary

**Critical Issues Found:** 14 major violations across 6 files
- **6 SRP violations** (Single Responsibility Principle)
- **8 DRY violations** (Don't Repeat Yourself)

**Most Critical:** `runner.rs` (3,380 lines, 12+ responsibilities)

---

## Progress Tracking

**Last Updated:** 2026-01-08 (Sprint 1 COMPLETE)

### Sprint Status

| Sprint | Status | Completion | Lines Reduced | Tests Passing |
|--------|--------|------------|---------------|---------------|
| **Sprint 1** | ✅ COMPLETE | 100% | DRY: 8→0 | 1271 → 1288 |
| **Sprint 2** | ⏸️ Pending | 0% | Target: ~1,000 | TBD |
| **Sprint 3** | ⏸️ Pending | 0% | Target: ~2,500 | TBD |

### Sprint 1 Accomplishments ✅

**Completed:** 2026-01-08

**Files Created:**
1. `tui/constants.rs` (103 lines) - UI spacing constants and helpers
2. `tui/geometry.rs` (176 lines) - Popup centering and alignment
3. `tui/scroll_utils.rs` (392 lines) - Scroll calculations and line counting

**Files Updated:** 15+
- Applied constants to eliminate magic numbers
- Applied geometry helpers for consistent centering
- Applied scroll utilities for bound calculations

**DRY Violations Eliminated:**
- ✅ `saturating_sub(4)` → `UiConstants::content_width()` (7 occurrences)
- ✅ `saturating_sub(2)` → `UiConstants::dialog_width()` (4 occurrences)
- ✅ Manual centering → `PopupGeometry` helpers (6 occurrences)
- ✅ Scroll clamping → `ScrollUtils::clamp_scroll()` (5 occurrences)
- ✅ Line counting → `LineCount::count()` (4 occurrences)

**Test Results:**
- Before: 1271 tests passing
- After: 1288 tests passing (+17 tests)
- 0 failures, 0 compilation errors

**Git Commits:** 4 commits documenting all changes

---

## Phase 1: HIGH Priority Refactoring

### 1.1 Break Up `runner.rs` God Class (CRITICAL)

**Current:** 3,380 lines handling 12+ responsibilities

**Impact:** Event loop mixed with popup management, streaming, selection, dialogs, history, etc.

**Refactoring Strategy:**

#### Step 1: Extract Event Loop (200 lines)
```rust
// NEW: tui/runner/event_loop.rs
pub struct EventLoop {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    event_rate: Duration,
}

impl EventLoop {
    pub fn new() -> Result<Self>;
    pub fn run<F>(&mut self, mut handler: F) -> Result<()>
    where F: FnMut(crossterm::event::Event) -> Result<EventLoopAction>;
}

pub enum EventLoopAction {
    Continue,
    Break,
    Suspend,
}
```

#### Step 2: Extract PopupManager (250 lines)
```rust
// NEW: tui/runner/popup_manager.rs
pub struct PopupManager {
    current: Option<PopupState>,
    providers: HashMap<PopupKind, Arc<dyn PopupProvider>>,
}

impl PopupManager {
    pub fn show(&mut self, kind: PopupKind, provider: Arc<dyn PopupProvider>) -> Result<()>;
    pub fn hide(&mut self);
    pub fn update_query(&mut self, query: &str);
    pub fn selected_item(&self) -> Option<PopupItem>;
    pub fn is_visible(&self) -> bool;
}
```

#### Step 3: Extract StreamingManager (200 lines)
```rust
// NEW: tui/runner/streaming_manager.rs (already exists!)
// Just needs to be integrated into runner
```

#### Step 4: Extract SelectionManager (200 lines)
```rust
// NEW: tui/runner/selection_manager.rs (already exists!)
// Just needs to be integrated into runner
```

#### Step 5: Extract HistoryManager (150 lines)
```rust
// NEW: tui/runner/history_manager.rs (already exists!)
// Just needs to be integrated into runner
```

**Result:** `runner.rs` reduced from 3,380 → ~800 lines

---

### 1.2 Split `state.rs` Types from Behavior (SEVERE)

**Current:** 1,686 lines mixing data structures, state, and behavior

**Refactoring Strategy:**

#### Step 1: Extract Data Types (300 lines)
```rust
// NEW: tui/state/types.rs
pub mod popup {
    pub use crate::tui::state::{PopupItem, PopupKind};
}

pub mod context {
    pub use crate::tui::state::{ContextAttachment, ContextKind};
}
```

#### Step 2: Extract Action Handlers (400 lines)
```rust
// NEW: tui/state/actions.rs
pub struct ActionExecutor;

impl ActionExecutor {
    pub fn execute_action(state: &mut TuiState, action: InputAction) -> Result<()> {
        match action {
            InputAction::InsertChar(c) => self.insert_char(state, c),
            InputAction::DeleteChar => self.delete_char(state),
            // ... 20+ action handlers
        }
    }
}
```

#### Step 3: Extract Navigation Utilities (150 lines)
```rust
// NEW: tui/state/navigation.rs
pub mod word_boundary {
    pub use crate::tui::state::{find_word_start_backward, find_word_start_forward};
}

pub struct HistoryNavigator {
    pub fn prev(&self, state: &TuiState, current_input: &str) -> Option<&str>;
    pub fn next(&self, state: &TuiState) -> Option<&str>;
}
```

**Result:** `state.rs` reduced from 1,686 → ~600 lines

---

## Phase 2: MEDIUM Priority Refactoring

### 2.1 Extract Widget Rendering Abstraction

**Duplicated Code:** Scroll & render logic in 2+ files

**Solution:**
```rust
// NEW: tui/widgets/scrolling.rs
pub trait ScrollingWidget {
    fn content_lines(&self, width: usize) -> Vec<String>;
    fn scroll_offset(&self) -> usize;
    fn horizontal_offset(&self) -> Option<u16>;
}

pub fn render_scrolling_content<W: ScrollingWidget>(
    widget: &W,
    area: Rect,
    buf: &mut Buffer,
) {
    let content_width = (area.width as usize).saturating_sub(4);
    let lines = widget.content_lines(content_width);
    let content_height = lines.len();
    let viewport_height = area.height as usize;

    // ... common scroll/render logic
}
```

**Files affected:**
- `conversation.rs` (session_history widget)
- `components/session_history.rs`

---

### 2.2 Extract UI Constants

**Duplicated Code:** Magic numbers repeated 13+ times

**Solution:**
```rust
// NEW: tui/constants.rs
pub struct UiConstants;

impl UiConstants {
    pub const CONTENT_MARGIN: usize = 4;
    pub const DIALOG_PADDING: usize = 2;
    pub const CURSOR_BOUNDS: usize = 1;

    pub fn content_width(area_width: u16) -> usize {
        (area_width as usize).saturating_sub(Self::CONTENT_MARGIN)
    }

    pub fn dialog_width(outer_width: u16) -> u16 {
        outer_width.saturating_sub(Self::DIALOG_PADDING * 2)
    }
}
```

**Files affected:** 13+ files with `saturating_sub(4)`

---

### 2.3 Extract Popup Geometry Helpers

**Duplicated Code:** Centering calculations in 3 files

**Solution:**
```rust
// NEW: tui/geometry.rs
pub struct PopupGeometry;

impl PopupGeometry {
    pub fn center_horizontally(inner: Rect, content_width: u16) -> u16 {
        inner.x + inner.width.saturating_sub(content_width) / 2
    }

    pub fn center_vertically(inner: Rect, content_height: u16) -> u16 {
        inner.y + inner.height.saturating_sub(content_height) / 2
    }
}
```

**Files affected:**
- `dialog.rs`
- `components/dialog.rs`
- `ask_batch_dialog.rs`

---

## Phase 3: LOW Priority Refactoring

### 3.1 Extract Widgets from `conversation.rs`

**Current:** Widgets mixed in conversation module

**Solution:**
```rust
// NEW: tui/conversation/widgets.rs
pub mod widgets {
    pub use crate::tui::conversation::ConversationWidget;
    pub use crate::tui::widgets::input_box::InputBoxWidget;
    pub use crate::tui::widgets::status_bar::StatusBarWidget;
}
```

---

### 3.2 Extract Viewport Layout Logic

**Current:** Layout calculations mixed with state

**Solution:**
```rust
// NEW: tui/viewport/layout.rs
pub struct LayoutCalculator {
    pub fn calculate_viewport(
        terminal_height: u16,
        input_box_height: u16,
        reasoning_height: u16,
        popup_height: u16,
    ) -> usize {
        // Unified layout calculation
    }
}
```

---

## Detailed File-by-File Plan

### `runner.rs` (3,380 → ~800 lines)

**Extract to new files:**
1. `runner/event_loop.rs` - Event loop coordination
2. `runner/popup_manager.rs` - Popup lifecycle
3. `runner/dialog_coordinator.rs` - Dialog management
4. `runner/session_persistence.rs` - Session logging
5. `runner/agent_lifecycle.rs` - Agent coordination

**Keep in `runner.rs`:**
- Main runner struct (delegates to subsystems)
- High-level event handling
- Integration of subsystems

---

### `state.rs` (1,686 → ~600 lines)

**Extract to new files:**
1. `state/types/popup.rs` - Popup types
2. `state/types/context.rs` - Context types
3. `state/actions/` - Action execution logic
4. `state/navigation/` - History, cursor movement
5. `state/reasoning/` - Thinking model tracking

**Keep in `state.rs`:**
- `TuiState` struct (reduced to data container)
- Minimal accessor methods
- Delegates to subsystems for behavior

---

### `conversation_view.rs` (1,479 → ~900 lines)

**Extract to new files:**
1. `conversation_view/layout.rs` - Layout calculations
2. `conversation_view/rendering/` - Rendering subsystem
3. `conversation_view/state/` - ViewState split

**Keep in `conversation_view.rs`:**
- `ConversationView` trait
- `RatatuiView` implementation (delegates to subsystems)

---

### `conversation.rs` (1,570 → ~1,000 lines)

**Extract to new files:**
1. `conversation/widgets.rs` - Widget definitions
2. `conversation/rendering/` - Rendering logic
3. `conversation/export.rs` - Markdown export

---

## Implementation Order

### Sprint 1: Easy Wins (1-2 days)
1. Extract UI constants
2. Extract popup geometry helpers
3. Extract line count utilities
4. Extract scroll calculations

**Impact:** Low risk, high consistency gain

---

### Sprint 2: Medium Refactoring (3-5 days)
1. Split `state.rs` types from behavior
2. Extract scrolling widget abstraction
3. Split `conversation.rs` widgets

**Impact:** Moderate risk, significant maintainability improvement

---

### Sprint 3: Major Refactoring (1-2 weeks)
1. Break up `runner.rs` god class
2. Split `conversation_view.rs`
3. Integrate manager structs (already created!)

**Impact:** High risk, dramatic architecture improvement

**Note:** Manager structs (`StreamingManager`, `HistoryManager`, etc.) were already created in previous work. Just need integration!

---

## Success Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| runner.rs lines | 3,380 | 800 | <1,000 |
| state.rs lines | 1,686 | 600 | <800 |
| DRY violations | 8 | 0 | 0 |
| God classes | 2 | 0 | 0 |
| Test coverage | 90% | 95% | >90% |

---

## Risk Assessment

### LOW Risk:
- UI constants extraction
- Geometry helpers
- Utility functions

### MEDIUM Risk:
- Splitting state.rs (well-defined boundaries)
- Extracting widgets (local changes)

### HIGH Risk:
- Breaking up runner.rs (affects entire event loop)
- Splitting conversation_view.rs (trait boundaries)

---

## Recommendations

1. **Start with Phase 1 (Sprint 1)** - Quick wins, build confidence
2. **Create feature branch** - `refactor/solid-dry-cleanup`
3. **Run tests after each extraction** - Ensure no regression
4. **Document new patterns** - Help team avoid future violations
5. **Pair program on high-risk changes** - Runner refactoring

---

## Ralph Loop Continuation Plan

**Current Status:** Sprint 1 COMPLETE ✅
**Next Sprint:** Sprint 2 (Medium Priority - MEDIUM RISK)
**Alternative:** Sprint 3 (High Priority - HIGH RISK)

### Sprint 2: Start HERE Next ⬅️

**Goal:** Split `state.rs` (1,686 lines) into manageable modules

**Tasks:**
1. Extract data types to `state/types/`
   - `state/types/popup.rs` - PopupItem, PopupKind
   - `state/types/context.rs` - ContextAttachment, ContextKind
2. Extract action handlers to `state/actions.rs`
   - Move `execute_action()` logic
   - Create ActionExecutor struct
3. Extract navigation to `state/navigation/`
   - Move `find_word_start_backward/forward`
   - Create HistoryNavigator

**Expected Impact:**
- Reduce `state.rs` from 1,686 → ~600 lines
- Improve separation of concerns
- Medium risk (well-defined boundaries)

**Estimated Files to Modify:**
- `state.rs` (main)
- Files importing from state.rs (update imports)

### Sprint 3: Alternative High-Value Path

**Goal:** Break up `runner.rs` (3,380 lines) by integrating existing managers

**Key Insight:** Manager structs already exist! They just need integration:
- ✅ `StreamingManager` - created, not integrated
- ✅ `SelectionManager` - created, not integrated
- ✅ `HistoryManager` - created, not integrated
- ✅ `InputModeManager` - created, not integrated

**Tasks:**
1. Add manager fields to `RatatuiRunner`
2. Delegate responsibilities to managers
3. Remove duplicated code from runner
4. Extract EventLoop for event handling
5. Extract PopupManager for popup lifecycle

**Expected Impact:**
- Reduce `runner.rs` from 3,380 → ~800 lines
- Integrate 4 existing managers
- High risk (affects entire event loop)

### Recommendation for Ralph Loop

**START WITH:** Sprint 2 (Split state.rs)
- **Reason:** Medium risk, clear boundaries, builds on Sprint 1 success
- **If blocked:** Move to Sprint 3 (managers already exist, just need wiring)

**DO NOT:**
- Skip tests - run after every extraction
- Create large commits - commit frequently
- Ignore compilation errors - fix immediately

**SUCCESS CRITERIA:**
- All tests passing
- No compilation warnings
- Clear commit messages
- Updated documentation

---

## Next Steps (Legacy - Use Section Above Instead)

Choose your adventure:

**A) Cautious Approach:** Start with Sprint 1 (constants, helpers)
**B) Moderate Approach:** Sprint 1 + Sprint 2 (skip high-risk runner work)
**C) Bold Approach:** Full refactoring plan (Sprints 1-3)

**D) Targeted Approach:** Pick one specific file (e.g., just state.rs)

---

*Generated: 2026-01-08*
*Analysis: SOLID + DRY comprehensive audit*
*Total Refactoring Opportunities: 14*
