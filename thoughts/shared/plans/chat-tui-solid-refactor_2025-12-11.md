# Chat TUI SOLID Refactor Design

**Date:** 2025-12-11
**Status:** Design Complete, Ready for Implementation
**Goal:** Refactor chat_tui to adhere to SOLID principles, enable configurable keybindings, and prepare for multi-platform support.

## Problem Statement

The current chat_tui implementation has several SOLID violations identified by QA:

1. **SRP Violation:** `ChatApp` handles state, keybindings, completion lifecycle, and UI decisions
2. **OCP Violation:** Hardcoded completion items, keybindings scattered across 3 files
3. **ISP Violation:** Tight coupling between rendering and event handling
4. **No configurability:** Users cannot remap keybindings
5. **Platform coupling:** Direct use of crossterm types throughout

## Design Decisions

### 1. Two-Enum Action Pattern

Separate input triggers from output events:

```rust
// What keybindings trigger (no data payload)
pub enum KeyAction {
    Submit,
    Quit,
    Cancel,
    InsertNewline,
    CompletionNext,
    CompletionPrev,
    // ...
}

// What the system produces (carries data)
pub enum ChatEvent {
    SendMessage(String),
    CommandExecuted(String),
    Exit,
    None,
}
```

**Rationale:** KeyAction is serializable for config files, ChatEvent carries runtime data.

### 2. Platform-Agnostic Key Representation

```rust
/// Key code enum (platform-agnostic)
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Up, Down, Left, Right,
    Home, End,
    PageUp, PageDown,
    Space,
    F(u8),
}

/// Modifier bitflags (Helix-style)
bitflags! {
    pub struct Modifiers: u8 {
        const NONE     = 0b0000;
        const SHIFT    = 0b0001;
        const CONTROL  = 0b0010;
        const ALT      = 0b0100;
        const PLATFORM = 0b1000;  // Cmd on macOS, Ctrl on Windows/Linux
    }
}

/// Key pattern for matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}
```

**Rationale:**
- Bitflags are efficient (`Copy`, single `u8`)
- `PLATFORM` modifier handles Cmd/Ctrl cross-platform
- Matches Helix's proven approach

### 3. Layered Keybinding Maps

```rust
pub struct KeyBindings {
    global: HashMap<KeyPattern, KeyAction>,      // Always checked first
    modes: HashMap<InputMode, HashMap<KeyPattern, KeyAction>>,
}

impl KeyBindings {
    pub fn resolve(&self, key: KeyPattern, mode: InputMode) -> Option<KeyAction> {
        // 1. Check global first (Ctrl+C, Ctrl+D)
        if let Some(action) = self.global.get(&key) {
            return Some(*action);
        }
        // 2. Check mode-specific
        self.modes.get(&mode)?.get(&key).copied()
    }
}
```

**Rationale:**
- Global bindings defined once
- Mode-specific layers for Normal, Completion
- Extensible for future vim modes (VimNormal, VimInsert, VimVisual)

### 4. Input Mode Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Completion,
    // Future: VimNormal, VimInsert, VimVisual
}
```

### 5. Session Actions (Special Commands)

Session-level operations separate from agent commands:

```rust
pub enum SessionAction {
    Clear,              // Reset conversation display
    Exit,               // End session
    SetMode(ChatMode),  // Change Plan/Act/Auto
    ToggleMode,
}
```

**Rationale:** `/clear`, `/exit` are session controls, not agent-routed commands.

### 6. Completion Source Injection

Builder pattern with hot-reload support:

```rust
impl ChatApp {
    pub fn new(keybindings: KeyBindings) -> Self { ... }

    // Builder-style
    pub fn with_command_source(mut self, source: Box<dyn CompletionSource>) -> Self;
    pub fn with_file_source(mut self, source: Box<dyn CompletionSource>) -> Self;

