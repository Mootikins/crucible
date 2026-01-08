# TUI Architecture Improvement Plan

> Goal: Improve composability, testability, and LLM iteration speed without migrating frameworks

## Current State Analysis

### Strengths
- Clean dialog pattern (state/widget separation in `dialog.rs`)
- Generic popup with trait-based items (`Popup<T>` with `PopupItem` trait)
- Good test harness with snapshot support via insta
- Action dispatch layer enables scripting hooks

### Friction Points

| Issue | Location | Impact |
|-------|----------|--------|
| State duplication | `TuiState` vs `ViewState` | Manual sync required, bugs from drift |
| Two event systems | `WidgetEventResult` vs `EventResult` | Confusion about which to use |
| Monolithic runner | `RatatuiRunner` (50+ fields) | Hard to understand, test, modify |
| Legacy wrapper | `LegacyPopupItem` | Incomplete migration, extra indirection |

### Why This Hurts LLM Iteration
- Inconsistent patterns = LLMs generate wrong code
- Hidden state sync = subtle bugs
- Too many files to understand = context overload
- No clear "shape" to the UI = structure isn't scannable

---

## Framework Alternatives Evaluated

| Library | Composability | Testability | LLM Iteration | Verdict |
|---------|--------------|-------------|---------------|---------|
| **iocraft** | Excellent | Good | Excellent | Missing critical features (styled text, virtual scroll, selection) |
| **tui-realm** | Good | Poor | Moderate | Adds abstraction without enough benefit |
| **Cursive** | Good | Excellent | Good | Different paradigm, high migration cost |
| **Dioxus TUI** | Excellent | Good | Excellent | Dead - official renderer deprecated |

**Decision: Don't migrate.** Fix architecture instead.

---

## Execution Phases

### Phase 1: Unify State (PARALLEL)

**Goal:** Single source of truth for all TUI state

#### Current State
```rust
// Two overlapping states requiring manual sync
struct TuiState { input_buffer, cursor_position, has_popup, ... }
struct ViewState { input_buffer, cursor_position, popup: Option<PopupState>, ... }

// Harness must call this after every operation
fn sync_input_to_view(&mut self) {
    self.view.state_mut().input_buffer = self.state.input_buffer.clone();
    self.view.state_mut().cursor_position = self.state.cursor_position;
}
```

#### Target State
```rust
/// Unified TUI state - ViewState is canonical owner of all UI state
pub struct TuiState {
    /// ViewState owns ALL view-related fields
    pub view: ViewState,

    /// TuiState owns non-view concerns only
    pub streaming: StreamingState,
    pub history: CommandHistory,
    pub mode_id: String,
}

/// ViewState is the SINGLE owner of:
pub struct ViewState {
    // Input state (MOVED from TuiState)
    pub input_buffer: String,
    pub cursor_position: usize,

    // Popup state (already here)
    pub popup: Option<PopupState>,

    // Dialog state (already here)
    pub dialog_stack: DialogStack,

    // Conversation state (already here)
    pub conversation: ConversationState,

    // Scroll/viewport (already here)
    pub scroll_offset: usize,
    pub width: u16,
    pub height: u16,
}
```

#### Files to Modify

| File | Changes | Keep/Delete |
|------|---------|-------------|
| `state.rs` | Remove `input_buffer`, `cursor_position`, `has_popup` from `TuiState` | MODIFY |
| `conversation_view.rs` | Already owns `input_buffer` - no change | KEEP AS-IS |
| `testing/harness.rs` | Remove `sync_input_to_view()`, access via `self.state.view.*` | MODIFY |
| `runner.rs` | Access input via `self.view.state().input_buffer` | MODIFY |

#### Skeleton: Updated TuiState

```rust
// crates/crucible-cli/src/tui/state.rs

/// Unified TUI state - orchestrates view + non-view concerns
pub struct TuiState {
    /// View state (owns input, popup, dialog, conversation)
    view: ViewState,

    /// Streaming state (separate concern)
    streaming: StreamingState,

    /// Command history (separate concern)
    history: CommandHistory,

    /// Current mode
    mode_id: String,

    /// Whether we're waiting for agent response
    waiting_for_response: bool,
}

impl TuiState {
    pub fn new(mode_id: &str) -> Self {
        Self {
            view: ViewState::new(mode_id, 80, 24), // Default dims
            streaming: StreamingState::default(),
            history: CommandHistory::new(100),
            mode_id: mode_id.to_string(),
            waiting_for_response: false,
        }
    }

    // Accessors delegate to view
    pub fn input(&self) -> &str { &self.view.input_buffer }
    pub fn input_mut(&mut self) -> &mut String { &mut self.view.input_buffer }
    pub fn cursor(&self) -> usize { self.view.cursor_position }
    pub fn set_cursor(&mut self, pos: usize) { self.view.cursor_position = pos; }

    pub fn has_popup(&self) -> bool { self.view.popup.is_some() }
    pub fn popup(&self) -> Option<&PopupState> { self.view.popup.as_ref() }
    pub fn popup_mut(&mut self) -> Option<&mut PopupState> { self.view.popup.as_mut() }
}
```

