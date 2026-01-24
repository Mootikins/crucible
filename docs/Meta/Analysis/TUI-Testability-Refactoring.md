# TUI Testability Refactoring Plan

Analysis of crucible-cli's TUI/oil module with recommendations for improving testability.

## Current Architecture

The TUI is a declarative Elm/Dioxus-inspired framework:

```
┌─────────────────────────────────────────────────────────────┐
│                    InkChatApp (State)                       │
│  • ViewportCache: cached items, streaming, graduation       │
│  • InputBuffer: cursor, content, history                    │
│  • Popup state: visibility, selection, filtering            │
│  • ChatMode, Model, Status, Error                           │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
                   view(&ctx) → Node tree
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│              FramePlanner (Graduation State)                │
│  • plan_graduation(): finds Static nodes to graduate        │
│  • commit_graduation(): tracks graduated keys               │
│  • format_stdout_delta(): produces stdout output            │
└─────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                     Render Layer                            │
│  • render_with_cursor_filtered(): renders with skip logic   │
│  • Filter graduation keys from viewport                     │
│  • Produce viewport content + cursor position               │
└─────────────────────────────────────────────────────────────┘
```

### Key Files

| File | Lines | Purpose |
|------|-------|---------|
| `chat_app.rs` | 2200+ | Core state, event handling, rendering mixed |
| `viewport_cache.rs` | 860+ | Streaming, graduation, cached items |
| `runtime.rs` | 370+ | GraduationState, TestRuntime |
| `planning.rs` | 180 | Frame planning and rendering |
| `components/message_list.rs` | 825 | Message rendering |
| `node.rs` | 713 | Node type system |

### Current Problems

1. **`InkChatApp` is massive** - state management, event handling, rendering mixed
2. **Business logic in render functions** - tool summarization, thinking truncation
3. **Complex state transitions** - 300+ lines of nested conditionals in `handle_key()`
4. **Popup state machine intertwined** - autocomplete detection mixed with input handling

---

## Testable Invariants

### Graduation (Already Well-Tested)

Located in `graduation_tests.rs` and `streaming_tests.rs`:

- Static nodes graduate exactly once (no duplication)
- Graduated content in stdout, not viewport
- Blank line separates graduated from viewport
- Continuation nodes append without newline
- Graduated keys unique and ordered
- Empty static nodes don't graduate
- MAX_GRADUATED_KEYS (256) bounds memory

### Scrollback/Viewport (Partially Tested)

- Graduated content never in viewport
- Input always after streaming content
- Viewport lines bounded by terminal width
- Cursor position accurate at input end
- Overlay composited correctly

### State Machine (Mostly Untested)