    // Hot-reload mutations (for ACP dynamic commands)
    pub fn set_command_source(&mut self, source: Box<dyn CompletionSource>);
    pub fn clear_command_source(&mut self);
}
```

## Module Structure

### crucible-core/src/traits/input.rs (NEW)

Platform-agnostic types shared across CLI, Web, Desktop:

```rust
// Types
pub enum KeyCode { ... }
pub struct Modifiers { ... }  // bitflags
pub struct KeyPattern { ... }
pub enum InputMode { ... }
pub enum KeyAction { ... }
pub enum ChatEvent { ... }
pub enum SessionAction { ... }
```

### crucible-cli/src/chat_tui/ (REFACTORED)

```
chat_tui/
├── mod.rs              # Public API, setup_inline_terminal
├── app.rs              # ChatApp (slimmed - state + orchestration)
├── keybindings.rs      # KeyBindings struct, default mappings
├── convert.rs          # crossterm::KeyEvent → KeyPattern conversion
├── input.rs            # ChatInput (tui-textarea wrapper, unchanged)
├── completion.rs       # CompletionState (unchanged)
├── sources.rs          # Completion sources (existing)
├── event_loop.rs       # Event loop (simplified, uses KeyBindings)
├── render.rs           # Rendering (unchanged)
├── messages.rs         # Message display (unchanged)
└── widgets/            # Widgets (unchanged)
```

## Code to Delete

1. `handle_key_event` in event_loop.rs (duplicate of `handle_key_with_agent`)
2. `event_loop_inner` in event_loop.rs (unused simpler loop)
3. `run_event_loop` in event_loop.rs (not wired in)
4. Hardcoded completion items in `show_command_completion()` (app.rs:250-258)
5. Hardcoded completion items in `show_file_completion()` (app.rs:263-272)
6. Scattered keybinding logic in app.rs, input.rs, event_loop.rs

## Data Flow

```
crossterm::KeyEvent
    ↓
convert.rs: Into<KeyPattern>
    ↓
KeyBindings::resolve(pattern, mode) → Option<KeyAction>
    ↓
ChatApp::execute(action) → ChatEvent
    ↓
Event loop handles ChatEvent (send message, exit, etc.)
```

## Default Keybindings

### Global (all modes)
| Key | Action |
|-----|--------|
| Ctrl+D | Quit |
| Ctrl+C | Cancel (exit or close completion) |

### Normal Mode
| Key | Action |
|-----|--------|
| Enter | Submit |
| Ctrl+Enter | InsertNewline |
| Shift+Enter | InsertNewline |
| / (at word start) | TriggerCommandCompletion |
| @ (at word start) | TriggerFileCompletion |

### Completion Mode
| Key | Action |
|-----|--------|
| Up, Ctrl+K | CompletionPrev |
| Down, Ctrl+J | CompletionNext |
| Enter, Tab | CompletionConfirm |
| Escape | CompletionCancel |
| Space | ToggleSelection (multi-select only) |
| Backspace | FilterBackspace (cancel if empty) |
| Char(c) | FilterChar(c) |

## Test Strategy

1. **Unit tests for KeyBindings:** resolution logic, layering, defaults
2. **Unit tests for conversion:** crossterm → KeyPattern
3. **Existing tests preserved:** All 144 tests should continue passing
4. **New tests for KeyAction execution:** action → state change → ChatEvent

## Migration Path

### Phase 1: Add new types (no breaking changes)
- Add `crucible-core/src/traits/input.rs`
- Add `chat_tui/keybindings.rs`
- Add `chat_tui/convert.rs`

### Phase 2: Refactor ChatApp
- Inject KeyBindings via constructor
- Replace hardcoded key matching with `keybindings.resolve()`
- Replace hardcoded completion items with source injection

### Phase 3: Delete dead code
- Remove duplicate functions
- Remove hardcoded items

### Phase 4: Wire integration
- Connect chat_tui to actual chat command
- Test end-to-end

## Success Criteria

- [ ] All SOLID principles adhered to
- [ ] Keybindings configurable via KeyBindings struct
- [ ] No hardcoded completion items
- [ ] No hardcoded keybindings in app.rs/input.rs/event_loop.rs
- [ ] All 144 existing tests pass
- [ ] New tests for keybinding resolution
- [ ] Platform-agnostic types in crucible-core

## References

- [Helix keymap implementation](https://github.com/helix-editor/helix)
- [GPUI keystroke.rs](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/platform/keystroke.rs)
- [Ratatui backend comparison](https://ratatui.rs/concepts/backends/comparison/)