#### Skeleton: Updated Harness

```rust
// crates/crucible-cli/src/tui/testing/harness.rs

pub struct Harness {
    /// Unified state (no more separate TuiState + ConversationState)
    pub state: TuiState,

    /// Viewport dimensions
    pub width: u16,
    pub height: u16,
}

impl Harness {
    pub fn new(width: u16, height: u16) -> Self {
        let mut state = TuiState::new("plan");
        state.view.set_dimensions(width, height);

        Self { state, width, height }
    }

    // NO MORE sync_input_to_view() - state is unified

    pub fn input_text(&self) -> &str {
        self.state.input()
    }

    pub fn key(&mut self, code: KeyCode) {
        // Input goes directly to unified state
        if let KeyCode::Char(c) = code {
            self.state.input_mut().push(c);
            self.state.set_cursor(self.state.cursor() + 1);
        }
        // ... rest of key handling
    }
}
```

#### Verification Criteria
- [ ] `sync_input_to_view()` deleted from harness.rs
- [ ] All tests pass without manual sync
- [ ] `TuiState.input_buffer` field removed
- [ ] `TuiState.cursor_position` field removed
- [ ] `TuiState.has_popup` field removed (derive from `view.popup.is_some()`)

---

### Phase 2: Unify Event Types (PARALLEL)

**Goal:** One event result type, one action type

#### Current State
```rust
// components/mod.rs - widget-level events
pub enum WidgetEventResult { Consumed, Ignored, Action(WidgetAction) }
pub enum WidgetAction { Scroll(isize), ConfirmPopup(usize), DismissPopup, ... }

// event_result.rs - runner-level events
pub enum EventResult { Ignored, Handled, NeedsRender, Action(TuiAction) }
pub enum TuiAction { SendMessage(String), ExecuteCommand(String), Scroll(ScrollAction), ... }
```

#### Target State
```rust
// event_result.rs - SINGLE event type
pub enum EventResult {
    Ignored,
    Consumed,
    NeedsRender,
    Action(TuiAction),
}

// TuiAction absorbs WidgetAction variants
pub enum TuiAction {
    // From WidgetAction
    Scroll(isize),
    ScrollTo(usize),
    ConfirmPopup(usize),
    DismissPopup,
    CycleMode,
    RequestFocus(FocusTarget),
    CloseDialog(DialogAction),

    // Original TuiAction
    SendMessage(String),
    ExecuteCommand(String),
    Cancel,
    Exit,
    PopupConfirm(PopupItem),
    PopupClose,
    DialogConfirm,
    DialogCancel,
    DialogSelect(usize),
    DialogDismiss,
}
```

#### Files to Modify

| File | Changes | Keep/Delete |
|------|---------|-------------|
| `components/mod.rs` | DELETE `WidgetEventResult`, `WidgetAction`, update `InteractiveWidget` trait | MODIFY |
| `event_result.rs` | Add `Consumed` variant, merge `WidgetAction` into `TuiAction` | MODIFY |
| `components/session_history.rs` | Return `EventResult` instead of `WidgetEventResult` | MODIFY |
| `components/input_box.rs` | Return `EventResult` instead of `WidgetEventResult` | MODIFY |
| `components/dialog.rs` | Return `EventResult` instead of `WidgetEventResult` | MODIFY |
| `components/generic_popup.rs` | Return `EventResult` instead of `WidgetEventResult` | MODIFY |
| `components/layer_stack.rs` | Return `EventResult` instead of `WidgetEventResult` | MODIFY |

#### Skeleton: Unified InteractiveWidget

```rust
// crates/crucible-cli/src/tui/components/mod.rs

use crate::tui::event_result::EventResult;

/// Extension trait for widgets that handle input events
pub trait InteractiveWidget: Widget {
    /// Handle an input event, returning unified EventResult
    fn handle_event(&mut self, event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn focusable(&self) -> bool {
        false
    }
}

// DELETE WidgetEventResult
// DELETE WidgetAction
// DELETE FocusTarget (move to event_result.rs)
// DELETE DialogAction (move to event_result.rs)
```

#### Skeleton: Unified EventResult

