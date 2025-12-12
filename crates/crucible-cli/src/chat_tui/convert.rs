//! Conversion from crossterm key events to platform-agnostic KeyPattern
//!
//! This module provides the bridge between crossterm's input events
//! and our platform-agnostic KeyPattern type.
//!
//! ## Usage
//!
//! ```ignore
//! use ratatui::crossterm::event::KeyEvent;
//! use crate::chat_tui::convert::key_event_to_pattern;
//!
//! fn handle_key(event: KeyEvent) {
//!     let pattern = key_event_to_pattern(event);
//!     // Now use pattern with KeyBindings::resolve()
//! }
//! ```

use crucible_core::traits::{KeyCode, KeyPattern, Modifiers};
use ratatui::crossterm::event::{KeyCode as CtKeyCode, KeyEvent, KeyModifiers};

/// Convert crossterm KeyCode to our KeyCode
pub fn keycode_from_crossterm(key: CtKeyCode) -> KeyCode {
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

/// Convert crossterm KeyModifiers to our Modifiers
pub fn modifiers_from_crossterm(mods: KeyModifiers) -> Modifiers {
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

/// Convert crossterm KeyEvent to KeyPattern
pub fn key_event_to_pattern(event: KeyEvent) -> KeyPattern {
    KeyPattern {
        key: keycode_from_crossterm(event.code),
        modifiers: modifiers_from_crossterm(event.modifiers),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === KeyCode conversion tests ===

    #[test]
    fn test_keycode_conversion_char() {
        let ct = CtKeyCode::Char('a');
        let kc = keycode_from_crossterm(ct);
        assert_eq!(kc, KeyCode::Char('a'));
    }

    #[test]
    fn test_keycode_conversion_special() {
        assert_eq!(keycode_from_crossterm(CtKeyCode::Enter), KeyCode::Enter);
        assert_eq!(keycode_from_crossterm(CtKeyCode::Esc), KeyCode::Escape);
        assert_eq!(
            keycode_from_crossterm(CtKeyCode::Backspace),
            KeyCode::Backspace
        );
        assert_eq!(keycode_from_crossterm(CtKeyCode::Tab), KeyCode::Tab);
        assert_eq!(keycode_from_crossterm(CtKeyCode::Delete), KeyCode::Delete);
    }

    #[test]
    fn test_keycode_conversion_arrows() {
        assert_eq!(keycode_from_crossterm(CtKeyCode::Up), KeyCode::Up);
        assert_eq!(keycode_from_crossterm(CtKeyCode::Down), KeyCode::Down);
        assert_eq!(keycode_from_crossterm(CtKeyCode::Left), KeyCode::Left);
        assert_eq!(keycode_from_crossterm(CtKeyCode::Right), KeyCode::Right);
    }

    #[test]
    fn test_keycode_conversion_navigation() {
        assert_eq!(keycode_from_crossterm(CtKeyCode::Home), KeyCode::Home);
        assert_eq!(keycode_from_crossterm(CtKeyCode::End), KeyCode::End);
        assert_eq!(keycode_from_crossterm(CtKeyCode::PageUp), KeyCode::PageUp);
        assert_eq!(
            keycode_from_crossterm(CtKeyCode::PageDown),
            KeyCode::PageDown
        );
    }

    #[test]
    fn test_keycode_conversion_function() {
        assert_eq!(keycode_from_crossterm(CtKeyCode::F(1)), KeyCode::F(1));
        assert_eq!(keycode_from_crossterm(CtKeyCode::F(12)), KeyCode::F(12));
    }

    // === Modifiers conversion tests ===

    #[test]
    fn test_modifiers_conversion_none() {
        let ct = KeyModifiers::NONE;
        let m = modifiers_from_crossterm(ct);
        assert_eq!(m, Modifiers::NONE);
    }

    #[test]
    fn test_modifiers_conversion_control() {
        let ct = KeyModifiers::CONTROL;
        let m = modifiers_from_crossterm(ct);
        assert!(m.contains(Modifiers::CONTROL));
        assert!(!m.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_modifiers_conversion_shift() {
        let ct = KeyModifiers::SHIFT;
        let m = modifiers_from_crossterm(ct);
        assert!(m.contains(Modifiers::SHIFT));
        assert!(!m.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_modifiers_conversion_alt() {
        let ct = KeyModifiers::ALT;
        let m = modifiers_from_crossterm(ct);
        assert!(m.contains(Modifiers::ALT));
        assert!(!m.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_modifiers_conversion_combined() {
        let ct = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        let m = modifiers_from_crossterm(ct);
        assert!(m.contains(Modifiers::CONTROL));
        assert!(m.contains(Modifiers::SHIFT));
        assert!(!m.contains(Modifiers::ALT));
    }

    // === KeyEvent conversion tests ===

    #[test]
    fn test_key_event_conversion() {
        let event = KeyEvent::new(CtKeyCode::Char('c'), KeyModifiers::CONTROL);
        let pattern = key_event_to_pattern(event);

        assert_eq!(pattern.key, KeyCode::Char('c'));
        assert!(pattern.modifiers.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_key_event_conversion_no_modifiers() {
        let event = KeyEvent::new(CtKeyCode::Enter, KeyModifiers::NONE);
        let pattern = key_event_to_pattern(event);

        assert_eq!(pattern.key, KeyCode::Enter);
        assert_eq!(pattern.modifiers, Modifiers::NONE);
    }

    #[test]
    fn test_key_event_conversion_multiple_modifiers() {
        let event = KeyEvent::new(
            CtKeyCode::Char('x'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        let pattern = key_event_to_pattern(event);

        assert_eq!(pattern.key, KeyCode::Char('x'));
        assert!(pattern.modifiers.contains(Modifiers::CONTROL));
        assert!(pattern.modifiers.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_key_event_preserves_character_case() {
        // Shift+a should produce 'A' on most terminals
        let event = KeyEvent::new(CtKeyCode::Char('A'), KeyModifiers::SHIFT);
        let pattern = key_event_to_pattern(event);

        assert_eq!(pattern.key, KeyCode::Char('A'));
        assert!(pattern.modifiers.contains(Modifiers::SHIFT));
    }
}
