---
description: TUI component system architecture and widget design
tags:
  - tui
  - architecture
  - components
status: implemented
---

# Component Architecture

The TUI uses a composable widget architecture built on ratatui's `Widget` trait, extended with interactive capabilities.

## Core Traits

### Widget (ratatui)

Standard ratatui trait for stateless rendering:

```rust
pub trait Widget {
    fn render(self, area: Rect, buf: &mut Buffer);
}
```

### InteractiveWidget

Extension trait adding event handling:

```rust
pub trait InteractiveWidget: Widget {
    fn handle_event(&mut self, event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn focusable(&self) -> bool {
        false
    }
}
```

## Event System

### EventResult

Controls event propagation:

```rust
pub enum EventResult {
    /// Event handled, stop propagation
    Consumed,
    /// Event not handled, continue to next widget
    Ignored,
    /// Delegate action to runner
    Action(TuiAction),
}
```

### TuiAction

High-level actions widgets request from the runner:

```rust
pub enum TuiAction {
    Scroll(isize),
    ScrollTo(usize),
    ConfirmPopup(usize),
    DismissPopup,
    CycleMode,
    RequestFocus(FocusTarget),
    CloseDialog(DialogAction),
}
```

## Components

### SessionHistoryWidget

Renders conversation history with scroll support.

**State:**
- `conversation: &ConversationState`
- `scroll_offset: usize`
- `viewport_height: u16`

**Events:** Scroll navigation (Ctrl+Up/Down, Page Up/Down, Home/End)

**Location:** `crates/crucible-cli/src/tui/components/session_history.rs`

### InputBoxWidget

Text input with cursor positioning.

**State:**
- `buffer: &str`
- `cursor_position: usize`
- `prompt: &str`
- `focused: bool`

**Events:** Display only (input handled by runner)

**Location:** `crates/crucible-cli/src/tui/components/input_box.rs`

### StatusBarWidget

Mode indicator and status display.

**State:**
- `mode_id: &str`
- `status_text: &str`
- `token_count: Option<usize>`
- `notification: Option<(NotificationLevel, &str)>`

**Events:** None (display only)

**Location:** `crates/crucible-cli/src/tui/components/status_bar.rs`

### PopupWidget

Autocomplete popup for commands and files.

**State:**
- `items: &[PopupItem]`
- `selected: usize`
- `viewport_offset: usize`
- `kind: PopupKind`

**Events:** Navigation (Up/Down, j/k), confirmation (Enter/Tab), dismiss (Esc)

**Location:** `crates/crucible-cli/src/tui/components/popup.rs`

### DialogWidget

Modal dialogs for confirmations and selections.

**State:**
- `dialog: &DialogState`

**Events:** Confirm (Enter/Y), cancel (Esc/N), navigation for select dialogs

**Focus Trap:** Captures all events when active

**Location:** `crates/crucible-cli/src/tui/components/dialog.rs`

### LayerStack

Coordinates event routing through UI layers.

**Layers:**
1. Modal (highest priority, captures all)
2. Popup (when focused)
3. Base (fallback)

**Location:** `crates/crucible-cli/src/tui/components/layer_stack.rs`

## Rendering Pipeline

```
RatatuiView::render_frame()
    │
    ├── Calculate layout constraints
    │
    ├── Render base layer
    │   ├── SessionHistoryWidget
    │   ├── InputBoxWidget
    │   └── StatusBarWidget
    │
    ├── Render popup (if active)
    │   └── PopupWidget
    │
    └── Render modal (if active)
        └── DialogWidget
```

## Testing

Components are tested via:

1. **Unit tests** - Event handling, state transitions
2. **Snapshot tests** - Visual rendering with insta

Example snapshot test:

```rust
#[test]
fn test_empty_conversation() {
    let state = ConversationState::new();
    let widget = SessionHistoryWidget::new(&state);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|f| f.render_widget(widget, f.area())).unwrap();

    insta::assert_snapshot!(terminal.backend());
}
```

## See Also

- [[Help/TUI/Index]] - TUI overview
- [[Help/TUI/Rune API]] - Scripting interface
- [[Help/Extending/Creating Plugins]] - Plugin development