```rust
// crates/crucible-cli/src/tui/event_result.rs

/// Result of handling an event
#[derive(Debug, Clone, PartialEq)]
pub enum EventResult {
    /// Event was not handled
    Ignored,
    /// Event was handled (absorbed), stop propagation
    Consumed,
    /// Event was handled, UI needs repaint
    NeedsRender,
    /// Event produced an action for runner
    Action(TuiAction),
}

/// Unified actions from all components
#[derive(Debug, Clone, PartialEq)]
pub enum TuiAction {
    // === Scroll actions ===
    ScrollLines(isize),           // Was WidgetAction::Scroll
    ScrollTo(usize),              // Was WidgetAction::ScrollTo
    ScrollPage(ScrollDirection),  // Combines PageUp/PageDown

    // === Input actions ===
    SendMessage(String),
    ExecuteCommand(String),

    // === Popup actions ===
    ConfirmPopup(usize),          // Was WidgetAction::ConfirmPopup
    DismissPopup,                 // Was WidgetAction::DismissPopup
    PopupConfirm(PopupItem),      // Popup with resolved item
    PopupClose,

    // === Dialog actions ===
    CloseDialog(DialogResult),    // Was WidgetAction::CloseDialog
    DialogConfirm,
    DialogCancel,
    DialogSelect(usize),
    DialogDismiss,

    // === Mode/focus actions ===
    CycleMode,
    RequestFocus(FocusTarget),

    // === Control actions ===
    Cancel,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection { Up, Down }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget { Input, History, Popup, Dialog }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult { Confirm, Cancel, Select(usize) }
```

#### Verification Criteria
- [ ] `WidgetEventResult` deleted from components/mod.rs
- [ ] `WidgetAction` deleted from components/mod.rs
- [ ] All component files updated to use `EventResult`
- [ ] `InteractiveWidget::handle_event` returns `EventResult`
- [ ] All tests pass

---

### Phase 3: Extract Runner Subsystems (DEPENDS ON 1, 2)

**Goal:** Runner becomes a coordinator, not an owner of everything

#### Current State
```rust
// runner.rs - 139KB, 50+ fields
struct RatatuiRunner {
    streaming_rx, streaming_parser, pending_chunks,
    history, history_index, history_saved_input,
    selection_start, selection_end, clipboard, mouse_mode,
    rapid_input_buffer, in_rapid_input,
    // ... many more
}
```

#### Target State
```rust
// runner.rs - ~500 lines, coordination only
struct RatatuiRunner {
    /// Subsystems own their state
    streaming: StreamingManager,
    history: HistoryManager,
    selection: SelectionManager,
    input_mode: InputModeManager,

    /// View owns UI state
    view: RatatuiView,

    /// Event coordination
    event_loop: EventLoop,
    hooks: PopupHooks,
}
```

#### New Files to Create

| File | Purpose | Approximate Size |
|------|---------|------------------|
| `streaming_manager.rs` | Streaming channel, parser, buffer | ~300 lines |
| `history_manager.rs` | Command history, navigation | ~150 lines |
| `selection_manager.rs` | Text selection, clipboard | ~200 lines |
| `input_mode_manager.rs` | Rapid input, paste detection | ~150 lines |

#### Skeleton: StreamingManager

```rust
// crates/crucible-cli/src/tui/streaming_manager.rs

use crate::tui::streaming_channel::StreamingEvent;
use crate::tui::streaming_parser::StreamingParser;
use tokio::sync::mpsc::Receiver;

/// Manages streaming content from LLM
pub struct StreamingManager {
    /// Receives streaming events from agent
    rx: Option<Receiver<StreamingEvent>>,
    /// Parses markdown chunks into structured blocks
    parser: StreamingParser,
    /// Buffered chunks awaiting processing
    pending_chunks: Vec<String>,
    /// Whether we're currently streaming
    is_streaming: bool,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            rx: None,
            parser: StreamingParser::new(),
            pending_chunks: Vec::new(),
            is_streaming: false,
        }
    }

    /// Start receiving from a new streaming channel
    pub fn connect(&mut self, rx: Receiver<StreamingEvent>) {
        self.rx = Some(rx);
        self.is_streaming = true;
        self.parser.reset();
        self.pending_chunks.clear();
    }

    /// Poll for pending events (non-blocking)
    pub fn poll(&mut self) -> Vec<StreamingEvent> {
        let mut events = Vec::new();
        if let Some(rx) = &mut self.rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        events
    }

    /// Process a chunk through the parser
    pub fn process_chunk(&mut self, chunk: &str) -> Vec<StreamBlock> {
        self.parser.push(chunk)
    }

    /// Complete streaming, flush parser
    pub fn complete(&mut self) -> Vec<StreamBlock> {
        self.is_streaming = false;
        self.parser.flush()
    }

    pub fn is_streaming(&self) -> bool { self.is_streaming }
}
```

#### Skeleton: HistoryManager

