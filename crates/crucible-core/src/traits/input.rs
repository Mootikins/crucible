//! Input abstraction traits for cross-platform UI
//!
//! Provides platform-agnostic key representation and action types
//! that can be shared across CLI, Web, and Desktop interfaces.
//!
//! ## Architecture
//!
//! ```text
//! crossterm::KeyEvent -> KeyPattern -> KeyBindings::resolve() -> KeyAction
//!                                                                    |
//!                                                                    v
//!                                              ChatApp::execute() -> ChatEvent
//! ```
//!
//! - **KeyCode**: Platform-agnostic key representation
//! - **Modifiers**: Bitflags for Shift, Ctrl, Alt, Platform (Cmd/Win)
//! - **KeyPattern**: Key + modifiers for keybinding matching
//! - **InputMode**: Modal state (Normal, Completion, future vim modes)
//! - **KeyAction**: What a keybinding triggers (no data payload)
//! - **ChatEvent**: What the system produces (carries data)
//! - **SessionAction**: Local session operations (/clear, /exit)

use bitflags::bitflags;

use super::chat::ChatMode;

/// Platform-agnostic key code
///
/// Represents a physical or logical key independent of the underlying
/// terminal library (crossterm, termion, etc.) or platform (CLI, web, desktop).
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

bitflags! {
    /// Modifier key flags
    ///
    /// Uses bitflags for efficient storage and combination.
    /// PLATFORM represents Cmd on macOS, Win key on Windows.
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

/// Platform-agnostic key pattern for keybinding matching
///
/// Combines a key code with modifier flags. This is the lookup key
/// for keybinding resolution in `KeyBindings`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    // === KeyCode tests ===

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
        let mut set = HashSet::new();
        set.insert(KeyCode::Enter);
        set.insert(KeyCode::Escape);
        assert!(set.contains(&KeyCode::Enter));
    }

    // === Modifiers tests ===

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

    // === KeyPattern tests ===

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
        let mut map = HashMap::new();
        map.insert(KeyPattern::ctrl('c'), "quit");
        assert_eq!(map.get(&KeyPattern::ctrl('c')), Some(&"quit"));
    }

    // === InputMode tests ===

    #[test]
    fn test_input_mode_default() {
        assert_eq!(InputMode::default(), InputMode::Normal);
    }

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_ne!(InputMode::Normal, InputMode::Completion);
    }

    // === KeyAction tests ===

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

    // === ChatEvent tests ===

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

    // === SessionAction tests ===

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
}
