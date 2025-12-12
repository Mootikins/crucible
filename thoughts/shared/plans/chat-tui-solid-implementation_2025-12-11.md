# Chat TUI SOLID Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor chat_tui to adhere to SOLID principles with configurable keybindings and platform-agnostic input handling.

**Architecture:** Two-enum pattern (KeyAction triggers, ChatEvent outputs) with layered keybindings (global → mode-specific). Platform-agnostic types in crucible-core, CLI-specific keybindings in chat_tui.

**Tech Stack:** Rust, bitflags, ratatui, crossterm

---

## Wave 0: Dependencies

### Task 0.1: Add bitflags to crucible-core

**Files:**
- Modify: `crates/crucible-core/Cargo.toml`

**Step 1: Add bitflags dependency**

```toml
# Add after line 26 (after sha2 = "0.10")
bitflags = "2.4"
```

**Step 2: Verify build**

Run: `cargo build -p crucible-core`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/crucible-core/Cargo.toml
git commit -m "build(core): add bitflags dependency for input modifiers"
```

---

## Wave 1: Core Input Types (crucible-core)

### Task 1.1: Create input.rs with KeyCode enum

**Files:**
- Create: `crates/crucible-core/src/traits/input.rs`
- Modify: `crates/crucible-core/src/traits/mod.rs`

**Step 1: Write failing test for KeyCode**

Create file `crates/crucible-core/src/traits/input.rs`:

```rust
//! Input abstraction traits for cross-platform UI
//!
//! Provides platform-agnostic key representation and action types
//! that can be shared across CLI, Web, and Desktop interfaces.

/// Platform-agnostic key code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A character key
    Char(char),
    /// Enter/Return key
    Enter,
    /// Escape key
    Escape,
    /// Backspace key
    Backspace,
    /// Delete key
    Delete,
    /// Tab key
    Tab,
    /// Space bar
    Space,
    /// Up arrow
    Up,
    /// Down arrow
    Down,
    /// Left arrow
    Left,
    /// Right arrow
    Right,
    /// Home key
    Home,
    /// End key
    End,
    /// Page Up key
    PageUp,
    /// Page Down key
    PageDown,
    /// Function key (F1-F12)
    F(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keycode_char_equality() {
        assert_eq!(KeyCode::Char('a'), KeyCode::Char('a'));
        assert_ne!(KeyCode::Char('a'), KeyCode::Char('b'));
    }

    #[test]
    fn test_keycode_special_keys() {
        assert_eq!(KeyCode::Enter, KeyCode::Enter);
        assert_ne!(KeyCode::Enter, KeyCode::Escape);
    }

    #[test]
    fn test_keycode_function_keys() {
        assert_eq!(KeyCode::F(1), KeyCode::F(1));
        assert_ne!(KeyCode::F(1), KeyCode::F(2));
    }

    #[test]
    fn test_keycode_is_copy() {
        let k = KeyCode::Enter;
        let k2 = k; // Copy
        assert_eq!(k, k2);
    }

    #[test]
    fn test_keycode_is_hashable() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyCode::Enter);
        set.insert(KeyCode::Escape);
        assert!(set.contains(&KeyCode::Enter));
    }
}
```

**Step 2: Register module in mod.rs**

Add to `crates/crucible-core/src/traits/mod.rs` after line 37:

```rust
pub mod input;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test -p crucible-core input::tests`
Expected: All 5 tests pass

**Step 4: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs crates/crucible-core/src/traits/mod.rs
git commit -m "feat(core): add KeyCode enum for platform-agnostic input"
```

---

### Task 1.2: Add Modifiers bitflags

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write failing test for Modifiers**

Add to `crates/crucible-core/src/traits/input.rs` after KeyCode enum:

```rust
use bitflags::bitflags;

bitflags! {
    /// Modifier key flags
    ///
    /// Uses bitflags for efficient storage and combination.
    /// PLATFORM represents Cmd on macOS, Ctrl on Windows/Linux.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u8 {
        /// No modifiers
        const NONE = 0b0000;
        /// Shift key
        const SHIFT = 0b0001;
        /// Control key
        const CONTROL = 0b0010;
        /// Alt/Option key
        const ALT = 0b0100;
        /// Platform key (Cmd on macOS, Win on Windows)
        const PLATFORM = 0b1000;
    }
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers::NONE
    }
}
```

Add tests to the tests module:

```rust
    #[test]
    fn test_modifiers_none() {
        let m = Modifiers::NONE;
        assert!(!m.contains(Modifiers::CONTROL));
        assert!(!m.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_modifiers_single() {
        let m = Modifiers::CONTROL;
        assert!(m.contains(Modifiers::CONTROL));
        assert!(!m.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_modifiers_combination() {
        let m = Modifiers::CONTROL | Modifiers::SHIFT;
        assert!(m.contains(Modifiers::CONTROL));
        assert!(m.contains(Modifiers::SHIFT));
        assert!(!m.contains(Modifiers::ALT));
    }

    #[test]
    fn test_modifiers_default_is_none() {
        assert_eq!(Modifiers::default(), Modifiers::NONE);
    }

    #[test]
    fn test_modifiers_is_copy() {
        let m = Modifiers::CONTROL;
        let m2 = m;
        assert_eq!(m, m2);
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 10 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add Modifiers bitflags for key modifiers"
```

---

### Task 1.3: Add KeyPattern struct

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write KeyPattern and tests**

Add after Modifiers impl:

```rust
/// Platform-agnostic key pattern for keybinding matching
///
/// Combines a key code with modifier flags. This is the lookup key
/// for keybinding resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPattern {
    /// The primary key
    pub key: KeyCode,
    /// Active modifier keys
    pub modifiers: Modifiers,
}

impl KeyPattern {
    /// Create a new key pattern
    pub fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Create a pattern for a plain character (no modifiers)
    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), Modifiers::NONE)
    }

    /// Create a pattern for Ctrl+character
    pub fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c), Modifiers::CONTROL)
    }

    /// Create a pattern for a special key (no modifiers)
    pub fn key(key: KeyCode) -> Self {
        Self::new(key, Modifiers::NONE)
    }
}
```

Add tests:

```rust
    #[test]
    fn test_key_pattern_char() {
        let p = KeyPattern::char('a');
        assert_eq!(p.key, KeyCode::Char('a'));
        assert_eq!(p.modifiers, Modifiers::NONE);
    }

    #[test]
    fn test_key_pattern_ctrl() {
        let p = KeyPattern::ctrl('c');
        assert_eq!(p.key, KeyCode::Char('c'));
        assert!(p.modifiers.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_key_pattern_equality() {
        let p1 = KeyPattern::ctrl('c');
        let p2 = KeyPattern::ctrl('c');
        let p3 = KeyPattern::char('c');
        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn test_key_pattern_hashable() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(KeyPattern::ctrl('c'), "quit");
        assert_eq!(map.get(&KeyPattern::ctrl('c')), Some(&"quit"));
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 14 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add KeyPattern for keybinding matching"
```

---

### Task 1.4: Add InputMode enum

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write InputMode and tests**

Add after KeyPattern:

```rust
/// Input mode for modal key handling
///
/// Determines which keybinding layer is active.
/// Extensible for future vim modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputMode {
    /// Normal text input mode
    #[default]
    Normal,
    /// Completion popup is active
    Completion,
    // Future: VimNormal, VimInsert, VimVisual
}
```

Add tests:

```rust
    #[test]
    fn test_input_mode_default() {
        assert_eq!(InputMode::default(), InputMode::Normal);
    }

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_ne!(InputMode::Normal, InputMode::Completion);
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 16 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add InputMode enum for modal keybindings"
```

---

### Task 1.5: Add KeyAction enum

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write KeyAction and tests**

Add after InputMode:

```rust
/// Action triggered by a keybinding
///
/// These are the abstract actions that keybindings map to.
/// They carry no data - that's handled by ChatEvent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    // === Text Editing ===
    /// Submit the current input
    Submit,
    /// Insert a newline character
    InsertNewline,

    // === Completion Navigation ===
    /// Trigger command completion (/)
    TriggerCommandCompletion,
    /// Trigger file/agent completion (@)
    TriggerFileCompletion,
    /// Move to next completion item
    CompletionNext,
    /// Move to previous completion item
    CompletionPrev,
    /// Confirm the current completion selection
    CompletionConfirm,
    /// Cancel completion without selecting
    CompletionCancel,
    /// Toggle selection in multi-select mode
    ToggleSelection,

    // === Global ===
    /// Quit the application
    Quit,
    /// Cancel current operation (close popup or exit)
    Cancel,

    // === Passthrough ===
    /// Let the underlying widget handle this key
    Passthrough,
}
```

Add tests:

```rust
    #[test]
    fn test_key_action_is_copy() {
        let a = KeyAction::Submit;
        let a2 = a;
        assert_eq!(a, a2);
    }

    #[test]
    fn test_key_action_variants() {
        assert_ne!(KeyAction::Submit, KeyAction::Quit);
        assert_eq!(KeyAction::CompletionNext, KeyAction::CompletionNext);
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 18 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add KeyAction enum for keybinding actions"
```

---

### Task 1.6: Add ChatEvent enum

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write ChatEvent and tests**

Add after KeyAction:

```rust
/// Event emitted by the chat system
///
/// These are the outputs that carry data, produced by
/// executing KeyActions on the current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    /// Send a message to the agent
    SendMessage(String),
    /// A local command was executed
    CommandExecuted(String),
    /// Request to exit the chat
    Exit,
    /// No event produced
    None,
}
```

Add tests:

```rust
    #[test]
    fn test_chat_event_with_data() {
        let e = ChatEvent::SendMessage("hello".to_string());
        if let ChatEvent::SendMessage(msg) = e {
            assert_eq!(msg, "hello");
        } else {
            panic!("Expected SendMessage");
        }
    }

    #[test]
    fn test_chat_event_equality() {
        assert_eq!(ChatEvent::Exit, ChatEvent::Exit);
        assert_eq!(ChatEvent::None, ChatEvent::None);
        assert_ne!(ChatEvent::Exit, ChatEvent::None);
    }

    #[test]
    fn test_chat_event_clone() {
        let e = ChatEvent::SendMessage("test".to_string());
        let e2 = e.clone();
        assert_eq!(e, e2);
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 21 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add ChatEvent enum for output events"
```

---

### Task 1.7: Add SessionAction enum

**Files:**
- Modify: `crates/crucible-core/src/traits/input.rs`

**Step 1: Write SessionAction and tests**

Add after ChatEvent:

```rust
use super::chat::ChatMode;

/// Session-level operations (not sent to agent)
///
/// These are local commands like /clear and /exit that
/// modify the session state without agent involvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    /// Clear conversation display
    Clear,
    /// Exit the session
    Exit,
    /// Set chat mode
    SetMode(ChatMode),
    /// Toggle to next chat mode
    ToggleMode,
}
```

Add tests:

```rust
    use super::super::chat::ChatMode;

    #[test]
    fn test_session_action_variants() {
        assert_eq!(SessionAction::Clear, SessionAction::Clear);
        assert_ne!(SessionAction::Clear, SessionAction::Exit);
    }

    #[test]
    fn test_session_action_with_mode() {
        let a = SessionAction::SetMode(ChatMode::Plan);
        if let SessionAction::SetMode(mode) = a {
            assert_eq!(mode, ChatMode::Plan);
        } else {
            panic!("Expected SetMode");
        }
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-core input::tests`
Expected: All 23 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/input.rs
git commit -m "feat(core): add SessionAction for local session commands"
```

---

### Task 1.8: Add re-exports to traits/mod.rs

**Files:**
- Modify: `crates/crucible-core/src/traits/mod.rs`

**Step 1: Add re-exports**

Add after line 54 in `crates/crucible-core/src/traits/mod.rs`:

```rust
pub use input::{ChatEvent, InputMode, KeyAction, KeyCode, KeyPattern, Modifiers, SessionAction};
```

**Step 2: Verify build**

Run: `cargo build -p crucible-core`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/crucible-core/src/traits/mod.rs
git commit -m "feat(core): re-export input types from traits module"
```

---

## Wave 1 QA Checkpoint

Run: `cargo test -p crucible-core`
Expected: All core tests pass, including 23 new input tests

Run: `cargo build --workspace`
Expected: Full workspace builds

---

## Wave 2: CLI Keybindings

### Task 2.1: Create keybindings.rs with KeyBindings struct

**Files:**
- Create: `crates/crucible-cli/src/chat_tui/keybindings.rs`
- Modify: `crates/crucible-cli/src/chat_tui/mod.rs`

**Step 1: Write failing test**

Create `crates/crucible-cli/src/chat_tui/keybindings.rs`:

```rust
//! Keybinding configuration for the chat TUI
//!
//! Provides layered keybinding resolution: global → mode-specific.
//! Supports future vim modes by using HashMap<InputMode, ...>.

use std::collections::HashMap;

use crucible_core::traits::{InputMode, KeyAction, KeyPattern};

/// Layered keybinding configuration
///
/// Resolution order:
/// 1. Check global bindings first
/// 2. Check mode-specific bindings
#[derive(Debug, Clone)]
pub struct KeyBindings {
    /// Global bindings (active in all modes)
    global: HashMap<KeyPattern, KeyAction>,
    /// Mode-specific bindings
    modes: HashMap<InputMode, HashMap<KeyPattern, KeyAction>>,
}

impl KeyBindings {
    /// Create empty keybindings
    pub fn new() -> Self {
        Self {
            global: HashMap::new(),
            modes: HashMap::new(),
        }
    }

    /// Resolve a key pattern to an action
    ///
    /// Checks global bindings first, then mode-specific.
    pub fn resolve(&self, key: KeyPattern, mode: InputMode) -> Option<KeyAction> {
        // 1. Check global first
        if let Some(action) = self.global.get(&key) {
            return Some(*action);
        }
        // 2. Check mode-specific
        self.modes.get(&mode)?.get(&key).copied()
    }

    /// Bind a key globally (all modes)
    pub fn bind_global(&mut self, key: KeyPattern, action: KeyAction) {
        self.global.insert(key, action);
    }

    /// Bind a key for a specific mode
    pub fn bind_mode(&mut self, mode: InputMode, key: KeyPattern, action: KeyAction) {
        self.modes.entry(mode).or_default().insert(key, action);
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::traits::{KeyCode, Modifiers};

    #[test]
    fn test_empty_bindings_returns_none() {
        let bindings = KeyBindings::new();
        let result = bindings.resolve(KeyPattern::char('a'), InputMode::Normal);
        assert!(result.is_none());
    }

    #[test]
    fn test_global_binding() {
        let mut bindings = KeyBindings::new();
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Cancel);

        // Should work in any mode
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Cancel)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Completion),
            Some(KeyAction::Cancel)
        );
    }

    #[test]
    fn test_mode_specific_binding() {
        let mut bindings = KeyBindings::new();
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::Submit,
        );

        // Should work in Normal mode
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Normal),
            Some(KeyAction::Submit)
        );
        // Should NOT work in Completion mode
        assert!(bindings
            .resolve(KeyPattern::key(KeyCode::Enter), InputMode::Completion)
            .is_none());
    }

    #[test]
    fn test_global_takes_precedence() {
        let mut bindings = KeyBindings::new();

        // Bind same key globally and for mode
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Quit);
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::ctrl('c'),
            KeyAction::Cancel,
        );

        // Global should win
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Quit)
        );
    }
}
```

**Step 2: Add module to mod.rs**

Add to `crates/crucible-cli/src/chat_tui/mod.rs` after line 31 (after `pub mod widgets;`):

```rust
mod keybindings;
pub use keybindings::KeyBindings;
```

**Step 3: Run tests**

Run: `cargo test -p crucible-cli keybindings::tests`
Expected: All 4 tests pass