```rust
// crates/crucible-cli/src/tui/history_manager.rs

/// Manages command history navigation
pub struct HistoryManager {
    /// Past commands
    entries: Vec<String>,
    /// Current position (None = not browsing)
    index: Option<usize>,
    /// Saved input when starting to browse
    saved_input: String,
    /// Max entries to keep
    capacity: usize,
}

impl HistoryManager {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            index: None,
            saved_input: String::new(),
            capacity,
        }
    }

    /// Add a command to history
    pub fn push(&mut self, command: String) {
        if !command.trim().is_empty() {
            // Dedupe consecutive duplicates
            if self.entries.last() != Some(&command) {
                self.entries.push(command);
                if self.entries.len() > self.capacity {
                    self.entries.remove(0);
                }
            }
        }
        self.index = None;
    }

    /// Start browsing history, saving current input
    pub fn start_browse(&mut self, current_input: &str) {
        if self.index.is_none() {
            self.saved_input = current_input.to_string();
            self.index = Some(self.entries.len());
        }
    }

    /// Navigate up in history
    pub fn up(&mut self) -> Option<&str> {
        let idx = self.index?;
        if idx > 0 {
            self.index = Some(idx - 1);
            Some(&self.entries[idx - 1])
        } else {
            None
        }
    }

    /// Navigate down in history
    pub fn down(&mut self) -> Option<&str> {
        let idx = self.index?;
        if idx < self.entries.len() {
            self.index = Some(idx + 1);
            if idx + 1 == self.entries.len() {
                Some(&self.saved_input)
            } else {
                Some(&self.entries[idx + 1])
            }
        } else {
            None
        }
    }

    /// Exit history browsing
    pub fn stop_browse(&mut self) {
        self.index = None;
    }
}
```

#### Skeleton: SelectionManager

```rust
// crates/crucible-cli/src/tui/selection_manager.rs

use crate::tui::selection::SelectionPoint;

/// Manages text selection and clipboard
pub struct SelectionManager {
    /// Selection start point
    start: Option<SelectionPoint>,
    /// Selection end point
    end: Option<SelectionPoint>,
    /// Copied text
    clipboard: Option<String>,
    /// Whether mouse drag is active
    dragging: bool,
}

impl SelectionManager {
    pub fn new() -> Self {
        Self {
            start: None,
            end: None,
            clipboard: None,
            dragging: false,
        }
    }

    /// Start a selection at a point
    pub fn start(&mut self, point: SelectionPoint) {
        self.start = Some(point);
        self.end = None;
        self.dragging = true;
    }

    /// Update selection endpoint
    pub fn update(&mut self, point: SelectionPoint) {
        if self.dragging {
            self.end = Some(point);
        }
    }

    /// Complete selection
    pub fn complete(&mut self) {
        self.dragging = false;
    }

    /// Copy selected text to clipboard
    pub fn copy(&mut self, text: String) {
        self.clipboard = Some(text);
        // Also copy to system clipboard
        if let Err(e) = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&text)) {
            tracing::debug!("Failed to copy to system clipboard: {}", e);
        }
    }

    /// Clear selection
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.dragging = false;
    }

    pub fn has_selection(&self) -> bool {
        self.start.is_some() && self.end.is_some()
    }

    pub fn bounds(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        Some((self.start?, self.end?))
    }
}
```

#### Skeleton: InputModeManager

```rust
// crates/crucible-cli/src/tui/input_mode_manager.rs

use once_cell::sync::Lazy;
use regex::Regex;
use std::time::{Duration, Instant};

/// Manages input modes (normal, rapid/paste)
pub struct InputModeManager {
    /// Buffer for rapid input (paste detection)
    rapid_buffer: String,
    /// Whether in rapid input mode
    in_rapid_mode: bool,
    /// Last character timestamp for paste detection
    last_char_time: Option<Instant>,
    /// Threshold for rapid input detection
    rapid_threshold: Duration,
}

static PASTE_INDICATOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\x00-\x1F]").unwrap()
});

impl InputModeManager {
    pub fn new() -> Self {
        Self {
            rapid_buffer: String::new(),
            in_rapid_mode: false,
            last_char_time: None,
            rapid_threshold: Duration::from_millis(5),
        }
    }

    /// Check if character should trigger rapid mode
    pub fn check_rapid_mode(&mut self, c: char) -> bool {
        let now = Instant::now();

        if let Some(last) = self.last_char_time {
            if now.duration_since(last) < self.rapid_threshold {
                self.in_rapid_mode = true;
            }
        }

        self.last_char_time = Some(now);
        self.in_rapid_mode
    }

    /// Buffer a character in rapid mode
    pub fn buffer_char(&mut self, c: char) {
        if self.in_rapid_mode {
            self.rapid_buffer.push(c);
        }
    }

    /// Flush rapid buffer and return contents
    pub fn flush(&mut self) -> String {
        self.in_rapid_mode = false;
        std::mem::take(&mut self.rapid_buffer)
    }

    /// Check if text looks like a paste (contains control chars)
    pub fn is_paste_indicator(text: &str) -> bool {
        PASTE_INDICATOR_RE.is_match(text)
    }
}
```

#### Verification Criteria
- [ ] `runner.rs` under 500 lines
- [ ] All streaming logic in `streaming_manager.rs`
- [ ] All history logic in `history_manager.rs`
- [ ] All selection logic in `selection_manager.rs`
- [ ] All rapid input logic in `input_mode_manager.rs`
- [ ] Runner only coordinates between subsystems
- [ ] All tests pass