- Popup: hidden → visible → selected → hidden
- Input mode: Normal ↔ Command ↔ Shell
- Ctrl+C double-press quit (300ms window)
- Streaming cancels on Esc/Ctrl+C
- ChatMode cycles: Normal → Plan → Auto → Normal
- Autocomplete triggers (@, [[, /, :)

### Tool Ordering (Tested)

Located in `tool_ordering_tests.rs`:

- Tools maintain relative position during graduation
- Tool calls between text blocks stay in order
- Subagent events preserve ordering
- No tool duplication after graduation

---

## Extractable Pure Functions

### A. Autocomplete Trigger Detection

**Current location:** `chat_app.rs:812-828` (90+ lines)

**Problem:** Buried in InkChatApp, requires full app to test

**Proposed extraction:**

```rust
// src/tui/oil/autocomplete.rs
pub struct Trigger {
    pub kind: AutocompleteKind,
    pub pos: usize,
    pub filter: String,
}

/// Pure function: detect autocomplete trigger from cursor position
pub fn detect_trigger(content: &str, cursor: usize) -> Option<Trigger> {
    // Extract from InkChatApp::detect_trigger
}

/// Parse command args for autocomplete context
pub fn parse_command_context(line_before_cursor: &str) -> CommandContext {
    // Command name, arg index, current filter
}
```

**Tests needed:**
- Cursor at trigger boundaries
- Multiple triggers (`@file [[note]]`)
- Malformed triggers (`[[no closing`)
- Whitespace handling

### B. Tool Result Summarization

**Current location:** `message_list.rs:484-566` (80+ lines)

**Problem:** Business logic in render function

**Proposed extraction:**

```rust
// src/tui/oil/tool_summary.rs
pub struct ToolSummary {
    pub short: Option<String>,  // "3 files", "applied"
    pub collapsed: bool,
    pub show_expanded: bool,
}

/// Pure: determine how to display tool result
pub fn summarize_tool(name: &str, result: &str) -> ToolSummary {
    match name {
        "read_file" | "mcp_read" => summarize_read(result),
        "glob" | "mcp_glob" => count_newline_items(result),
        "grep" | "mcp_grep" => count_grep_matches(result),
        _ => ToolSummary::default(),
    }
}
```

**Tests needed:**
- File counts for glob
- Match counts for grep
- Line counts for read
- Empty/malformed results

### C. Command Parsing

**Current location:** `chat_app.rs:1107-1181` (150+ lines)

**Problem:** Parsing mixed with state mutations

**Proposed extraction:**

```rust
// src/tui/oil/command_parser.rs
pub enum ReplCommand {
    Quit,
    Help,
    Palette,
    Clear,
    Set(SetCommand),
    Model { name: Option<String> },
    Export { path: String },
}

/// Pure: parse REPL command string
pub fn parse_repl_command(input: &str) -> Result<ReplCommand, ParseError> {
    // Extract parsing, no state mutations
}
```

**Tests needed:**
- All command variations (`:quit`, `:q`)
- Commands with args (`:model llama3`)
- Error cases (unknown commands)
- Case sensitivity

### D. Popup State Machine

**Current location:** `chat_app.rs:980-1036`

**Problem:** Navigation intertwined with InkChatApp

**Proposed extraction:**

```rust
// src/tui/oil/popup_state.rs
pub struct PopupState {
    pub visible: bool,
    pub kind: AutocompleteKind,
    pub selected: usize,
    pub filter: String,
    pub items: Vec<PopupItem>,
}

impl PopupState {
    /// Pure: handle key, return action
    pub fn handle_key(&self, key: KeyCode) -> PopupAction {
        // Navigation without mutations
    }
    
    /// Pure: filter items
    pub fn visible_items(&self) -> &[PopupItem] { ... }
}

pub enum PopupAction {
    Noop,
    Close,
    Select { label: String },
    UpdateFilter { new_filter: String },
}
```

**Tests needed:**
- Navigation bounds (Up/Down)
- Enter with empty items
- Esc always closes
- Filter narrows visible items
- Selection wraps at boundaries

### E. Shell Modal Scrolling

**Current location:** `chat_app.rs:220-309`

**Problem:** Scrolling logic mixed with modal state

**Proposed extraction:**

```rust
// src/tui/oil/shell_modal.rs
impl ShellModal {
    /// Pure: calculate visible lines
    pub fn visible_lines(&self, max_lines: usize) -> &[String] {
        let total = self.output_lines.len();
        if total <= max_lines {
            return &self.output_lines;
        }
        let start = self.scroll_offset.min(total.saturating_sub(max_lines));
        &self.output_lines[start..start + max_lines]
    }
    
    /// Pure: scroll with bounds
    pub fn scroll(&self, direction: ScrollDirection, lines: usize) -> usize {
        // New offset, bounds-checked
    }
}
```

**Tests needed:**
- Offset never negative
- Offset never exceeds total - max
- User scrolled flag

---

## Proposed Test Fixtures

### State Machine Fixture

```rust
// src/tui/oil/testing/fixtures/state_fixtures.rs
pub struct ChatStateFixture {
    app: InkChatApp,
    runtime: TestRuntime,
}

impl ChatStateFixture {
    /// Pure: get trigger without side effects
    pub fn detect_trigger(&self) -> Option<Trigger> {
        autocomplete::detect_trigger(
            self.app.input.content(),
            self.app.input.cursor()
        )
    }
    
    /// Apply key sequence
    pub fn apply_keys(&mut self, keys: &[KeyCode]) -> Result<()> {
        for key in keys {
            self.app.handle_key(key);
        }
        Ok(())
    }
    
    pub fn assert_state(&self, expected: StateAssertion) { ... }
}

pub enum StateAssertion {
    PopupVisible(bool),
    PopupKind(AutocompleteKind),
    InputContains(&'static str),
    ChatMode(ChatMode),
    Streaming(bool),
}
```

### Streaming Fixture

```rust
// src/tui/oil/testing/fixtures/streaming_fixtures.rs
pub struct StreamingFixture {
    buffer: StreamingBuffer,
}

impl StreamingFixture {
    pub fn append_and_check<F>(&mut self, delta: &str, check: F)
    where F: Fn(&Self) {
        self.buffer.append(delta);
        check(self);
    }
    
    pub fn assert_graduated_blocks(&self, expected: &[&str]) { ... }
    pub fn assert_in_progress(&self, expected: &str) { ... }
    pub fn assert_no_duplication(&self) { ... }
}
```

### Property Test Strategies

```rust
// src/tui/oil/testing/fixtures/property_fixtures.rs
pub mod properties {
    use proptest::prelude::*;
    
    pub fn arb_chat_sequence() -> impl Strategy<Value = Vec<ChatAppMsg>> {
        prop::collection::vec(
            prop_oneof![
                Just(ChatAppMsg::UserMessage("Test".into())),
                Just(ChatAppMsg::TextDelta("content".into())),
                Just(ChatAppMsg::StreamComplete),
            ],
            1..20
        )
    }
    
    pub fn arb_terminal_size() -> impl Strategy<Value = (u16, u16)> {
        (80u16..200, 24u16..100)
    }
}
```

### Test Helpers

```rust
// src/tui/oil/testing/fixtures/helpers.rs

/// Assert content appears exactly once across stdout+viewport
pub fn assert_content_once(app: &InkChatApp, runtime: &mut TestRuntime, content: &str) {
    let tree = view_with_default_ctx(app);
    runtime.render(&tree);
    
    let combined = format!("{}{}", runtime.stdout_content(), runtime.viewport_content());
    let count = combined.matches(content).count();
    assert_eq!(count, 1, "Content '{}' appeared {} times", content, count);
}

/// Assert content graduated to stdout
pub fn assert_graduated(app: &InkChatApp, runtime: &mut TestRuntime, content: &str) {
    let tree = view_with_default_ctx(app);
    runtime.render(&tree);
    
    assert!(runtime.stdout_content().contains(content));
    assert!(!runtime.viewport_content().contains(content));
}

/// Verify viewport lines within width bounds
pub fn assert_lines_bounded(content: &str, width: usize, tolerance: usize) {
    for (i, line) in content.lines().enumerate() {
        let visible_width = ansi::visible_width(line);
        assert!(visible_width <= width + tolerance,
            "Line {} exceeds width: {} > {}", i, visible_width, width + tolerance);
    }
}
```

---

## Implementation Priority

### High Priority (Immediate Impact)

1. **Extract `detect_trigger()`** to `autocomplete.rs`
   - Enables popup state testing
   - Currently 90+ lines buried in InkChatApp

2. **Extract `summarize_tool_result()`** to `tool_summary.rs`
   - Pure business logic, easily testable
   - Currently in render function

3. **Create `ChatStateFixture`**
   - Enables state machine testing
   - Without full app setup

### Medium Priority (Better Structure)

4. **Extract `parse_repl_command()`** to `command_parser.rs`
5. **Extract ShellModal scrolling** to pure methods
6. **Create `StreamingFixture`** for graduation invariants

### Low Priority (Nice to Have)

7. **Refactor `handle_key()`** into smaller functions
8. **Extract rendering helpers** to utility module
9. **Add property tests** for extracted functions

---

## Summary Table

| Location | Function | Complexity | Current Testability |
|----------|----------|------------|---------------------|
| `chat_app.rs:838` | `detect_trigger()` | High | Very low |
| `chat_app.rs:155` | `display_tool_name()` | Low | Medium |
| `message_list.rs:158` | `format_elapsed()` | Low | High (pure) |
| `message_list.rs:320` | `collapse_result()` | Medium | Medium |
| `message_list.rs:484` | `summarize_tool_result()` | High | Medium |
| `message_list.rs:545` | `count_newline_items()` | Low | High (pure) |
| `viewport_cache.rs:867` | `find_graduation_point()` | Medium | Low |
| `runtime.rs:63` | `format_stdout_delta()` | Medium | Medium |
| `chat_app.rs:251` | `visible_lines()` | Low | Medium |