**Step 4: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/keybindings.rs crates/crucible-cli/src/chat_tui/mod.rs
git commit -m "feat(chat-tui): add KeyBindings with layered resolution"
```

---

### Task 2.2: Add default keybindings

**Files:**
- Modify: `crates/crucible-cli/src/chat_tui/keybindings.rs`

**Step 1: Write failing test for defaults**

Add to keybindings.rs:

```rust
impl KeyBindings {
    /// Create default keybindings matching current behavior
    pub fn defaults() -> Self {
        let mut bindings = Self::new();

        // === Global bindings ===
        bindings.bind_global(KeyPattern::ctrl('d'), KeyAction::Quit);
        bindings.bind_global(KeyPattern::ctrl('c'), KeyAction::Cancel);

        // === Normal mode ===
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::Submit,
        );
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::new(KeyCode::Enter, Modifiers::CONTROL),
            KeyAction::InsertNewline,
        );
        bindings.bind_mode(
            InputMode::Normal,
            KeyPattern::new(KeyCode::Enter, Modifiers::SHIFT),
            KeyAction::InsertNewline,
        );

        // === Completion mode ===
        // Navigation
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Up),
            KeyAction::CompletionPrev,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Down),
            KeyAction::CompletionNext,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::ctrl('k'),
            KeyAction::CompletionPrev,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::ctrl('j'),
            KeyAction::CompletionNext,
        );

        // Selection
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Enter),
            KeyAction::CompletionConfirm,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Tab),
            KeyAction::CompletionConfirm,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Escape),
            KeyAction::CompletionCancel,
        );
        bindings.bind_mode(
            InputMode::Completion,
            KeyPattern::key(KeyCode::Space),
            KeyAction::ToggleSelection,
        );

        bindings
    }
}
```

Add to imports at top:

```rust
use crucible_core::traits::{InputMode, KeyAction, KeyCode, KeyPattern, Modifiers};
```

Add tests:

```rust
    #[test]
    fn test_defaults_global_ctrl_d_quits() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('d'), InputMode::Normal),
            Some(KeyAction::Quit)
        );
    }

    #[test]
    fn test_defaults_ctrl_c_cancels() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Normal),
            Some(KeyAction::Cancel)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('c'), InputMode::Completion),
            Some(KeyAction::Cancel)
        );
    }

    #[test]
    fn test_defaults_enter_submits_in_normal() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Normal),
            Some(KeyAction::Submit)
        );
    }

    #[test]
    fn test_defaults_ctrl_enter_newline() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.resolve(
                KeyPattern::new(KeyCode::Enter, Modifiers::CONTROL),
                InputMode::Normal
            ),
            Some(KeyAction::InsertNewline)
        );
    }

    #[test]
    fn test_defaults_completion_navigation() {
        let bindings = KeyBindings::defaults();

        // Arrow keys
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Up), InputMode::Completion),
            Some(KeyAction::CompletionPrev)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Down), InputMode::Completion),
            Some(KeyAction::CompletionNext)
        );

        // Ctrl+J/K
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('j'), InputMode::Completion),
            Some(KeyAction::CompletionNext)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::ctrl('k'), InputMode::Completion),
            Some(KeyAction::CompletionPrev)
        );
    }

    #[test]
    fn test_defaults_completion_confirm() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Enter), InputMode::Completion),
            Some(KeyAction::CompletionConfirm)
        );
        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Tab), InputMode::Completion),
            Some(KeyAction::CompletionConfirm)
        );
    }

    #[test]
    fn test_defaults_completion_cancel() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Escape), InputMode::Completion),
            Some(KeyAction::CompletionCancel)
        );
    }

    #[test]
    fn test_defaults_toggle_selection() {
        let bindings = KeyBindings::defaults();

        assert_eq!(
            bindings.resolve(KeyPattern::key(KeyCode::Space), InputMode::Completion),
            Some(KeyAction::ToggleSelection)
        );
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-cli keybindings::tests`
Expected: All 12 tests pass

**Step 3: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/keybindings.rs
git commit -m "feat(chat-tui): add default keybindings matching current behavior"
```

---

### Task 2.3: Create convert.rs for crossterm conversion

**Files:**
- Create: `crates/crucible-cli/src/chat_tui/convert.rs`
- Modify: `crates/crucible-cli/src/chat_tui/mod.rs`

**Step 1: Write conversion and tests**

Create `crates/crucible-cli/src/chat_tui/convert.rs`:

```rust
//! Conversion from crossterm key events to platform-agnostic KeyPattern
//!
//! This module provides the bridge between crossterm's input events
//! and our platform-agnostic KeyPattern type.

use crucible_core::traits::{KeyCode, KeyPattern, Modifiers};
use ratatui::crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers};

/// Convert crossterm KeyCode to our KeyCode
impl From<CtKeyCode> for KeyCode {
    fn from(key: CtKeyCode) -> Self {
        match key {
            CtKeyCode::Char(c) => KeyCode::Char(c),
            CtKeyCode::Enter => KeyCode::Enter,
            CtKeyCode::Esc => KeyCode::Escape,
            CtKeyCode::Backspace => KeyCode::Backspace,
            CtKeyCode::Delete => KeyCode::Delete,
            CtKeyCode::Tab => KeyCode::Tab,
            CtKeyCode::Up => KeyCode::Up,
            CtKeyCode::Down => KeyCode::Down,
            CtKeyCode::Left => KeyCode::Left,
            CtKeyCode::Right => KeyCode::Right,
            CtKeyCode::Home => KeyCode::Home,
            CtKeyCode::End => KeyCode::End,
            CtKeyCode::PageUp => KeyCode::PageUp,
            CtKeyCode::PageDown => KeyCode::PageDown,
            CtKeyCode::F(n) => KeyCode::F(n),
            // Map other keys to Space as fallback (or could use an Unknown variant)
            _ => KeyCode::Space,
        }
    }
}

/// Convert crossterm KeyModifiers to our Modifiers
impl From<KeyModifiers> for Modifiers {
    fn from(mods: KeyModifiers) -> Self {
        let mut result = Modifiers::NONE;
        if mods.contains(KeyModifiers::SHIFT) {
            result |= Modifiers::SHIFT;
        }
        if mods.contains(KeyModifiers::CONTROL) {
            result |= Modifiers::CONTROL;
        }
        if mods.contains(KeyModifiers::ALT) {
            result |= Modifiers::ALT;
        }
        // Note: crossterm doesn't have a SUPER/META modifier directly
        // PLATFORM would be set based on OS detection if needed
        result
    }
}

/// Convert crossterm KeyEvent to KeyPattern
impl From<KeyEvent> for KeyPattern {
    fn from(event: KeyEvent) -> Self {
        KeyPattern {
            key: event.code.into(),
            modifiers: event.modifiers.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keycode_conversion_char() {
        let ct = CtKeyCode::Char('a');
        let kc: KeyCode = ct.into();
        assert_eq!(kc, KeyCode::Char('a'));
    }

    #[test]
    fn test_keycode_conversion_special() {
        assert_eq!(KeyCode::from(CtKeyCode::Enter), KeyCode::Enter);
        assert_eq!(KeyCode::from(CtKeyCode::Esc), KeyCode::Escape);
        assert_eq!(KeyCode::from(CtKeyCode::Backspace), KeyCode::Backspace);
        assert_eq!(KeyCode::from(CtKeyCode::Tab), KeyCode::Tab);
    }

    #[test]
    fn test_keycode_conversion_arrows() {
        assert_eq!(KeyCode::from(CtKeyCode::Up), KeyCode::Up);
        assert_eq!(KeyCode::from(CtKeyCode::Down), KeyCode::Down);
        assert_eq!(KeyCode::from(CtKeyCode::Left), KeyCode::Left);
        assert_eq!(KeyCode::from(CtKeyCode::Right), KeyCode::Right);
    }

    #[test]
    fn test_keycode_conversion_function() {
        assert_eq!(KeyCode::from(CtKeyCode::F(1)), KeyCode::F(1));
        assert_eq!(KeyCode::from(CtKeyCode::F(12)), KeyCode::F(12));
    }

    #[test]
    fn test_modifiers_conversion_none() {
        let ct = KeyModifiers::NONE;
        let m: Modifiers = ct.into();
        assert_eq!(m, Modifiers::NONE);
    }

    #[test]
    fn test_modifiers_conversion_control() {
        let ct = KeyModifiers::CONTROL;
        let m: Modifiers = ct.into();
        assert!(m.contains(Modifiers::CONTROL));
        assert!(!m.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_modifiers_conversion_combined() {
        let ct = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let m: Modifiers = ct.into();
        assert!(m.contains(Modifiers::CONTROL));
        assert!(m.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_key_event_conversion() {
        let event = KeyEvent::new(CtKeyCode::Char('c'), KeyModifiers::CONTROL);
        let pattern: KeyPattern = event.into();

        assert_eq!(pattern.key, KeyCode::Char('c'));
        assert!(pattern.modifiers.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_key_event_conversion_no_modifiers() {
        let event = KeyEvent::new(CtKeyCode::Enter, KeyModifiers::NONE);
        let pattern: KeyPattern = event.into();

        assert_eq!(pattern.key, KeyCode::Enter);
        assert_eq!(pattern.modifiers, Modifiers::NONE);
    }
}
```

**Step 2: Add module to mod.rs**

Add to `crates/crucible-cli/src/chat_tui/mod.rs` after keybindings:

```rust
mod convert;
```

**Step 3: Run tests**

Run: `cargo test -p crucible-cli convert::tests`
Expected: All 10 tests pass

**Step 4: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/convert.rs crates/crucible-cli/src/chat_tui/mod.rs
git commit -m "feat(chat-tui): add crossterm to KeyPattern conversion"
```

---

## Wave 2 QA Checkpoint

Run: `cargo test -p crucible-cli`
Expected: All tests pass (144 existing + 22 new = 166+)

Run: `cargo clippy -p crucible-cli`
Expected: No warnings

---

## Wave 3: Refactor ChatApp

### Task 3.1: Add KeyBindings to ChatApp

**Files:**
- Modify: `crates/crucible-cli/src/chat_tui/app.rs`

**Step 1: Write failing test**

Add to app.rs imports:

```rust
use super::keybindings::KeyBindings;
use crucible_core::traits::InputMode as CoreInputMode;
```

Modify ChatApp struct (add field after render_state):

```rust
    /// Keybinding configuration
    pub keybindings: KeyBindings,
```

Modify ChatApp::new():

```rust
    pub fn new() -> Self {
        Self::with_keybindings(KeyBindings::defaults())
    }

    /// Create with custom keybindings
    pub fn with_keybindings(keybindings: KeyBindings) -> Self {
        Self {
            mode: ChatMode::default(),
            input: ChatInput::new(),
            completion: None,
            render_state: RenderState::new(),
            keybindings,
            is_streaming: false,
            should_exit: false,
        }
    }
```

Add test:

```rust
    #[test]
    fn test_chat_app_with_custom_keybindings() {
        use super::keybindings::KeyBindings;

        let bindings = KeyBindings::new(); // Empty bindings
        let app = ChatApp::with_keybindings(bindings);
        assert!(app.keybindings.resolve(
            crucible_core::traits::KeyPattern::ctrl('d'),
            crucible_core::traits::InputMode::Normal
        ).is_none()); // No defaults
    }

    #[test]
    fn test_chat_app_default_has_keybindings() {
        let app = ChatApp::new();
        // Should have default Ctrl+D binding
        assert!(app.keybindings.resolve(
            crucible_core::traits::KeyPattern::ctrl('d'),
            crucible_core::traits::InputMode::Normal
        ).is_some());
    }
```

**Step 2: Run tests**

Run: `cargo test -p crucible-cli chat_tui::app::tests`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/app.rs
git commit -m "feat(chat-tui): inject KeyBindings into ChatApp"
```

---

### Task 3.2: Add CompletionSource trait and injection

**Files:**
- Modify: `crates/crucible-cli/src/chat_tui/sources.rs`
- Modify: `crates/crucible-cli/src/chat_tui/app.rs`

**Step 1: Add trait to sources.rs**

Add at top of sources.rs after imports:

```rust
/// Trait for completion data sources
///
/// Implement this to provide completion items from various sources
/// (slash commands, files, agents, etc.)
pub trait CompletionSource: Send + Sync {
    /// Get all completion items from this source
    fn get_items(&self) -> Vec<CompletionItem>;

    /// Whether this source supports multi-select
    fn supports_multi_select(&self) -> bool {
        false
    }
}

impl CompletionSource for FileSource {
    fn get_items(&self) -> Vec<CompletionItem> {
        // Use existing method
        FileSource::get_items(self)
    }

    fn supports_multi_select(&self) -> bool {
        true // Files support multi-select
    }
}

/// Command source wrapper that implements CompletionSource
pub struct CommandSource {
    items: Vec<CompletionItem>,
}

impl CommandSource {
    /// Create from a slash command registry
    pub fn from_registry(registry: &SlashCommandRegistry) -> Self {
        Self {
            items: command_source(registry),
        }
    }

    /// Create from a list of items
    pub fn new(items: Vec<CompletionItem>) -> Self {
        Self { items }
    }
}

impl CompletionSource for CommandSource {
    fn get_items(&self) -> Vec<CompletionItem> {
        self.items.clone()
    }

    fn supports_multi_select(&self) -> bool {
        false // Commands are single-select
    }
}
```

**Step 2: Add source fields to ChatApp**

Add to ChatApp struct:

```rust
    /// Command completion source (optional)
    command_source: Option<Box<dyn CompletionSource>>,
    /// File completion source (optional)
    file_source: Option<Box<dyn CompletionSource>>,
```

Update constructors to initialize as None, add builder methods:

```rust
    /// Set command completion source (builder)
    pub fn with_command_source(mut self, source: Box<dyn CompletionSource>) -> Self {
        self.command_source = Some(source);
        self
    }

    /// Set file completion source (builder)
    pub fn with_file_source(mut self, source: Box<dyn CompletionSource>) -> Self {
        self.file_source = Some(source);
        self
    }

    /// Set command source (mutation for hot-reload)
    pub fn set_command_source(&mut self, source: Box<dyn CompletionSource>) {
        self.command_source = Some(source);
    }

    /// Clear command source
    pub fn clear_command_source(&mut self) {
        self.command_source = None;
    }
```

**Step 3: Update show_command_completion to use source**

Replace `show_command_completion`:

```rust
    /// Show command completion popup
    pub fn show_command_completion(&mut self) {
        let items = if let Some(source) = &self.command_source {
            source.get_items()
        } else {
            // Fallback to hardcoded items if no source configured
            vec![
                CompletionItem::new("clear", Some("Clear conversation".into()), CompletionType::Command),
                CompletionItem::new("help", Some("Show help".into()), CompletionType::Command),
                CompletionItem::new("mode", Some("Change mode".into()), CompletionType::Command),
                CompletionItem::new("exit", Some("Exit chat".into()), CompletionType::Command),
            ]
        };
        self.completion = Some(CompletionState::new(items, CompletionType::Command));
        self.render_state.mark_dirty();
    }
```

Replace `show_file_completion`:

```rust
    /// Show file completion popup
    pub fn show_file_completion(&mut self) {
        let (items, multi_select) = if let Some(source) = &self.file_source {
            (source.get_items(), source.supports_multi_select())
        } else {
            // Fallback
            (
                vec![
                    CompletionItem::new("README.md", None, CompletionType::File),
                    CompletionItem::new("CLAUDE.md", None, CompletionType::File),
                ],
                true,
            )
        };
        let mut state = CompletionState::new(items, CompletionType::File);
        state.multi_select = multi_select;
        self.completion = Some(state);
        self.render_state.mark_dirty();
    }
```

**Step 4: Add import to app.rs**

```rust
use super::sources::CompletionSource;
```

**Step 5: Add tests**

```rust
    #[test]
    fn test_chat_app_with_command_source() {
        use super::sources::{CommandSource, CompletionSource};
        use super::completion::CompletionItem;

        let items = vec![
            CompletionItem::new("test", Some("Test command".into()), CompletionType::Command),
        ];
        let source = CommandSource::new(items);

        let mut app = ChatApp::new().with_command_source(Box::new(source));
        app.show_command_completion();

        assert!(app.completion.is_some());
        let completion = app.completion.as_ref().unwrap();
        assert_eq!(completion.filtered_items.len(), 1);
        assert_eq!(completion.filtered_items[0].text, "test");
    }

    #[test]
    fn test_chat_app_hot_reload_command_source() {
        use super::sources::CommandSource;
        use super::completion::CompletionItem;

        let mut app = ChatApp::new();

        // Initially uses fallback
        app.show_command_completion();
        let initial_count = app.completion.as_ref().unwrap().filtered_items.len();
        app.completion = None;

        // Hot-reload new source
        let items = vec![
            CompletionItem::new("new1", None, CompletionType::Command),
            CompletionItem::new("new2", None, CompletionType::Command),
        ];
        app.set_command_source(Box::new(CommandSource::new(items)));

        app.show_command_completion();
        assert_eq!(app.completion.as_ref().unwrap().filtered_items.len(), 2);

        // Clear source returns to fallback
        app.completion = None;
        app.clear_command_source();
        app.show_command_completion();
        assert_eq!(
            app.completion.as_ref().unwrap().filtered_items.len(),
            initial_count
        );
    }
```

**Step 6: Run tests**

Run: `cargo test -p crucible-cli chat_tui`
Expected: All tests pass

**Step 7: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/sources.rs crates/crucible-cli/src/chat_tui/app.rs
git commit -m "feat(chat-tui): add CompletionSource trait with hot-reload support"
```

---

## Wave 3 QA Checkpoint

Run: `cargo test -p crucible-cli chat_tui`
Expected: All chat_tui tests pass

Run: `cargo clippy -p crucible-cli`
Expected: No warnings

---

## Wave 4: Refactor Event Loop

### Task 4.1: Use KeyBindings in handle_key_with_agent

**Files:**
- Modify: `crates/crucible-cli/src/chat_tui/event_loop.rs`

**Step 1: Update imports**

Add to imports:

```rust
use crucible_core::traits::{InputMode, KeyAction, KeyPattern};
```

**Step 2: Refactor handle_key_with_agent**

Replace the function with:

```rust
/// Handle a key event with agent integration
///
/// Uses KeyBindings for resolution, then executes the action.
fn handle_key_with_agent<B: ratatui::backend::Backend>(
    app: &mut ChatApp,
    key: KeyEvent,
    message_tx: &mpsc::UnboundedSender<String>,
    terminal: &mut Terminal<B>,
) -> Result<EventResult> {
    // Convert crossterm event to our KeyPattern
    let pattern: KeyPattern = key.into();

    // Determine current input mode
    let mode = if app.completion.is_some() {
        InputMode::Completion
    } else {
        InputMode::Normal
    };

    // Resolve keybinding
    if let Some(action) = app.keybindings.resolve(pattern, mode) {
        return execute_action(app, action, message_tx, terminal);
    }

    // No binding found - pass through to input widget
    if mode == InputMode::Normal {
        // Let ChatApp handle passthrough (typing, etc.)
        if let Some(content) = app.handle_key(key) {
            return handle_submitted_content(app, content, message_tx, terminal);
        }
    } else if mode == InputMode::Completion {
        // In completion mode, unbound chars go to filter
        if let KeyCode::Char(c) = key.code {
            if let Some(completion) = app.completion.as_mut() {
                completion.query.push(c);
                completion.refilter();
                app.render_state.mark_dirty();
            }
        } else if key.code == KeyCode::Backspace {
            if let Some(completion) = app.completion.as_mut() {
                if completion.query.pop().is_none() {
                    app.completion = None; // Cancel on empty backspace
                } else {
                    completion.refilter();
                }
                app.render_state.mark_dirty();
            }
        }
    }

    Ok(EventResult::Continue)
}

/// Execute a resolved KeyAction
fn execute_action<B: ratatui::backend::Backend>(
    app: &mut ChatApp,
    action: KeyAction,
    message_tx: &mpsc::UnboundedSender<String>,
    terminal: &mut Terminal<B>,
) -> Result<EventResult> {
    match action {
        KeyAction::Quit => Ok(EventResult::Quit),

        KeyAction::Cancel => {
            if app.completion.is_some() {
                app.completion = None;
                app.render_state.mark_dirty();
            } else {
                app.request_exit();
                return Ok(EventResult::Quit);
            }
            Ok(EventResult::Continue)
        }

        KeyAction::Submit => {
            if let Some(content) = app.submit_input() {
                return handle_submitted_content(app, content, message_tx, terminal);
            }
            Ok(EventResult::Continue)
        }

        KeyAction::InsertNewline => {
            app.input.textarea_mut().insert_newline();
            app.render_state.mark_dirty();
            Ok(EventResult::Continue)
        }

        KeyAction::TriggerCommandCompletion => {
            app.show_command_completion();
            Ok(EventResult::Continue)
        }

        KeyAction::TriggerFileCompletion => {
            app.show_file_completion();
            Ok(EventResult::Continue)
        }

        KeyAction::CompletionNext => {
            if let Some(completion) = app.completion.as_mut() {
                completion.select_next();
                app.render_state.mark_dirty();
            }
            Ok(EventResult::Continue)
        }

        KeyAction::CompletionPrev => {
            if let Some(completion) = app.completion.as_mut() {
                completion.select_prev();
                app.render_state.mark_dirty();
            }
            Ok(EventResult::Continue)
        }

        KeyAction::CompletionConfirm => {
            app.confirm_completion();
            Ok(EventResult::Continue)
        }

        KeyAction::CompletionCancel => {
            app.completion = None;
            app.render_state.mark_dirty();
            Ok(EventResult::Continue)
        }

        KeyAction::ToggleSelection => {
            if let Some(completion) = app.completion.as_mut() {
                if completion.multi_select {
                    completion.toggle_selection();
                    app.render_state.mark_dirty();
                }
            }
            Ok(EventResult::Continue)
        }

        KeyAction::Passthrough => {
            // Already handled by the fallback logic
            Ok(EventResult::Continue)
        }
    }
}

/// Handle submitted content (check for local commands, then send to agent)
fn handle_submitted_content<B: ratatui::backend::Backend>(
    app: &mut ChatApp,
    content: String,
    message_tx: &mpsc::UnboundedSender<String>,
    terminal: &mut Terminal<B>,
) -> Result<EventResult> {
    let trimmed = content.trim();

    // Handle /clear command locally
    if trimmed == "/clear" {
        handle_clear_command(terminal, app)?;
        return Ok(EventResult::CommandHandled);
    }

    // Handle /exit and /quit commands locally
    if trimmed == "/exit" || trimmed == "/quit" {
        let system_msg = ChatMessageDisplay::system("Exiting chat session...");
        ChatApp::insert_message(terminal, &system_msg)
            .context("failed to insert system message")?;
        app.request_exit();
        return Ok(EventResult::Quit);
    }

    // Display the user message in scrollback
    let user_msg = ChatMessageDisplay::user(&content);
    ChatApp::insert_message(terminal, &user_msg)
        .context("failed to insert user message")?;

    // Send to agent
    message_tx
        .send(content.clone())
        .context("failed to send message to agent")?;

    // Set streaming state
    app.set_streaming(true);
    app.render_state.mark_dirty();

    Ok(EventResult::SendMessage(content))
}
```

**Step 3: Update app.rs to make confirm_completion public**

In app.rs, change:

```rust
fn confirm_completion(&mut self) {
```

to:

```rust
pub fn confirm_completion(&mut self) {
```

**Step 4: Run tests**

Run: `cargo test -p crucible-cli chat_tui`
Expected: All tests pass

**Step 5: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/event_loop.rs crates/crucible-cli/src/chat_tui/app.rs
git commit -m "refactor(chat-tui): use KeyBindings in event loop"
```

---

### Task 4.2: Delete dead code

**Files:**
- Modify: `crates/crucible-cli/src/chat_tui/event_loop.rs`
- Modify: `crates/crucible-cli/src/chat_tui/app.rs`

**Step 1: Remove handle_key_event (duplicate)**

Delete the `handle_key_event` function (around lines 340-357).

**Step 2: Remove event_loop_inner (unused)**

Delete the `event_loop_inner` function (around lines 281-338).

**Step 3: Remove run_event_loop (unused)**

Delete the `run_event_loop` function (around lines 42-64).

**Step 4: Update exports in mod.rs**

Remove `run_event_loop` from the pub use statement in mod.rs if present.

**Step 5: Remove ChatMessage struct if unused**

Check if `ChatMessage` struct is still needed. If not, remove it.

**Step 6: Clean up app.rs handle_key**

The old `handle_key` method in app.rs can be simplified or removed since keybindings are now in event_loop. Keep it for passthrough handling but simplify.

**Step 7: Run tests**

Run: `cargo test -p crucible-cli chat_tui`
Expected: All tests pass (some tests may need adjustment)

**Step 8: Commit**

```bash
git add crates/crucible-cli/src/chat_tui/
git commit -m "refactor(chat-tui): remove dead code and duplicates"
```

---

## Wave 4 QA Checkpoint

Run: `cargo test -p crucible-cli`
Expected: All tests pass

Run: `cargo clippy --workspace`
Expected: No warnings

Run: `cargo build --release`
Expected: Build succeeds

---

## Wave 5: Integration

### Task 5.1: Wire chat_tui into chat command

**Files:**
- Modify: `crates/crucible-cli/src/commands/chat.rs`

**Step 1: Update imports**

Add:

```rust
use crate::chat_tui;
```

**Step 2: Replace run_interactive_session call**

In the `chat` function, replace line 204:

```rust
run_interactive_session(core, &mut client, initial_mode, no_context, context_size, live_progress).await?;
```

with:

```rust
chat_tui::run_with_agent(client).await?;
```

Note: This may require adjusting the client ownership (remove `&mut`).

**Step 3: Run build**

Run: `cargo build -p crucible-cli`
Expected: Build succeeds (may need signature adjustments)

**Step 4: Manual test**

Run: `cargo run -- chat`
Expected: New TUI appears with ratatui viewport

**Step 5: Commit**

```bash
git add crates/crucible-cli/src/commands/chat.rs
git commit -m "feat(cli): wire chat_tui into chat command"
```

---

### Task 5.2: Update WORKTREE.md

**Files:**
- Modify: `WORKTREE.md`

**Step 1: Update status**

Update WORKTREE.md to reflect SOLID refactor completion.

**Step 2: Commit**

```bash
git add WORKTREE.md
git commit -m "docs(worktree): update with SOLID refactor completion"
```

---

## Final QA

Run: `cargo test --workspace`
Expected: All tests pass

Run: `cargo clippy --workspace`
Expected: No warnings

Run: `cargo run -- chat`
Expected: Chat TUI works with new keybindings

---

## Success Criteria Checklist

- [ ] All SOLID principles adhered to
- [ ] Keybindings configurable via KeyBindings struct
- [ ] No hardcoded completion items (uses CompletionSource)
- [ ] No hardcoded keybindings in app.rs/input.rs/event_loop.rs
- [ ] All existing tests pass
- [ ] New tests for keybinding resolution (22+ tests)
- [ ] Platform-agnostic types in crucible-core
- [ ] Chat command uses new TUI

---

## Dependency Graph (Topological Order)

```
Wave 0: Dependencies
  └── Task 0.1: bitflags

Wave 1: Core Types (no dependencies on CLI)
  ├── Task 1.1: KeyCode
  ├── Task 1.2: Modifiers (depends on 1.1 for same file)
  ├── Task 1.3: KeyPattern (depends on 1.1, 1.2)
  ├── Task 1.4: InputMode
  ├── Task 1.5: KeyAction
  ├── Task 1.6: ChatEvent
  ├── Task 1.7: SessionAction (depends on chat.rs ChatMode)
  └── Task 1.8: Re-exports

Wave 2: CLI Keybindings (depends on Wave 1)
  ├── Task 2.1: KeyBindings struct
  ├── Task 2.2: Default keybindings (depends on 2.1)
  └── Task 2.3: crossterm conversion

Wave 3: Refactor ChatApp (depends on Wave 2)
  ├── Task 3.1: Inject KeyBindings
  └── Task 3.2: CompletionSource injection

Wave 4: Refactor Event Loop (depends on Wave 3)
  ├── Task 4.1: Use KeyBindings
  └── Task 4.2: Delete dead code

Wave 5: Integration (depends on Wave 4)
  ├── Task 5.1: Wire to chat command
  └── Task 5.2: Update docs
```

Each wave has a QA checkpoint. Subagents can work in parallel within a wave but must complete before the next wave starts.