---

### Phase 4: Complete Popup Migration (PARALLEL)

**Goal:** Remove `LegacyPopupItem` wrapper, use `Popup<T>` directly

#### Current State
```rust
// Legacy enum still exists
pub enum PopupItem { Command {...}, Agent {...}, File {...}, ... }

// Wrapper adapts old to new
struct LegacyPopupItem(PopupItem);
impl PopupItem for LegacyPopupItem { ... }
```

#### Target State
```rust
// Type-specific items implement trait directly
pub struct CommandItem { name: String, description: String, ... }
impl PopupItemTrait for CommandItem { ... }

pub struct AgentItem { id: String, name: String, ... }
impl PopupItemTrait for AgentItem { ... }

// PopupState uses trait object
pub struct PopupState {
    items: Vec<Box<dyn PopupItemTrait>>,
    // ...
}
```

#### Files to Modify

| File | Changes | Keep/Delete |
|------|---------|-------------|
| `state.rs` | Keep `PopupItem` enum as domain type | KEEP |
| `components/generic_popup.rs` | DELETE `LegacyPopupItem`, use `PopupItem` directly | MODIFY |
| `widgets/popup.rs` | Update to work with `PopupItem` directly | MODIFY |

#### Why Keep PopupItem Enum

The `PopupItem` enum is a **domain type** representing the different things that can appear in popups. It's used in:
- `action_dispatch.rs` - scripting hooks
- `event_result.rs` - actions
- `registries/` - item providers

Converting to separate structs adds complexity without benefit. Instead, implement `PopupItemTrait` directly on the enum:

```rust
// crates/crucible-cli/src/tui/state.rs

/// Domain type for popup items - KEEP AS-IS
#[derive(Debug, Clone, PartialEq)]
pub enum PopupItem {
    Command { name: String, description: String, shortcut: Option<String> },
    Agent { id: String, name: String, provider: String },
    File { path: String, name: String },
    Note { path: String, title: String },
    Skill { name: String, description: String },
    ReplCommand { name: String, description: String },
    Session { id: String, title: String, timestamp: String },
}

// Implement trait directly on enum
impl crate::tui::widgets::popup::PopupItemTrait for PopupItem {
    fn label(&self) -> &str {
        match self {
            PopupItem::Command { name, .. } => name,
            PopupItem::Agent { name, .. } => name,
            PopupItem::File { name, .. } => name,
            PopupItem::Note { title, .. } => title,
            PopupItem::Skill { name, .. } => name,
            PopupItem::ReplCommand { name, .. } => name,
            PopupItem::Session { title, .. } => title,
        }
    }

    fn description(&self) -> Option<&str> {
        match self {
            PopupItem::Command { description, .. } => Some(description),
            PopupItem::Agent { provider, .. } => Some(provider),
            PopupItem::Skill { description, .. } => Some(description),
            PopupItem::ReplCommand { description, .. } => Some(description),
            _ => None,
        }
    }

    fn matches(&self, query: &str) -> bool {
        let label = self.label().to_lowercase();
        let query = query.to_lowercase();
        label.contains(&query)
    }

    fn enabled(&self) -> bool { true }
    fn shortcut(&self) -> Option<&str> {
        match self {
            PopupItem::Command { shortcut, .. } => shortcut.as_deref(),
            _ => None,
        }
    }
}
```

#### Skeleton: Updated PopupState

```rust
// crates/crucible-cli/src/tui/components/generic_popup.rs

use crate::tui::state::PopupItem;
use crate::tui::widgets::popup::PopupItemTrait;

// DELETE LegacyPopupItem wrapper entirely

/// Popup state using PopupItem directly
pub struct PopupState {
    kind: PopupKind,
    provider: Arc<dyn PopupProvider>,
    items: Vec<PopupItem>,          // Direct, not wrapped
    filtered: Vec<usize>,
    selected: usize,
    query: String,
}

impl PopupState {
    pub fn items(&self) -> impl Iterator<Item = &PopupItem> {
        self.filtered.iter().map(|&i| &self.items[i])
    }

    pub fn selected_item(&self) -> Option<&PopupItem> {
        self.filtered.get(self.selected).map(|&i| &self.items[i])
    }
}
```

#### Verification Criteria
- [ ] `LegacyPopupItem` struct deleted
- [ ] `PopupItem` implements `PopupItemTrait` directly
- [ ] All popup tests pass
- [ ] Scripting hooks still work (`action_dispatch.rs`)

---

## Scripting Backend Integration

### Current Hook Points

```rust
// action_dispatch.rs
pub trait PopupHook: Send + Sync {
    fn on_popup_select(&self, item: &PopupItem) -> Option<PopupEffect>;
}
```

### Required Updates for ALL Backends

The `TuiAction` and `EventResult` changes require updates to scripting bindings:

#### Rune Integration

```rust
// crates/crucible-rune/src/tui_module.rs (NEW FILE)

use rune::{Any, Module};
use crucible_cli::tui::event_result::{TuiAction, EventResult, FocusTarget, DialogResult};
use crucible_cli::tui::state::PopupItem;

/// Install TUI types into Rune module
pub fn module() -> Result<Module, rune::ContextError> {
    let mut module = Module::with_crate("crucible", ["tui"])?;

    // Register event types
    module.ty::<TuiAction>()?;
    module.ty::<EventResult>()?;
    module.ty::<FocusTarget>()?;
    module.ty::<DialogResult>()?;
    module.ty::<PopupItem>()?;

    // Register action constructors
    module.function_meta(TuiAction::send_message)?;
    module.function_meta(TuiAction::scroll_lines)?;
    module.function_meta(TuiAction::dismiss_popup)?;

    Ok(module)
}

// Action constructors for Rune scripts
impl TuiAction {
    #[rune::function(path = Self::send_message)]
    fn send_message(msg: String) -> Self {
        TuiAction::SendMessage(msg)
    }

    #[rune::function(path = Self::scroll_lines)]
    fn scroll_lines(lines: i64) -> Self {
        TuiAction::ScrollLines(lines as isize)
    }

    #[rune::function(path = Self::dismiss_popup)]
    fn dismiss_popup() -> Self {
        TuiAction::DismissPopup
    }
}
```

#### Steel Integration

```rust
// crates/crucible-steel/src/tui_module.rs (NEW FILE)

use steel::steel_vm::engine::Engine;
use crucible_cli::tui::event_result::{TuiAction, EventResult};
use crucible_cli::tui::state::PopupItem;

/// Register TUI types with Steel VM
pub fn register_tui_types(engine: &mut Engine) {
    // Register struct types
    engine.register_type::<TuiAction>("TuiAction");
    engine.register_type::<EventResult>("EventResult");
    engine.register_type::<PopupItem>("PopupItem");

    // Register constructors
    engine.register_fn("tui-action-send-message", |msg: String| {
        TuiAction::SendMessage(msg)
    });

    engine.register_fn("tui-action-scroll", |lines: i64| {
        TuiAction::ScrollLines(lines as isize)
    });

    engine.register_fn("tui-action-dismiss-popup", || {
        TuiAction::DismissPopup
    });

    // Register predicates
    engine.register_fn("popup-item-command?", |item: &PopupItem| {
        matches!(item, PopupItem::Command { .. })
    });

    engine.register_fn("popup-item-name", |item: &PopupItem| {
        item.label().to_string()
    });
}
```

#### Lua/Fennel Integration

```rust
// crates/crucible-lua/src/tui_module.rs (NEW FILE)

use mlua::{Lua, Result, UserData, UserDataMethods};
use crucible_cli::tui::event_result::{TuiAction, EventResult};
use crucible_cli::tui::state::PopupItem;

impl UserData for TuiAction {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        // Static constructors
        methods.add_function("send_message", |_, msg: String| {
            Ok(TuiAction::SendMessage(msg))
        });

        methods.add_function("scroll", |_, lines: i64| {
            Ok(TuiAction::ScrollLines(lines as isize))
        });

        methods.add_function("dismiss_popup", |_, ()| {
            Ok(TuiAction::DismissPopup)
        });
    }
}

impl UserData for PopupItem {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("label", |_, this, ()| {
            Ok(this.label().to_string())
        });

        methods.add_method("description", |_, this, ()| {
            Ok(this.description().map(|s| s.to_string()))
        });

        methods.add_method("is_command", |_, this, ()| {
            Ok(matches!(this, PopupItem::Command { .. }))
        });
    }
}

/// Register TUI module with Lua
pub fn register_tui_module(lua: &Lua) -> Result<()> {
    let tui = lua.create_table()?;

    tui.set("Action", lua.create_table()?)?;
    // ... populate action constructors

    lua.globals().set("tui", tui)?;
    Ok(())
}
```

### Scripting API Surface

All backends expose the same API:

| Function | Purpose | Example |
|----------|---------|---------|
| `tui.action.send_message(msg)` | Create send action | `tui.action.send_message("hello")` |
| `tui.action.scroll(lines)` | Create scroll action | `tui.action.scroll(-5)` |
| `tui.action.dismiss_popup()` | Create dismiss action | `tui.action.dismiss_popup()` |
| `popup_item.label` | Get item label | `item.label` |
| `popup_item.description` | Get item description | `item.description` |
| `popup_item:is_command()` | Check if command | `item:is_command()` |

---

## Testing Strategy

### Multi-Snapshot Test Pattern

For stateful UI interactions, use sequential snapshot testing:

```rust
// crates/crucible-cli/src/tui/testing/interaction_tests.rs

use super::harness::Harness;
use insta::assert_snapshot;

/// Test a complete user flow with multiple snapshots
#[test]
fn test_popup_workflow_multi_snapshot() {
    let mut h = Harness::new(80, 24);

    // Step 1: Initial state
    assert_snapshot!("popup_workflow_01_initial", h.render());

    // Step 2: Type trigger character
    h.key(KeyCode::Char('/'));
    assert_snapshot!("popup_workflow_02_trigger", h.render());
    assert!(h.has_popup(), "Popup should open on /");

    // Step 3: Type query
    h.keys("help");
    assert_snapshot!("popup_workflow_03_filter", h.render());

    // Step 4: Navigate selection
    h.key(KeyCode::Down);
    assert_snapshot!("popup_workflow_04_navigate", h.render());

    // Step 5: Confirm selection
    h.key(KeyCode::Enter);
    assert_snapshot!("popup_workflow_05_confirm", h.render());
    assert!(!h.has_popup(), "Popup should close on confirm");
    assert!(h.input_text().contains("/help"), "Command inserted");
}

/// Test dialog interaction flow
#[test]
fn test_dialog_workflow_multi_snapshot() {
    let mut h = Harness::new(80, 24);
    h.push_confirmation_dialog("Delete file?", vec!["Yes", "No"]);

    // Step 1: Dialog shown
    assert_snapshot!("dialog_workflow_01_shown", h.render());

    // Step 2: Navigate options
    h.key(KeyCode::Down);
    assert_snapshot!("dialog_workflow_02_navigate", h.render());

    // Step 3: Escape cancels
    h.key(KeyCode::Esc);
    assert_snapshot!("dialog_workflow_03_cancelled", h.render());
    assert!(!h.has_dialog());
}

/// Test input wrapping behavior
#[test]
fn test_input_wrap_multi_snapshot() {
    let mut h = Harness::new(40, 10); // Narrow viewport

    // Step 1: Empty input
    assert_snapshot!("input_wrap_01_empty", h.render());

    // Step 2: Short text (no wrap)
    h.keys("Hello world");
    assert_snapshot!("input_wrap_02_short", h.render());

    // Step 3: Long text (triggers wrap)
    h.keys(" this is a much longer message that should wrap");
    assert_snapshot!("input_wrap_03_wrapped", h.render());

    // Step 4: Cursor at end
    assert_eq!(h.cursor_position(), h.input_text().len());

    // Step 5: Navigate back
    h.key(KeyCode::Home);
    assert_snapshot!("input_wrap_05_cursor_home", h.render());
}
```

### Required Test Coverage

| Feature | Test File | Required Tests |
|---------|-----------|----------------|
| State unification | `state_tests.rs` | No manual sync needed, accessor methods work |
| Event unification | `event_result_tests.rs` | All TuiAction variants, EventResult.or() priority |
| Streaming manager | `streaming_manager_tests.rs` | Connect, poll, process chunks, complete |
| History manager | `history_manager_tests.rs` | Push, navigate up/down, dedup |
| Selection manager | `selection_manager_tests.rs` | Start, update, complete, copy |
| Input mode manager | `input_mode_manager_tests.rs` | Rapid mode detection, buffering |
| Popup (no legacy) | `popup_tests.rs` | Filter, navigate, confirm without LegacyPopupItem |
| Multi-snapshot flows | `interaction_tests.rs` | Popup workflow, dialog workflow, input wrap |

### Test Commands

```bash
# Run all TUI tests
cargo test -p crucible-cli --lib tui::

# Run specific test module
cargo test -p crucible-cli --lib tui::testing::interaction_tests

# Update snapshots
cargo insta test -p crucible-cli --accept

# Run with verbose output
cargo test -p crucible-cli --lib tui:: -- --nocapture
```

---

## Ralph Loop Execution

### What is Ralph?

Ralph is a pattern for executing implementation phases using parallel subagents. Each phase gets a dedicated agent that:
1. Reads the phase specification
2. Implements the changes
3. Runs tests
4. Reports success/failure

### Ralph Prompt Template

```markdown
# Phase Implementation Task

You are implementing **Phase {N}: {Phase Name}** of the TUI Architecture Improvement plan.

## Your Objective

{Copy the relevant phase section from the plan}

## Files to Modify

{List files with specific changes}

## Verification Criteria

{Copy verification criteria}

## Rules

1. **Make minimal changes** - only what's specified
2. **Run tests frequently** - `cargo test -p crucible-cli --lib` after each file
3. **Preserve existing tests** - don't delete tests unless they test removed code
4. **Update snapshots** - if tests fail due to rendering changes, review and accept
5. **Report blockers** - if stuck after 3 attempts, report the specific issue

## Escalation Protocol

If you encounter an error:
1. Read the error message carefully
2. Attempt to fix it (up to 5 attempts)
3. If still failing, report:
   - The exact error message
   - What you've tried
   - Your hypothesis for the root cause

## Success Criteria

When done, verify:
- [ ] All specified changes made
- [ ] All tests pass: `cargo test -p crucible-cli --lib`
- [ ] No clippy warnings: `cargo clippy -p crucible-cli -- -D warnings`
- [ ] Formatted: `cargo fmt --check`

Report completion with:
- List of files changed
- Test results summary
- Any notes for subsequent phases
```

### Execution Order

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Phase 1: State  │     │ Phase 2: Events │     │ Phase 4: Popup  │
│   (PARALLEL)    │     │   (PARALLEL)    │     │   (PARALLEL)    │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                                 ▼
                  ┌──────────────────────────┐
                  │   Phase 3: Runner        │
                  │   (DEPENDS ON 1, 2)      │
                  └──────────────────────────┘
```

### Launching Ralph Agents

```rust
// Pseudocode for launching parallel agents

async fn execute_phases() {
    // Launch parallel phases
    let phase1 = spawn_agent("phase1_state", PHASE1_PROMPT);
    let phase2 = spawn_agent("phase2_events", PHASE2_PROMPT);
    let phase4 = spawn_agent("phase4_popup", PHASE4_PROMPT);

    // Wait for all parallel phases
    let results = join!(phase1, phase2, phase4);

    // Check for failures
    for (name, result) in results {
        if let Err(e) = result {
            println!("Phase {} failed: {}", name, e);
            // Escalate or retry
        }
    }

    // Launch dependent phase
    if all_succeeded(&results) {
        let phase3 = spawn_agent("phase3_runner", PHASE3_PROMPT);
        phase3.await?;
    }
}
```

---

## Deletions Summary

| Item | Location | Why Delete |
|------|----------|------------|
| `sync_input_to_view()` | `harness.rs` | State unification eliminates need |
| `WidgetEventResult` | `components/mod.rs` | Merged into `EventResult` |
| `WidgetAction` | `components/mod.rs` | Merged into `TuiAction` |
| `LegacyPopupItem` | `generic_popup.rs` | Direct trait impl on `PopupItem` |
| `TuiState.input_buffer` | `state.rs` | Moved to `ViewState` |
| `TuiState.cursor_position` | `state.rs` | Moved to `ViewState` |
| `TuiState.has_popup` | `state.rs` | Derived from `view.popup.is_some()` |
| 50+ fields from runner | `runner.rs` | Moved to manager structs |

**No deprecations** - all removals are immediate. Code that uses removed items will fail to compile, which is the desired behavior for catching all usages.

---

## Success Metrics

| Metric | Before | After | How to Measure |
|--------|--------|-------|----------------|
| State sync bugs | Possible | Impossible | `sync_input_to_view` deleted |
| Event type confusion | 2 types | 1 type | `WidgetEventResult` deleted |
| Runner complexity | 139KB | <20KB | `wc -c runner.rs` |
| Legacy wrappers | 1 | 0 | `LegacyPopupItem` deleted |
| Test coverage | ~80% | >90% | `cargo tarpaulin` |
| LLM iteration speed | Slow | Fast | Qualitative (fewer mistakes) |

---

## Escalation Protocol

### When to Escalate

1. **Compile errors after 5 fix attempts** - report the exact error
2. **Test failures that don't make sense** - report test name and failure
3. **Circular dependencies** - report the cycle
4. **Missing context** - report what information is needed

### Escalation Format

```markdown
## Escalation Report

**Phase:** {N}
**Attempts:** {count}/5
**Issue:** {brief description}

### Error Message
```
{exact error}
```

### What I Tried
1. {attempt 1}
2. {attempt 2}
...

### Hypothesis
{Your theory about the root cause}

### Requested Help
{What you need to proceed}
```

### Resolution Path

1. Human reviews escalation
2. Provides guidance or code fix
3. Agent retries with new information
4. If still failing, human takes over that specific change

---

## Appendix: File Index

### Files to CREATE

| File | Phase | Lines |
|------|-------|-------|
| `streaming_manager.rs` | 3 | ~300 |
| `history_manager.rs` | 3 | ~150 |
| `selection_manager.rs` | 3 | ~200 |
| `input_mode_manager.rs` | 3 | ~150 |
| `tui_module.rs` (rune) | Scripting | ~100 |
| `tui_module.rs` (steel) | Scripting | ~80 |
| `tui_module.rs` (lua) | Scripting | ~100 |
| `interaction_tests.rs` | Testing | ~200 |

### Files to MODIFY

| File | Phase | Changes |
|------|-------|---------|
| `state.rs` | 1 | Remove duplicated fields |
| `conversation_view.rs` | 1 | Minor accessor updates |
| `harness.rs` | 1 | Remove sync, update accessors |
| `components/mod.rs` | 2 | Delete WidgetEventResult/Action |
| `event_result.rs` | 2 | Expand TuiAction |
| `runner.rs` | 3 | Extract to managers |
| `generic_popup.rs` | 4 | Delete LegacyPopupItem |

### Files to DELETE

None - all changes are modifications or additions.
